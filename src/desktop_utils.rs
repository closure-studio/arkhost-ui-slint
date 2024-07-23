use crate::app;

pub fn alloc_console() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::System::Console::AllocConsole;
        let result = AllocConsole();
        if result != 0 {
            println!("[alloc_console] AllocConsole() success ");
        } else {
            println!(
                "[alloc_console] Error calling AllocConsole(): {:#x}",
                GetLastError()
            );
        }
    }
}

pub fn attach_console() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let result = AttachConsole(ATTACH_PARENT_PROCESS);
        if result != 0 {
            println!("[attach_console] AttachConsole(ATTACH_PARENT_PROCESS) success ");
        } else {
            println!(
                "[attach_console] Error calling AttachConsole(ATTACH_PARENT_PROCESS): {:#x}",
                GetLastError()
            );
        }
    }
}

pub fn show_console(visible: bool) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOW};
        let hwnd = GetConsoleWindow();

        if hwnd == 0 {
            println!("[show_console] hWnd is NULL");
            return;
        }

        _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
    }
}

pub fn show_crash_window(exit_status: &str, error_info: &str) {
    show_console(true);
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::{
            GetStdHandle, SetConsoleTextAttribute, FOREGROUND_RED, STD_OUTPUT_HANDLE,
        };
        let hconsole = GetStdHandle(STD_OUTPUT_HANDLE);
        _ = SetConsoleTextAttribute(hconsole, FOREGROUND_RED);
    }

    println!(
        "\n********************************************************************************\n"
    );
    println!(
        concat!(
            "可露希尔罢工了！\n",
            "错误信息：{}\n",
            "- APP 版本号\t: {}\n",
            "- SHA256\t: {}\n",
            "- ExitStatus\t: {}\n",
            "\n",
            "如果发生反复崩溃无法使用、功能异常等问题，\n请截图控制台输出，反馈至可露希尔QQ群或QQ频道“PRTS接入 - APP讨论”板块。\n",
            "重新启动客户端前，请关闭该控制台窗口……\n"
        ),
        error_info,
        app::utils::app_metadata::CARGO_PKG_VERSION.unwrap_or("not found"),
        app::utils::app_metadata::executable_sha256().map_or("unable to hash".into(), |x| hex::encode(*x)),
        exit_status
    );
    println!(
        "\n********************************************************************************\n"
    );

    loop {
        use std::io::Read;
        _ = std::io::stdin().read(&mut [0u8]);
    }
}

pub fn on_duplicated_instance() {
    #[cfg(target_os = "windows")]
    unsafe {
        // TODO: 根据进程查找
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FindWindowW, GetWindowPlacement, SetForegroundWindow, ShowWindow, SHOW_WINDOW_CMD,
            SW_NORMAL, SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, WINDOWPLACEMENT,
        };
        let window_name: Vec<u16> = consts::WINDOWS_TITLE.encode_utf16().chain([0]).collect();
        let hwnd = FindWindowW(std::ptr::null(), window_name.as_ptr());
        if hwnd != 0 {
            let mut place: WINDOWPLACEMENT = core::mem::zeroed();
            GetWindowPlacement(hwnd, &mut place);
            let show_cmd: SHOW_WINDOW_CMD = match place.showCmd as i32 {
                SW_SHOWMAXIMIZED => SW_SHOWMAXIMIZED,
                SW_SHOWMINIMIZED => SW_RESTORE,
                _ => SW_NORMAL,
            };
            ShowWindow(hwnd, show_cmd);
            SetForegroundWindow(hwnd);
        }
    }
}

pub async fn update_client_if_exist() -> anyhow::Result<()> {
    use sha2::Digest;
    use tokio::io::AsyncBufReadExt;

    let pending_update = app::ota::pending_update()
        .map_err(|e| {
            println!("[update_client_if_exist] Error reading pending update record from DB: {e}");
        })
        .ok()
        .flatten();

    let pending_update = match pending_update {
        Some(pending_update) => pending_update,
        None => return Ok(()),
    };
    println!(
        "[update_client_if_exist] Found pending update: {}",
        &pending_update.version
    );

    let file_path = match &pending_update.binary.blob {
        app::ota::Blob::File(file_path) => file_path,
        #[allow(unused)]
        _ => anyhow::bail!("不支持更新数据类型，请提交Bug"),
    };

    if !matches!(tokio::fs::try_exists(file_path).await, Ok(true)) {
        return Ok(());
    }

    {
        let release_file = tokio::fs::File::open(file_path).await?;
        let mut reader = tokio::io::BufReader::new(release_file);
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

        if hasher.finalize()[..] != pending_update.binary.sha256[..] {
            anyhow::bail!("更新未完整下载或校验错误，请重试");
        }
    }

    if let Err(e) = self_replace::self_replace(file_path) {
        anyhow::bail!(format!(
            "无法替换旧客户端程序文件，请重新运行更新或手动使用新客户端文件覆盖旧客户端\n新客户端路径：{}\n错误：{e}",
            file_path.display()
        ));
    }

    _ = tokio::fs::remove_file(file_path).await;
    _ = app::ota::remove_pending_update();
    app::utils::notification::toast("可露希尔客户端更新成功！", None, "", None);
    Ok(())
}

#[allow(unused)]
pub mod consts {
    pub const WINDOWS_TITLE: &str = "Closure Studio";
}
