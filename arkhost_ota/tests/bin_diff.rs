#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use arkhost_ota::bin_diff::*;

    fn testing_binaries() -> (bytes::Bytes, bytes::Bytes) {
        (
            b"hello world".repeat(42).into(),
            b"Hello world".repeat(43).into(),
        )
    }

    fn testing_hashes(
        source: &[u8],
        target: &[u8],
    ) -> (digest::Output<Sha256>, digest::Output<Sha256>) {
        let source_hash = {
            let mut hasher = Sha256::new();
            hasher.update(source);
            hasher.finalize()
        };

        let target_hash = {
            let mut hasher = Sha256::new();
            hasher.update(target);
            hasher.finalize()
        };

        (source_hash, target_hash)
    }

    #[test]
    fn test_patch() {
        let (source, target) = testing_binaries();
        let (_, target_hash) = testing_hashes(&source, &target);
        let patch = bsdiff(&source, &target).unwrap();
        let actual_target =
            bspatch_check_integrity(&source, &patch, &target_hash, Sha256::new()).unwrap();
        assert_eq!(actual_target, target);
    }

    #[test]
    fn test_patch_filename() {
        let (source, target) = testing_binaries();
        let (source_hash, target_hash) = testing_hashes(&source, &target);

        assert_eq!(
            bspatch_filename(&source_hash, &target_hash),
            "456bbefe515d6e82eda993e5e4b9cfe8-69a2ed695177ed187db406e44ffafd6d.bspatch"
        );
    }
}