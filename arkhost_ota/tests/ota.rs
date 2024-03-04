#[cfg(test)]
pub mod tests {
    use arkhost_ota::*;
    use bytes::Buf;
    use pgp::{
        packet::{Packet, PacketParser},
        Deserializable, Signature, SignedPublicKey,
    };

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
        let key = release_public_key().unwrap();
        let other_key =
            SignedPublicKey::from_bytes(include_bytes!("./input/other.gpg").as_slice()).unwrap();
        let text_bytes = include_bytes!("./input/test.txt");

        let sig = read_sig(include_bytes!("./input/test.sig"));
        let sig_fail = read_sig(include_bytes!("./input/test_fail.sig"));
        let other_sig = read_sig(include_bytes!("./input/other.sig"));

        key.verify().unwrap();
        other_key.verify().unwrap();
        sig.verify(key, text_bytes.reader()).unwrap();
        assert!(sig_fail.verify(key, text_bytes.reader()).is_err());
        other_sig.verify(&other_key, text_bytes.reader()).unwrap();
        assert!(other_sig.verify(key, text_bytes.reader()).is_err());
    }

    fn read_sig(bytes: &[u8]) -> Signature {
        let mut packets: PacketParser<bytes::buf::Reader<&[u8]>> =
            PacketParser::new(bytes.reader()).into_iter();

        loop {
            match packets.next() {
                Some(Ok(Packet::Signature(sig))) => break sig,
                Some(Ok(_)) => continue,
                Some(Err(e)) => panic!("{e}"),
                None => panic!("no signature packet found in detached signature file"),
            }
        }
    }
}
