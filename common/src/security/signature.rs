
pub struct Message {}

pub struct Signature {
    bytes: Vec<u8>,
}

impl Signature {
    pub fn empty() -> Signature {
        Signature {
            bytes: vec![0u8; 64],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.iter().all(|b| *b == 0)
    }

    pub fn from_bytes(bytes: &[u8]) -> Signature {
        Signature {
            bytes: bytes.to_vec(),
        }
    }

    pub fn get_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn validate(&self, _message: &Message) -> bool {
        unimplemented!()
    }
}

