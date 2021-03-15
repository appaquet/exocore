use std::io::Read;

use multihash::Code;
pub use multihash::{Hasher, Multihash, MultihashDigest, Sha3_256, Sha3_512, StatefulHasher};

use crate::framing;

const MULTIHASH_CODE_SIZE: usize = 2;
/// Multihash digest extension.
pub trait MultihashDigestExt: StatefulHasher + Default {
    fn input_signed_frame<I: framing::FrameReader>(
        &mut self,
        frame: &framing::MultihashFrame<Self, I>,
    ) {
        self.update(frame.multihash_bytes());
    }

    fn multihash_size() -> usize {
        MULTIHASH_CODE_SIZE + usize::from(Self::size())
    }

    fn to_multihash(&self) -> Multihash;


    fn update_from_reader<R: Read>(&mut self, mut read: R) -> Result<Multihash, std::io::Error> {
        let mut bytes = Vec::new();
        read.read_to_end(&mut bytes)?;

        self.update(&bytes);
        Ok(self.to_multihash())
    }
}

impl<T> MultihashDigestExt for T
where
    T: StatefulHasher,
    Code: for<'a> From<&'a T::Digest>,
{
    fn to_multihash(&self) -> Multihash {
        let digest = self.finalize();
        Code::multihash_from_digest(&digest)
    }
}

pub trait MultihashExt {
    fn encode_bs58(&self) -> String;
}

impl MultihashExt for Multihash {
    fn encode_bs58(&self) -> String {
        bs58::encode(self.to_bytes()).into_string()
    }
}

pub fn multihash_decode_bs58(str: &str) -> Result<Multihash, HashError> {
    let bytes = bs58::decode(str).into_vec()?;
    let mh = Multihash::from_bytes(&bytes)?;
    Ok(mh)
}

#[derive(thiserror::Error, Debug)]
pub enum HashError {
    #[error("Base58 decoding error: {0}")]
    Bs58(#[from] bs58::decode::Error),
    #[error("Multihash error: {0}")]
    Multihash(#[from] multihash::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_multihash() {
        let stateless = Code::Sha3_256.digest(b"Hello world");

        let mut hasher = Sha3_256::default();
        hasher.update(b"Hello world");
        let stateful = hasher.to_multihash();

        assert_eq!(stateful, stateless);
    }

    #[test]
    fn bs58_encode_decode() {
        let mh_init = Code::Sha3_256.digest(b"Hello world");

        let bs58 = mh_init.encode_bs58();
        let mh_decoded = multihash_decode_bs58(&bs58).unwrap();
        assert_eq!(mh_init, mh_decoded);
    }
}
