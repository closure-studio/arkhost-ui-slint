#[cfg(test)]
pub mod tests {
    use arkhost_ota::*;
    use ed25519_dalek::{pkcs8::DecodePublicKey, Signature, VerifyingKey};

    #[test]
    pub fn test_file_url() {
        let file = Resource {
            path: "foo/bar.txt".into(),
            hash: "deadbeef".into(),
        };

        assert_eq!(file_path(&file), "foo/bar.txt/deadbeef");

        let file = Resource {
            path: "bar.txt".into(),
            hash: "2ac32e4e6b64d0c53a4dd9bbca50565e59d89d8f63e9192528a9a996e149e095".into(),
        };

        assert_eq!(file_path(&file), "bar.txt/2ac32e4e6b64d0c53a4dd9bbca50565e");
    }

    #[test]
    pub fn test_file_bspatch() {
        let file = Resource {
            path: "foo/bar.txt".into(),
            hash: "2ac32e4e6b64d0c53a4dd9bbca50565e59d89d8f63e9192528a9a996e149e095".into(),
        };
        let source_hash = "4a25c063ed412cf03617cc19df33e2469c41b3a03d2b506a192edde8eae519c8";

        assert_eq!(
            file_bspatch_path(&file, source_hash),
            "foo/bar.txt/4a25c063ed412cf03617cc19df33e246-2ac32e4e6b64d0c53a4dd9bbca50565e.bspatch"
        );
    }

    #[test]
    pub fn test_release_sign() {
        let key = release_public_key();
        let other_key =
            VerifyingKey::from_public_key_pem(include_str!("./input/other.pub")).unwrap();
        let text_bytes = include_bytes!("./input/test.txt");

        let sig = read_sig(include_bytes!("./input/test.sig"));
        let sig_fail = read_sig(include_bytes!("./input/test_fail.sig"));
        let other_sig = read_sig(include_bytes!("./input/other.sig"));

        key.verify_strict(text_bytes, &sig).unwrap();
        assert!(key.verify_strict(text_bytes, &sig_fail).is_err());
        other_key.verify_strict(text_bytes, &other_sig).unwrap();
        assert!(key.verify_strict(text_bytes, &other_sig).is_err());
    }

    fn read_sig(bytes: &[u8]) -> Signature {
        Signature::from_slice(bytes).unwrap()
    }
}
