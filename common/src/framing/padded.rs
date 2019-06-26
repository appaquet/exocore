use super::{check_into_size, FrameBuilder, FrameReader};
use crate::framing::check_from_size;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io;

///
///
///
pub struct PaddedFrame<I: FrameReader> {
    inner: I,
    padding_size: usize,
}

impl<I: FrameReader> PaddedFrame<I> {
    pub fn new(inner: I) -> Result<PaddedFrame<I>, io::Error> {
        let exposed_data = inner.exposed_data();
        check_from_size(4, exposed_data)?;

        let padding_size =
            (&exposed_data[exposed_data.len() - 4..]).read_u32::<LittleEndian>()? as usize;
        Ok(PaddedFrame {
            inner,
            padding_size,
        })
    }
}

impl<I: FrameReader> FrameReader for PaddedFrame<I> {
    type OwnedType = PaddedFrame<I::OwnedType>;

    fn exposed_data(&self) -> &[u8] {
        let exposed_data = self.inner.exposed_data();
        &exposed_data[..exposed_data.len() - 4 - self.padding_size]
    }

    fn whole_data(&self) -> &[u8] {
        self.inner.whole_data()
    }

    fn to_owned(&self) -> Self::OwnedType {
        PaddedFrame {
            inner: self.inner.to_owned(),
            padding_size: self.padding_size,
        }
    }
}

///
///
///
pub struct PaddedFrameBuilder<I: FrameBuilder> {
    inner: I,
    minimum_size: usize,
}

impl<I: FrameBuilder> PaddedFrameBuilder<I> {
    pub fn new(inner: I, minimum_size: usize) -> PaddedFrameBuilder<I> {
        PaddedFrameBuilder {
            inner,
            minimum_size,
        }
    }
}

impl<I: FrameBuilder> FrameBuilder for PaddedFrameBuilder<I> {
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let inner_size = self.inner.write(writer)?;

        let padding_size = if inner_size < self.minimum_size {
            let required_padding = self.minimum_size - inner_size;
            for _i in 0..required_padding {
                writer.write_u8(0)?;
            }
            required_padding
        } else {
            0
        };

        writer.write_u32::<LittleEndian>(padding_size as u32)?;
        Ok(inner_size + padding_size + 4)
    }

    fn write_into(&self, into: &mut [u8]) -> Result<usize, io::Error> {
        let inner_size = self.inner.write_into(into)?;

        let padding_size = if inner_size < self.minimum_size {
            let required_padding = self.minimum_size - inner_size;
            check_into_size(inner_size + required_padding, into)?;

            for i in 0..required_padding {
                into[inner_size + i] = 0;
            }
            required_padding
        } else {
            0
        };

        let total_size = inner_size + padding_size + 4;
        check_into_size(padding_size, into)?;
        (&mut into[inner_size + padding_size..]).write_u32::<LittleEndian>(padding_size as u32)?;

        Ok(total_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_frame_build_read() -> Result<(), failure::Error> {
        let builder = PaddedFrameBuilder::new(vec![1; 10], 0);
        let frame = PaddedFrame::new(builder.as_bytes())?;
        assert_eq!(vec![1; 10], frame.exposed_data());

        let builder = PaddedFrameBuilder::new(vec![1; 10], 10);
        let mut buffer = vec![0; 100];
        let len = builder.write_into(&mut buffer[..])?;
        let frame = PaddedFrame::new(&buffer[..len])?;
        assert_eq!(vec![1; 10], frame.exposed_data());

        let builder = PaddedFrameBuilder::new(vec![1; 10], 20);
        let frame = PaddedFrame::new(builder.as_bytes())?;
        assert_eq!(vec![1; 10], frame.exposed_data());
        assert_eq!(10, frame.padding_size);
        assert!(frame.whole_data().len() > 20);

        Ok(())
    }
}
