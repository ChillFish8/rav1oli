use std::io::Cursor;

use crate::ivf::IvfWriter;

/// The fields of an IVF file read back for checks.
pub(super) struct IvfFile {
    pub fourcc: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub rate: u32,
    pub scale: u32,
    pub frame_count: u32,
    pub frames: Vec<IvfFrame>,
}

pub(super) struct IvfFrame {
    pub size: u32,
    pub pts: u64,
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    let field = [bytes[offset], bytes[offset + 1]];
    u16::from_le_bytes(field)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let field: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
    u32::from_le_bytes(field)
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let field: [u8; 8] = bytes[offset..offset + 8].try_into().unwrap();
    u64::from_le_bytes(field)
}

/// Parses an IVF file, panicking on anything malformed.
pub(super) fn read_ivf(bytes: &[u8]) -> IvfFile {
    assert!(bytes.len() >= 32, "the file is shorter than its header");
    assert_eq!(&bytes[0..4], b"DKIF");
    assert_eq!(read_u16(bytes, 4), 0);
    assert_eq!(read_u16(bytes, 6), 32);

    let mut frames = Vec::new();
    let mut offset = 32;
    while offset < bytes.len() {
        assert!(offset + 12 <= bytes.len(), "a frame header is cut short");

        let size = read_u32(bytes, offset);
        let pts = read_u64(bytes, offset + 4);
        offset += 12 + size as usize;
        assert!(offset <= bytes.len(), "a frame is cut short");

        frames.push(IvfFrame { size, pts });
    }

    IvfFile {
        fourcc: bytes[8..12].try_into().unwrap(),
        width: read_u16(bytes, 12),
        height: read_u16(bytes, 14),
        rate: read_u32(bytes, 16),
        scale: read_u32(bytes, 20),
        frame_count: read_u32(bytes, 24),
        frames,
    }
}

#[test]
fn the_header_is_written_and_fixed_up() {
    let output = Cursor::new(Vec::new());
    let mut writer = IvfWriter::new(output, 136, 72, 30000, 1001).unwrap();

    writer.write_packet(&[1, 2, 3], 0).unwrap();
    writer.write_packet(&[4; 10], 1).unwrap();
    writer.write_packet(&[5; 7], 2).unwrap();

    let (output, frame_count) = writer.finish().unwrap();
    let bytes = output.into_inner();
    let file = read_ivf(&bytes);

    assert_eq!(frame_count, 3);
    assert_eq!(&file.fourcc, b"AV01");
    assert_eq!((file.width, file.height), (136, 72));
    assert_eq!((file.rate, file.scale), (30000, 1001));
    assert_eq!(file.frame_count, 3);

    let sizes: Vec<u32> = file.frames.iter().map(|frame| frame.size).collect();
    let timestamps: Vec<u64> = file.frames.iter().map(|frame| frame.pts).collect();
    assert_eq!(sizes, [3, 10, 7]);
    assert_eq!(timestamps, [0, 1, 2]);
    assert_eq!(&bytes[44..47], &[1, 2, 3]);
}
