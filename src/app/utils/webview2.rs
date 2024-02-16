use std::{ffi::OsStr, io};

use anyhow::anyhow;
use winreg::{enums::*, RegKey};

pub fn test_installation_ver() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 64-bit Windows
    if let Ok(ver) = test_installation_ver_reg(
        &hklm,
        r#"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"#,
        r#"pv"#,
    ) {
        return Some(ver);
    }

    if let Ok(ver) = test_installation_ver_reg(
        &hkcu,
        r#"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"#,
        r#"pv"#,
    ) {
        return Some(ver);
    }

    // 32-bit Windows
    if let Ok(ver) = test_installation_ver_reg(
        &hklm,
        r#"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"#,
        r#"pv"#,
    ) {
        return Some(ver);
    }

    if let Ok(ver) = test_installation_ver_reg(
        &hkcu,
        r#"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"#,
        r#"pv"#,
    ) {
        return Some(ver);
    }

    None
}

fn test_installation_ver_reg(
    reg_key: &RegKey,
    path: impl AsRef<OsStr>,
    name: impl AsRef<OsStr>,
) -> io::Result<String> {
    let ver: String = reg_key.open_subkey(path)?.get_value(name)?;
    if !ver.is_empty() && ver != "0.0.0.0" {
        Ok(ver)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            anyhow!("invalid WebView2 version or not found"),
        ))
    }
}
