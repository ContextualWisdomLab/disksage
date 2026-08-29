#![cfg(unix)]

use std::path::Path;

use disksage_lib::photo_duplicate::inspect_photo;

fn write_apng(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 8, 8);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_animated(2, 0).unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![7; 64]).unwrap();
    writer.write_image_data(&vec![9; 64]).unwrap();
    writer.finish().unwrap();
}

#[test]
fn animated_png_is_not_misreported_as_first_frame_exact_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("animated.png");
    write_apng(&path);
    assert_eq!(
        inspect_photo(&path).unwrap_err(),
        "photo-animation-unsupported"
    );
}

#[cfg(unix)]
#[test]
fn non_unicode_path_is_rejected_before_lossy_serialization() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp
        .path()
        .join(OsString::from_vec(b"photo-\xff.png".to_vec()));
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = png::Encoder::new(file, 4, 4);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![3; 16]).unwrap();
    writer.finish().unwrap();

    assert_eq!(
        inspect_photo(&path).unwrap_err(),
        "photo-input-path-encoding-unsupported"
    );
}
