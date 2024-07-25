use http_cache::{CacheMode, CacheModeFn};
use log::trace;
use std::sync::{Arc, OnceLock};

pub fn default_cache_mode_fn() -> CacheModeFn {
    static DEFAULT_CACHE_MODE_FN: OnceLock<CacheModeFn> = OnceLock::new();
    DEFAULT_CACHE_MODE_FN
        .get_or_init(|| {
            Arc::new(|req| {
                if matches!(req.method.as_str(), "HEAD" | "OPTIONS") {
                    trace!("NoStore for: {} {}", req.method, req.uri);
                    return CacheMode::NoStore;
                }

                // TODO: 其他方式识别资源文件类型（MIME type等）
                if req.uri.path().ends_with(".webp") || req.uri.path().ends_with(".png") {
                    trace!("ForceCache for: {} {}", req.method, req.uri);
                    return CacheMode::ForceCache;
                }
                let matches_ota_file = {
                    // OTA 更新文件URL： http://asset.server.com/foo/bar.txt/{hash}
                    let mut split = req.uri.path().rsplitn(2, '/');
                    matches!(
                    (split.next(), split.next()), 
                        (Some(hash_versioned_file), Some(hash_version_dir)) if
                            (hash_version_dir.ends_with(".exe")
                            || hash_versioned_file.ends_with(".bspatch")))
                    // TODO: 其他方式识别OTA更新文件
                };
                if matches_ota_file {
                    trace!("NoStore for OTA File: {} {}", req.method, req.uri);
                    CacheMode::NoStore
                } else {
                    trace!("Default for: {} {}", req.method, req.uri);
                    CacheMode::Default
                }
            })
        })
        .clone()
}
