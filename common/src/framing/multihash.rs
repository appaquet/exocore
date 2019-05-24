use super::{check_into_size, check_from_size, FrameBuilder, FrameReader};
use crate::crypto::hash::MultihashDigest;
use std::io;

///
///
///
pub struct MultihashFrame<D: MultihashDigest, I: FrameReader> {
    inner: I,
    phantom: std::marker::PhantomData<D>,
}

impl<D: MultihashDigest, I: FrameReader> MultihashFrame<D, I> {
    pub fn new(inner: I) -> Result<MultihashFrame<D, I>, io::Error> {
        Ok(MultihashFrame {
            inner,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn verify(&self) -> Result<bool, io::Error> {
        let mut digest = D::new();
        digest.input(self.exposed_data());
        let digest_output = digest.into_multihash_bytes();

        let inner_exposed_data = self.inner.exposed_data();
        check_from_size(digest_output.len(), inner_exposed_data)?;
        let hash_position = inner_exposed_data.len() - digest_output.len();
        let frame_hash = &inner_exposed_data[hash_position..hash_position + digest_output.len()];

        Ok(digest_output == frame_hash)
    }
}

impl<D: MultihashDigest, I: FrameReader> FrameReader for MultihashFrame<D, I> {
    type OwnedType = MultihashFrame<D, I::OwnedType>;

    fn exposed_data(&self) -> &[u8] {
        let multihash_size = D::multihash_output_size();
        let inner_exposed_data = self.inner.exposed_data();
        &inner_exposed_data[..inner_exposed_data.len() - multihash_size]
    }

    fn whole_data(&self) -> &[u8] {
        self.inner.whole_data()
    }

    fn to_owned(&self) -> Self::OwnedType {
        MultihashFrame {
            inner: self.inner.to_owned(),
            phantom: std::marker::PhantomData,
        }
    }
}

///
///
///
pub struct MultihashFrameBuilder<D: MultihashDigest, I: FrameBuilder> {
    inner: I,
    phantom: std::marker::PhantomData<D>,
}

impl<D: MultihashDigest, I: FrameBuilder> MultihashFrameBuilder<D, I> {
    pub fn new(inner: I) -> MultihashFrameBuilder<D, I> {
        MultihashFrameBuilder {
            inner,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<D: MultihashDigest, I: FrameBuilder> FrameBuilder for MultihashFrameBuilder<D, I> {
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        // TODO: optimize by creating a proxied writer that digests
        let mut buffer = Vec::new();
        self.inner.write(&mut buffer)?;
        writer.write_all(&buffer)?;

        let mut digest = D::new();
        digest.input(&buffer);
        let multihash_bytes = digest.into_multihash_bytes();
        writer.write_all(&multihash_bytes)?;

        Ok(buffer.len() + multihash_bytes.len())
    }

    fn write_into(&self, into: &mut [u8]) -> Result<usize, io::Error> {
        let inner_size = self.inner.write_into(into)?;

        let mut digest = D::new();
        digest.input(&into[..inner_size]);
        let multihash_bytes = digest.into_multihash_bytes();
        let total_size = inner_size + multihash_bytes.len();

        check_into_size(total_size, into)?;
        into[inner_size..total_size].copy_from_slice(&multihash_bytes);

        Ok(total_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha3::Sha3_256;

    // TODO: Validate invalid size

    #[test]
    fn can_write_read_multihash_frame() -> Result<(), failure::Error> {
        let inner = "hello".as_bytes().to_vec();
        let builder = MultihashFrameBuilder::<Sha3_256, _>::new(inner.clone());

        let mut buffer1_1 = Vec::new();
        builder.write(&mut buffer1_1)?;

        let mut buffer1_2 = vec![0; buffer1_1.len()];
        builder.write_into(&mut buffer1_2)?;
        assert_eq!(buffer1_1, buffer1_2);

        let reader1 = MultihashFrame::<Sha3_256, _>::new(&buffer1_1[..])?;
        assert_eq!(buffer1_1, reader1.whole_data());
        assert_eq!(inner, reader1.exposed_data());
        assert!(reader1.verify()?);

        let mut modified_buffer = buffer1_1.clone();
        modified_buffer[0..5].copy_from_slice("world".as_bytes());
        let reader2 = MultihashFrame::<Sha3_256, _>::new(&modified_buffer[..])?;
        assert!(!reader2.verify()?);

        Ok(())
    }
}
