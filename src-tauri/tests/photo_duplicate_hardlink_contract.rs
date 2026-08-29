#![cfg(unix)]

use std::path::Path;

use disksage_lib::photo_duplicate::audit_photos;

fn write_png(path: &Path, value: u8) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 8, 8);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![value; 64]).unwrap();
}

#[test]
fn nonadjacent_hard_link_alias_cannot_inflate_exact_group_membership() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("a.png");
    let independent = temp.path().join("b.png");
    let alias = temp.path().join("c.png");

    write_png(&first, 17);
    write_png(&independent, 17);
    std::fs::hard_link(&first, &alias).unwrap();

    let audit = audit_photos(&[first, independent, alias], 1);
    assert_eq!(audit.exact_groups.len(), 1);
    assert_eq!(
        audit.exact_groups[0].members.len(),
        2,
        "one filesystem object must contribute at most one group member even when its aliases are nonadjacent after path sorting"
    );
    let unique_ids: std::collections::BTreeSet<_> = audit.exact_groups[0]
        .members
        .iter()
        .map(|member| member.object_id.as_str())
        .collect();
    assert_eq!(unique_ids.len(), 2);
}
