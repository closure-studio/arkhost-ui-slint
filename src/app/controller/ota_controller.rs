use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{env, io};

use anyhow::bail;
use arkhost_ota;
use bytes::Bytes;
use futures::{Stream, StreamExt, TryFutureExt};
use sha2::Digest;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::app::utils::{app_metadata, data_dir, notification};
use crate::app::{asset_worker, ui};

use super::app_state_controller::AppStateController;
use super::{AssetCommand, Sender};

pub struct OtaController {
    app_state_controller: Arc<AppStateController>,
    sender: Arc<Sender>,

    cur_update_mode: Mutex<Option<asset_worker::ReleaseUpdateType>>,
    updating: AtomicBool,
}

impl OtaController {
    pub fn new(app_state_controller: Arc<AppStateController>, sender: Arc<Sender>) -> Self {
        Self {
            app_state_controller,
            sender,

            cur_update_mode: Mutex::new(None),
            updating: false.into(),
        }
    }

    pub async fn check_release_update(&self) {
        let mut attempts = [
            asset_worker::ReleaseUpdateType::Patch,
            asset_worker::ReleaseUpdateType::FullDownload,
        ]
        .into_iter();
        let (mode, release, download_size) = loop {
            let mode = match attempts.next() {
                Some(mode) => mode,
                None => return,
            };

            let (resp, mut rx) = oneshot::channel();
            let branch = app_metadata::RELEASE_UPDATE_BRANCH;
            match self
                .sender
                .send_asset_request(
                    AssetCommand::CheckReleaseUpdate {
                        branch: Some(branch.to_owned()),
                        mode,
                        resp,
                    },
                    &mut rx,
                )
                .await
            {
                Ok(None) => {
                    println!("[OTA] No updates from branch '{branch}'.");
                    return;
                }
                Ok(Some((release, download_size, _))) => break (mode, release, download_size),
                Err(e) => {
                    println!("[OTA] Unable to check update from branch '{branch}' with type {mode:?}: {e}");
                    continue;
                }
            }
        };

        self.update_release_info(mode, &release, download_size);
        _ = self.cur_update_mode.lock().await.insert(mode);
    }

    pub async fn try_auto_update_release(&self) {
        let mode = *self.cur_update_mode.lock().await;
        if matches!(mode, Some(asset_worker::ReleaseUpdateType::Patch)) {
            self.update_release().await;
        }
    }

