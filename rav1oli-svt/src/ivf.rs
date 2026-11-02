use std::io::{self, Seek, SeekFrom, Write};

const HEADER_BYTES: u16 = 32;
const FRAME_COUNT_OFFSET: u64 = 24;

/// Writes AV1 packets into an IVF container.
pub(crate) struct IvfWriter<W: Write + Seek> {
    output: W,
    frames: u32,
}

impl<W: Write + Seek> IvfWriter<W> {
    /// Writes the file header with a frame count of 0, which [IvfWriter::finish] fixes up.
    pub(crate) fn new(mut output: W, width: u16, height: u16, rate: u32, scale: u32) -> io::Result<Self> {
        let mut header = Vec::with_capacity(usize::from(HEADER_BYTES));
        header.extend_from_slice(b"DKIF");
        header.extend_from_slice(&0u16.to_le_bytes());
        header.extend_from_slice(&HEADER_BYTES.to_le_bytes());
        header.extend_from_slice(b"AV01");
        header.extend_from_slice(&width.to_le_bytes());
        header.extend_from_slice(&height.to_le_bytes());
        header.extend_from_slice(&rate.to_le_bytes());
        header.extend_from_slice(&scale.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());

        output.write_all(&header)?;

        Ok(Self { output, frames: 0 })
    }

    pub(crate) fn write_packet(&mut self, packet: &[u8], pts: i64) -> io::Result<()> {
        let Ok(size) = u32::try_from(packet.len()) else {
            let message = format!("a packet of {} bytes is too large for IVF", packet.len());
            return Err(io::Error::new(io::ErrorKind::InvalidData, message));
        };

        let Some(frames) = self.frames.checked_add(1) else {
            let message = "the IVF frame count is full";
            return Err(io::Error::new(io::ErrorKind::InvalidData, message));
        };

        self.output.write_all(&size.to_le_bytes())?;
        self.output.write_all(&pts.to_le_bytes())?;
        self.output.write_all(packet)?;
        self.frames = frames;

        Ok(())
    }

    /// Rewrites the header's frame count and returns the output with the number of frames written.
    pub(crate) fn finish(mut self) -> io::Result<(W, u64)> {
        self.output.seek(SeekFrom::Start(FRAME_COUNT_OFFSET))?;
        self.output.write_all(&self.frames.to_le_bytes())?;
        self.output.seek(SeekFrom::End(0))?;
        self.output.flush()?;

        Ok((self.output, u64::from(self.frames)))
    }
}
