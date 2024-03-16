pub mod bin_diff;

use std::{collections::HashMap, sync::OnceLock};

use ed25519_dalek::{pkcs8::DecodePublicKey, Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseIndexV1 {
    pub branches: HashMap<String, Release>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub file: Resource,
}

///
/// 例如 file = `Resource { path: "foo/bar.txt", hash: "deadbeef" }`;
///
/// 得到 `"foo/bar.txt/deadbeef"` （hash会被裁剪到32个字符长度）
///
pub fn file_path(file: &Resource) -> String {
    let hash = &file.hash[0..32.min(file.hash.len())];
    if file.path.ends_with('/') {
        format!("{}{}", &file.path, hash)
    } else {
        format!("{}/{}", &file.path, hash)
    }
}

///
/// 例如 file = `Resource { path: "foo/bar.txt", hash: "deadbeef" }`; source_hash = `"c0ffee"`
///
/// 得到 `"foo/bar.txt/c0ffee-deadbeef.bspatch"` （hash会被裁剪到32个字符长度）
///
pub fn file_bspatch_path(file: &Resource, source_hash: &str) -> String {
    let source_hash = &source_hash[0..32.min(source_hash.len())];
    let hash = &file.hash[0..32.min(file.hash.len())];
    if file.path.ends_with('/') {
        format!("{}{}-{}.bspatch", &file.path, source_hash, hash)
    } else {
        format!("{}/{}-{}.bspatch", &file.path, source_hash, hash)
    }
}

pub fn release_public_key() -> &'static VerifyingKey {
    static RELEASE_PUB_KEY: OnceLock<VerifyingKey> = OnceLock::new();
    RELEASE_PUB_KEY.get_or_init(|| {
        VerifyingKey::from_public_key_pem(consts::RELEASE_PUB_KEY_PKCS8)
            .expect("invalid public key PEM")
    })
}

pub fn try_parse_signature(bytes: &[u8]) -> signature::Result<Signature> {
    Signature::from_slice(bytes)
}

pub mod consts {
    pub const DEFAULT_BRANCH: &str = "main";
    pub const TMP_PATCH_EXECUTABLE_NAME: &str = "closure-studio.__exe_patch__.tmp";
    pub const RELEASE_PUB_KEY_PKCS8: &str = include_str!("../resource/release.pub");

    pub mod url {
        pub mod asset {
            pub mod ui_ota_v1 {
                pub const INDEX: &str = "ui/ota/v1/index.json";
                pub const INDEX_SIG: &str = "ui/ota/v1/index.json.sig";
                pub const FILES: &str = "ui/ota/v1/";
            }
        }
    }
}
