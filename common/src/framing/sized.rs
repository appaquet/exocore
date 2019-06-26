use super::{check_into_size, FrameBuilder, FrameReader};
use crate::framing::{check_from_size, check_offset_substract};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io;

///
///
///
pub struct SizedFrame<I: FrameReader> {
    inner: I,
    inner_size: usize,
}

impl<I: FrameReader> SizedFrame<I> {
    pub fn new(inner: I) -> Result<SizedFrame<I>, io::Error> {
        let inner_size = inner.exposed_data().read_u32::<LittleEndian>()? as usize;
        Ok(SizedFrame { inner, inner_size })
    }

    pub fn size(&self) -> usize {
        self.inner_size + 4 + 4
    }
}

impl SizedFrame<&[u8]> {
    pub fn new_from_next_offset(
        buffer: &[u8],
        next_offset: usize,
    ) -> Result<SizedFrame<&[u8]>, io::Error> {
        check_offset_substract(next_offset, 4)?;
        check_from_size(next_offset - 4, buffer)?;

        let inner_size = (&buffer[next_offset - 4..]).read_u32::<LittleEndian>()? as usize;
        let offset_subtract = 4 + inner_size + 4;
        check_offset_substract(next_offset, offset_subtract)?;
        let offset = next_offset - offset_subtract;

        SizedFrame::new(&buffer[offset..])
    }
}

impl<I: FrameReader> FrameReader for SizedFrame<I> {
    type OwnedType = SizedFrame<I::OwnedType>;

    fn exposed_data(&self) -> &[u8] {
        &self.inner.exposed_data()[4..4 + self.inner_size]
    }

    fn whole_data(&self) -> &[u8] {
        self.inner.whole_data()
    }

    fn to_owned(&self) -> Self::OwnedType {
        SizedFrame {
            inner: self.inner.to_owned(),
            inner_size: self.inner_size,
        }
    }
}

///
///
///
pub struct SizedFrameBuilder<I: FrameBuilder> {
    inner: I,
}

impl<I: FrameBuilder> SizedFrameBuilder<I> {
    pub fn new(inner: I) -> SizedFrameBuilder<I> {
        SizedFrameBuilder { inner }
    }
}

impl<I: FrameBuilder> FrameBuilder for SizedFrameBuilder<I> {
    type OwnedFrameType = SizedFrame<Vec<u8>>;

    fn write<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut buffer = Vec::new();
        self.inner.write(&mut buffer)?;

        writer.write_u32::<LittleEndian>(buffer.len() as u32)?;
        writer.write_all(&buffer)?;
        writer.write_u32::<LittleEndian>(buffer.len() as u32)?;

        Ok(4 + buffer.len() + 4)
    }

    fn write_into(&self, into: &mut [u8]) -> Result<usize, io::Error> {
        check_into_size(8, into)?;

        let inner_size = self.inner.write_into(&mut into[4..])?;

        (&mut into[0..4]).write_u32::<LittleEndian>(inner_size as u32)?;
        let total_size = inner_size + 8;
        check_into_size(total_size, into)?;
        (&mut into[4 + inner_size..]).write_u32::<LittleEndian>(inner_size as u32)?;

        Ok(total_size)
    }

    fn as_owned_frame(&self) -> Self::OwnedFrameType {
        SizedFrame::new(self.as_bytes()).expect("Couldn't read just-created frame")
    }
}

///
///
///
pub struct SizedFrameIterator<'a> {
    buffer: &'a [u8],
    current_offset: usize,
    pub last_error: Option<io::Error>,
}

impl<'a> SizedFrameIterator<'a> {
    pub fn new(buffer: &'a [u8]) -> SizedFrameIterator<'a> {
        SizedFrameIterator {
            buffer,
            current_offset: 0,
            last_error: None,
        }
    }
}

impl<'a> Iterator for SizedFrameIterator<'a> {
    type Item = IteratedSizedFrame<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let offset = self.current_offset;
        let slice = &self.buffer[offset..];

        match SizedFrame::new(slice) {
            Ok(frame) => {
                self.current_offset += frame.size();
                Some(IteratedSizedFrame { offset, frame })
            }
            Err(err) => {
                self.last_error = Some(err);
                None
            }
        }
    }
}

