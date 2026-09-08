#![cfg(unix)]

use std::path::Path;

use disksage_lib::photo_duplicate::inspect_photo;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn push_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(kind.len() + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    output.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn write_declared_grayscale_png(path: &Path, width: u32, height: u32) {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    push_chunk(&mut bytes, b"IHDR", &ihdr);
    // A syntactically valid empty zlib stream is enough for read_info() to reach image data.
    // The production preflight must reject hostile declared dimensions before allocating output.
    push_chunk(
        &mut bytes,
        b"IDAT",
        &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    );
    push_chunk(&mut bytes, b"IEND", &[]);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn declared_dimension_limit_is_checked_before_decode_allocation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("too-wide.png");
    // 67,108,865 RGBA16-normalized pixels require 512 MiB + 8 bytes.
    write_declared_grayscale_png(&path, 67_108_865, 1);
    assert_eq!(
        inspect_photo(&path).unwrap_err(),
        "photo-decoded-size-unsupported"
    );
}

#[test]
fn normalized_pixel_budget_is_checked_before_decode_allocation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pixel-bomb.png");
    // 8,193 × 8,192 pixels exceed the 512 MiB RGBA16-normalized budget.
    write_declared_grayscale_png(&path, 8_193, 8_192);
    assert_eq!(
        inspect_photo(&path).unwrap_err(),
        "photo-decoded-size-unsupported"
    );
}
