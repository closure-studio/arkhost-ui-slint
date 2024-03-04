use std::io;

use qbsdiff::{Bsdiff, Bspatch};

pub fn bsdiff(source: &[u8], target: &[u8]) -> io::Result<bytes::Bytes> {
    let mut patch = Vec::new();
    Bsdiff::new(source, target).compare(io::Cursor::new(&mut patch))?;
    Ok(patch.into())
}

pub fn bspatch(source: &[u8], patch: &[u8]) -> io::Result<bytes::Bytes> {
    let patcher = Bspatch::new(patch)?;
    let mut target = Vec::with_capacity(patcher.hint_target_size() as usize);
    patcher.apply(source, io::Cursor::new(&mut target))?;
    Ok(target.into())
}

pub fn bspatch_check_integrity(
    source: &[u8],
    patch: &[u8],
    hash: &[u8],
    mut hasher: impl digest::Digest,
) -> io::Result<bytes::Bytes> {
    let target = bspatch(source, patch)?;
    hasher.update(&target);
    let actual_hash = hasher.finalize();
    if actual_hash.as_slice() == hash {
        Ok(target)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "failed to verify patched binary integrity, expected: {}; got: {};",
                hex::encode(hash),
                hex::encode(actual_hash)
            ),
        ))
    }
}

pub fn bspatch_filename(source_hash: &[u8], target_hash: &[u8]) -> String {
    format!(
        "{}-{}.bspatch",
        hex::encode(&source_hash[0..source_hash.len().min(consts::PATCH_FILENAME_HASH_BYTES)]),
        hex::encode(&target_hash[0..target_hash.len().min(consts::PATCH_FILENAME_HASH_BYTES)])
    )
}

pub mod consts {
    pub const PATCH_FILENAME_HASH_BYTES: usize = 16;
}
