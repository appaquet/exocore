use std::io;

pub mod capnp;
pub mod multihash;
pub mod padded;
pub mod sized;

pub use self::capnp::{CapnpFrame, CapnpFrameBuilder, TypedCapnpFrame};
pub use multihash::{MultihashFrame, MultihashFrameBuilder};
pub use padded::{PaddedFrame, PaddedFrameBuilder};
pub use sized::{IteratedSizedFrame, SizedFrame, SizedFrameBuilder, SizedFrameIterator};

///
///
///
pub trait FrameBuilder {
    type OwnedFrameType;

    fn write<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error>;
    fn write_into(&self, into: &mut [u8]) -> Result<usize, io::Error>;

    fn as_owned_frame(&self) -> Self::OwnedFrameType;

    fn as_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.write(&mut buffer)
            .expect("Couldn't write frame into in-memory vec");
        buffer
    }
}

impl FrameBuilder for Vec<u8> {
    type OwnedFrameType = Vec<u8>;

    fn write<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&self)?;
        Ok(self.len())
    }

    fn write_into(&self, into: &mut [u8]) -> Result<usize, io::Error> {
        check_into_size(self.len(), into)?;
        into[0..self.len()].copy_from_slice(&self);
        Ok(self.len())
    }

    fn as_owned_frame(&self) -> Self::OwnedFrameType {
        self.clone()
    }
}

///
///
///
pub trait FrameReader {
    type OwnedType: FrameReader;

    fn exposed_data(&self) -> &[u8];
    fn whole_data(&self) -> &[u8];
    fn to_owned(&self) -> Self::OwnedType;

    fn write<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(self.whole_data())
    }

    fn write_into(&self, into: &mut [u8]) -> Result<usize, io::Error> {
        let whole_data = self.whole_data();
        check_into_size(whole_data.len(), into)?;
        into[0..whole_data.len()].copy_from_slice(&whole_data);
        Ok(whole_data.len())
    }
}

impl FrameReader for Vec<u8> {
    type OwnedType = Vec<u8>;

    fn exposed_data(&self) -> &[u8] {
        self.as_slice()
    }

    fn whole_data(&self) -> &[u8] {
        self.as_slice()
    }

    fn to_owned(&self) -> Self::OwnedType {
        self.clone()
    }
}

impl FrameReader for &[u8] {
    type OwnedType = Vec<u8>;

    fn exposed_data(&self) -> &[u8] {
        self
    }

    fn whole_data(&self) -> &[u8] {
        self
    }

    fn to_owned(&self) -> Self::OwnedType {
        self.to_vec()
    }
}

fn check_into_size(needed: usize, into: &[u8]) -> Result<(), io::Error> {
    if into.len() < needed {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Buffer not big enough to write {} bytes (buffer is {} bytes)",
                needed,
                into.len()
            ),
        ))
    } else {
        Ok(())
    }
}

fn check_from_size(needed: usize, from: &[u8]) -> Result<(), io::Error> {
    if from.len() < needed {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Buffer not big enough to read {} bytes (read is {} bytes)",
                needed,
                from.len()
            ),
        ))
    } else {
        Ok(())
    }
}

fn check_offset_substract(offset: usize, sub_offset: usize) -> Result<(), io::Error> {
    if sub_offset > offset {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Tried to substract offset {} from offset {}, which would yield into negative offset",
                sub_offset,
                offset,
            ),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn assert_builder_equals<B: FrameBuilder>(frame_builder: &B) -> Result<(), failure::Error> {
    let mut buffer1 = Vec::new();
    frame_builder.write(&mut buffer1)?;

    assert_ne!(0, buffer1.len());

    let mut buffer2 = vec![0; 500];
    let size = frame_builder.write_into(&mut buffer2)?;
    assert_eq!(&buffer1[..], &buffer2[..size]);

    assert_eq!(frame_builder.as_bytes(), buffer1);

    Ok(())
}