    pub async fn update_release(&self) {
        if self
            .updating
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_update_state(ui::ReleaseUpdateState::Downloading);
            })
        });
        let update_state = if let Err(e) = self.update_release_inner().await {
            println!("[OTA] Error updating release: {e}");
            ui::ReleaseUpdateState::Idle
        } else {
            ui::ReleaseUpdateState::Ready
        };

        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_update_state(update_state);
            })
        });

        self.updating.store(false, Ordering::Release);
    }

    async fn update_release_inner(&self) -> anyhow::Result<()> {
        let mode = match self.cur_update_mode.lock().await.as_ref() {
            Some(mode) => *mode,
            None => bail!("no update type specified"),
        };

        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_update_downloaded_size("--".into());
                x.set_update_indeterminate(true);
            })
        });

        let branch = app_metadata::RELEASE_UPDATE_BRANCH;
        let (resp, mut rx) = oneshot::channel();
        let (release, download_size, asset_path, response) = self
            .sender
            .send_asset_request(
                AssetCommand::DownloadReleaseUpdate {
                    branch: Some(branch.to_owned()),
                    mode,
                    resp,
                },
                &mut rx,
            )
            .await?;
        self.update_release_info(mode, &release, download_size);
        let target_file_path = data_dir::data_dir().join(arkhost_ota::consts::TMP_PATCH_EXECUTABLE_NAME);
        let target_hash = hex::decode(&release.file.hash)?;

        let download_file_path = match mode {
            asset_worker::ReleaseUpdateType::Patch => {
                let mut split = asset_path.trim_end_matches('/').rsplitn(3, '/');
                let file_name = match (split.next(), split.next(), split.next()) {
                    (Some(_), Some(file_name), _) => file_name.into(),
                    _ => format!(
                        "release-{}.{}.{}-{}.tmp",
                        release.version.major,
                        release.version.minor,
                        release.version.patch,
                        chrono::Utc::now().timestamp_micros()
                    ),
                };
                data_dir::data_dir().join(file_name)
            }
            asset_worker::ReleaseUpdateType::FullDownload => target_file_path.clone(),
        };
        println!("[OTA] Download file path: {}", download_file_path.display());

        if !download_file_exists(mode, &download_file_path, &target_hash).await {
            self.try_download_and_save(
                mode,
                response.bytes_stream().boxed(),
                &download_file_path,
                download_size,
                &target_hash,
            )
            .await?;
        } else {
            println!(
                "[OTA] Skipping downloading on existing tmp file hash matches target hash: {}",
                download_file_path.display()
            );
        }

        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_update_indeterminate(true);
            })
        });

        match mode {
            asset_worker::ReleaseUpdateType::Patch => {
                self.try_patch_self_executable(&download_file_path, &target_file_path, target_hash)
                    .await?;
            }
            asset_worker::ReleaseUpdateType::FullDownload => {}
        };

        // AppWindow退出后main()中的父进程检测TMP_PATCH_EXECUTABLE_NAME存在，并自我替换
        Ok(())
    }

    async fn try_download_and_save<S: Stream<Item = reqwest::Result<Bytes>> + Unpin>(
        &self,
        mode: asset_worker::ReleaseUpdateType,
        mut stream: S,
        download_file_path: &Path,
        download_size: usize,
        target_hash: &[u8],
    ) -> Result<(), anyhow::Error> {
        let mut file = match tokio::fs::File::create(download_file_path).await {
            Ok(file) => file,
            Err(e) => {
                notification::toast(
                    "更新失败",
                    None,
                    &format!(
                        "无法创建临时文件！请检查权限是否正确\n路径：{}\n{e}",
                        download_file_path.display()
                    ),
                    None,
                );
                return Err(e.into());
            }
        };
        let (tx_writer, mut rx_writer) = mpsc::unbounded_channel::<Bytes>();
        let writer: JoinHandle<io::Result<()>> = tokio::spawn(async move {
            while let Some(chunk) = rx_writer.recv().await {
                if chunk.is_empty() {
                    break;
                }
                file.write_all(&chunk).await?;
            }
            file.shutdown().await
        });
        let mut bytes_read = 0;
        let mut last_recorded_size = 0;
        let mut hasher = sha2::Sha256::new();
        while let Some(data) = stream.next().await {
            let data = match data {
                Ok(chunk) => chunk,
                Err(e) => {
                    notification::toast("更新失败", None, &format!("下载失败！请重试\n{e}"), None);
                    _ = tx_writer.send(vec![].into());
                    _ = writer.await;
                    _ = tokio::fs::remove_file(download_file_path).await;
                    return Err(e.into());
                }
            };

            bytes_read += data.len();
            hasher.update(&data);
            tx_writer.send(data)?;
            self.update_download_progress(download_size, bytes_read, &mut last_recorded_size);
        }
        println!(
            "[OTA] Download finished. {} read",
            humansize::format_size(bytes_read, humansize::DECIMAL)
        );
        _ = tx_writer.send(vec![].into());
        if let Err(e) = writer.await {
            notification::toast(
                "更新失败",
                None,
                &format!(
                    "写入临时文件失败！请尝试重新下载\n路径：{}\n{e}",
                    download_file_path.display()
                ),
                None,
            );
            _ = tokio::fs::remove_file(download_file_path).await;
            return Err(e.into());
        }
        let hash = hasher.finalize();
        if matches!(mode, asset_worker::ReleaseUpdateType::FullDownload)
            && hash[..] != target_hash[..]
        {
            notification::toast("更新失败", None, "哈希值校验失败！请尝试重新下载", None);
            _ = tokio::fs::remove_file(download_file_path).await;
            bail!(
                "failed to verify downloaded file {download_file_path:?} integrity: expected: {}; downloaded: {}",
                &hex::encode(hash),
                &hex::encode(target_hash)
            );
        };
        Ok(())
    }

    async fn try_patch_self_executable(
        &self,
        patch_file_path: &Path,
        target_file_path: &Path,
        target_hash: Vec<u8>,
    ) -> Result<(), anyhow::Error> {
        let (mut patch_file, mut target_file) = match (
            tokio::fs::File::open(&patch_file_path).await,
            tokio::fs::File::create(&target_file_path).await,
        ) {
            (Ok(x), Ok(y)) => (x, y),
            (r1, r2) => {
                notification::toast(
                    "更新失败",
                    None,
                    &format!(
                        "无法操作临时文件！请检查权限是否正确\n\t{}\n{r1:?}\n\t{}\n{r2:?}",
                        patch_file_path.display(),
                        target_file_path.display()
                    ),
                    None,
                );
                r1?;
                r2?;
                bail!(""); // unreachable
            }
        };
        let mut patch_bytes = Vec::new();
        patch_file.read_to_end(&mut patch_bytes).await?;
        let mut source = tokio::fs::File::open(env::current_exe()?).await?;
        let mut source_bytes = Vec::new();
        source.read_to_end(&mut source_bytes).await?;
        let target_bytes = match arkhost_ota::bin_diff::bspatch_check_integrity(
            &source_bytes,
            &patch_bytes,
            &target_hash,
            sha2::Sha256::new(),
        ) {
            Ok(x) => x,
            Err(e) => {
                notification::toast(
                    "更新失败",
                    None,
                    &format!("进行增量更新失败！请重试\n{e}",),
                    None,
                );
                _ = self
                    .cur_update_mode
                    .lock()
                    .await
                    .insert(asset_worker::ReleaseUpdateType::FullDownload);
                return Err(e.into());
            }
        };
        if let Err(e) = target_file.write_all(&target_bytes).await {
            notification::toast(
                "更新失败",
                None,
                &format!("写入增量更新到新版本客户端程序失败！请检查权限是否正确\n{e}",),
                None,
            );
            return Err(e.into());
        };
        Ok(())
    }

    fn update_release_info(
        &self,
        mode: asset_worker::ReleaseUpdateType,
        release: &arkhost_ota::Release,
        download_size: usize,
    ) {
        let update_version = release.version.to_string();

        let update_type = match mode {
            asset_worker::ReleaseUpdateType::Patch => ui::ReleaseUpdateType::Patch,
            asset_worker::ReleaseUpdateType::FullDownload => ui::ReleaseUpdateType::FullDownload,
        };

        let update_size = match download_size {
            0 => "未知大小".into(),
            sz => humansize::format_size(sz, humansize::DECIMAL),
        };

        self.app_state_controller.exec(move |x| {
            x.state_globals(move |x| {
                x.set_update_version(update_version.into());
                x.set_update_type(update_type);
                x.set_update_size(update_size.into());
            })
        });
    }

    fn update_download_progress(
        &self,
        total_size: usize,
        downloaded_size: usize,
        last_recorded_size: &mut usize,
    ) {
        if downloaded_size < total_size
            && (downloaded_size - *last_recorded_size)
                < (match total_size {
                    0 => 50 << 17, // 50 KB
                    total_size => total_size,
                } >> 7)
        // * (1/128)
        {
            return;
        }
        *last_recorded_size = downloaded_size;

        let downloaded_size_text = humansize::format_size(downloaded_size, humansize::DECIMAL);
        if total_size != 0 {
            self.app_state_controller.exec(move |x| {
                x.state_globals(move |x| {
                    x.set_update_downloaded_size(downloaded_size_text.into());
                    x.set_update_progress(downloaded_size as f32 / total_size as f32);
                    x.set_update_indeterminate(false);
                })
            });
        } else {
            self.app_state_controller.exec(move |x| {
                x.state_globals(move |x| {
                    x.set_update_downloaded_size(downloaded_size_text.into());
                    x.set_update_indeterminate(true);
                })
            });
        }
    }
}

async fn download_file_exists(
    mode: asset_worker::ReleaseUpdateType,
    download_file_path: &std::path::PathBuf,
    target_hash: &Vec<u8>,
) -> bool {
    matches!(mode, asset_worker::ReleaseUpdateType::FullDownload)
        && matches!(tokio::fs::try_exists(&download_file_path).await, Ok(true))
        && tokio::fs::File::open(download_file_path)
            .and_then(|f| async move {
                let mut reader = tokio::io::BufReader::new(f);
                let mut hasher = sha2::Sha256::new();
                let mut buf;
                while {
                    buf = reader.fill_buf().await?;
                    !buf.is_empty()
                } {
                    hasher.update(buf);
                    let len = buf.len();
                    reader.consume(len);
                }

                Ok(hasher.finalize())
            })
            .await
            .ok()
            .map_or(false, |x| x[..] == *target_hash)
}
