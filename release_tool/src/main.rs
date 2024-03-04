use std::{
    collections::HashMap,
    env,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};

use argh::FromArgs;
use arkhost_ota::{Release, ReleaseIndexV1, Resource};
use cargo::{
    self,
    core::{compiler::CompileMode, Shell, Workspace},
    ops::CompileOptions,
    Config,
};
use sha2::Digest;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, FromArgs)]
/// CLI发布工具
pub struct ProgramOptions {}

#[tokio::main]
async fn main() {
    let cwd = env::current_dir().expect("invalid env::current_dir()");
    let pervious_versions_dir = cwd.join("pervious_versions/");
    tokio::fs::create_dir_all(&pervious_versions_dir)
        .await
        .unwrap();
    let dst_dir = cwd.join("dst/ui/ota/v1/");
    let dst_index_path = dst_dir.join("index.json");

    println!("Invoking cargo build");
    let cargo_home = PathBuf::from(env::var("CARGO_HOME").unwrap());
    let cfg = &Config::new(Shell::new(), cwd.clone(), cargo_home);
    let ws = Workspace::new(&cwd.join("Cargo.toml"), cfg).unwrap();

    let mut compile_opts = CompileOptions::new(cfg, CompileMode::Build).unwrap();
    compile_opts.build_config.requested_profile = "release".into();

    let compilation = cargo::ops::compile(&ws, &compile_opts).unwrap();
    let release_path = compilation
        .binaries
        .iter()
        .find_map(|x| {
            if x.path
                .file_stem()
                .map(|x| x == consts::BINARY_TARGET)
                .is_some_and(|x| x)
            {
                Some(x.path.clone())
            } else {
                None
            }
        })
        .unwrap();
    let release_file_name = release_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let (release_bytes, release_hash) = read_file_with_hash(&release_path).await.unwrap();
    println!("Release SHA256: {}", hex::encode(release_hash));

    println!("Cleaning dst directory");
    _ = tokio::fs::remove_dir_all(&dst_dir).await;
    tokio::fs::create_dir_all(&dst_dir).await.unwrap();

    println!("Deploying release binary to hash versioning directory");
    let dst_hash_versions_dir = dst_dir.join(PathBuf::from(&format!("{release_file_name}/")));
    tokio::fs::create_dir_all(&dst_hash_versions_dir)
        .await
        .unwrap();
    let dst_executable_path = dst_hash_versions_dir.join(hex::encode(&release_hash[0..16]));
    tokio::fs::copy(&release_path, &dst_executable_path)
        .await
        .unwrap();

    println!("Creating patches for pervious versions");
    let mut read_dir = tokio::fs::read_dir(&pervious_versions_dir).await.unwrap();
    // TODO: 并行化生成patch
    while let Some(entry) = read_dir.next_entry().await.unwrap() {
        if matches!(entry.file_type().await, Ok(file_type) if file_type.is_file()) {
            let (perv_version_bytes, perv_version_hash) =
                read_file_with_hash(&entry.path()).await.unwrap();

            println!(
                "Generating patch for pervious version:\n\tSHA256: {}",
                hex::encode(&perv_version_hash)
            );
            if perv_version_hash == release_hash {
                println!("\tFile hash matches release hash, skipping");
                continue;
            }

            let patch_path = dst_hash_versions_dir.join(&arkhost_ota::bin_diff::bspatch_filename(
                &perv_version_hash,
                &release_hash,
            ));
            tokio::fs::write(
                &patch_path,
                &arkhost_ota::bin_diff::bsdiff(&perv_version_bytes, &release_bytes).unwrap(),
            )
            .await
            .unwrap();
        }
    }

    println!("Creating index");
    let mut branches = HashMap::<String, Release>::new();
    branches.insert(
        arkhost_ota::consts::DEFAULT_BRANCH.to_owned(),
        Release {
            version: ws.current().unwrap().version().clone(),
            file: Resource {
                path: release_file_name,
                hash: hex::encode(release_hash),
            },
        },
    );

    tokio::fs::write(
        &dst_index_path,
        serde_json::ser::to_vec(&ReleaseIndexV1 { branches }).unwrap(),
    )
    .await
    .unwrap();

    println!("Done");
}

async fn read_file_with_hash(
    path: &Path,
) -> anyhow::Result<(Vec<u8>, digest::Output<sha2::Sha256>)> {
    let mut hasher = sha2::Sha256::new();
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::with_capacity(file.metadata().await?.file_size() as usize);
    let mut reader = BufReader::new(file);

    let mut buf;
    while {
        buf = reader.fill_buf().await?;
        !buf.is_empty()
    } {
        hasher.update(buf);
        bytes.extend_from_slice(buf);
        let len = buf.len();
        reader.consume(len);
    }

    Ok((bytes, hasher.finalize()))
}

pub(crate) mod consts {
    pub const BINARY_TARGET: &str = "closure-studio";
}