pub struct IteratedSizedFrame<'a> {
    pub offset: usize,
    pub frame: SizedFrame<&'a [u8]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::assert_builder_equals;
    use std::io::Cursor;

    #[test]
    fn can_build_and_read() -> Result<(), failure::Error> {
        let inner = vec![8u8; 100];
        let builder = SizedFrameBuilder::new(inner.clone());
        assert_builder_equals(&builder)?;

        let buf1 = builder.as_bytes();
        let frame_reader = SizedFrame::new(buf1.clone())?;
        assert_eq!(inner, frame_reader.exposed_data());

        let frame_reader_owned = frame_reader.to_owned();
        assert_eq!(inner, frame_reader_owned.exposed_data());

        let mut buf3 = Vec::new();
        frame_reader.write(&mut buf3)?;
        assert_eq!(buf1, buf3);

        assert_eq!(buf1, frame_reader.whole_data());

        let mut buf4 = vec![0u8; 1000];
        let written_size = frame_reader.write_into(&mut buf4)?;
        assert_eq!(buf1, &buf4[0..written_size]);

        Ok(())
    }

    #[test]
    fn can_build_to_owned() -> Result<(), failure::Error> {
        let builder = SizedFrameBuilder::new(vec![1; 10]);

        let frame = builder.as_owned_frame();
        assert_eq!(vec![1; 10], frame.exposed_data());
        assert_eq!(10, frame.inner_size);

        Ok(())
    }

    #[test]
    fn frame_iterator() -> Result<(), failure::Error> {
        let buffer = {
            let buffer = Vec::new();
            let mut buffer_cursor = Cursor::new(buffer);

            let frame1 = SizedFrameBuilder::new(vec![1u8; 10]);
            frame1.write(&mut buffer_cursor)?;

            let frame2 = SizedFrameBuilder::new(vec![2u8; 10]);
            frame2.write(&mut buffer_cursor)?;

            buffer_cursor.into_inner()
        };

        let iter = SizedFrameIterator::new(&buffer);
        let frames = iter.collect::<Vec<_>>();
        assert_eq!(2, frames.len());
        assert_eq!(vec![1u8; 10], frames[0].frame.exposed_data());
        assert_eq!(vec![2u8; 10], frames[1].frame.exposed_data());

        let empty = Vec::new();
        let iter = SizedFrameIterator::new(&empty);
        assert_eq!(0, iter.count());

        Ok(())
    }

    #[test]
    fn from_next_offset() -> Result<(), failure::Error> {
        let buffer = {
            let buffer = Vec::new();
            let mut buffer_cursor = Cursor::new(buffer);

            let frame1 = SizedFrameBuilder::new(vec![1u8; 10]);
            frame1.write(&mut buffer_cursor)?;

            let frame2 = SizedFrameBuilder::new(vec![2u8; 10]);
            frame2.write(&mut buffer_cursor)?;

            buffer_cursor.into_inner()
        };

        let frame1 = SizedFrame::new(&buffer[..])?;
        let next_offset = frame1.size();
        let frame1_from_next = SizedFrame::new_from_next_offset(&buffer[..], next_offset)?;
        assert_eq!(1, frame1_from_next.exposed_data()[0]);

        let frame2_from_next = SizedFrame::new_from_next_offset(&buffer[..], buffer.len())?;
        assert_eq!(2, frame2_from_next.exposed_data()[0]);

        Ok(())
    }

    #[test]
    fn invalid_from_next_offset() -> Result<(), failure::Error> {
        let frame1 = SizedFrameBuilder::new(vec![1u8; 10]);
        let buffer = frame1.as_bytes();

        let result = SizedFrame::new_from_next_offset(&buffer[..], 1);
        assert!(result.is_err());

        let result = SizedFrame::new_from_next_offset(&buffer[..], buffer.len() + 2);
        assert!(result.is_err());

        let result = SizedFrame::new_from_next_offset(&buffer[..], buffer.len() - 1);
        assert!(result.is_err());

        let result = SizedFrame::new_from_next_offset(&buffer[..], buffer.len());
        assert!(result.is_ok());

        Ok(())
    }
}
