use std::fs;
use std::path::PathBuf;

#[test]
fn runtime_storage_binary_is_explicitly_declared_for_packaging() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let source = fs::read_to_string(&manifest).expect("read Cargo manifest");
    let mut sections = source.split("[[bin]]").skip(1);
    let declared = sections.any(|section| {
        let section = section.split("[[").next().unwrap_or(section);
        section
            .lines()
            .any(|line| line.trim() == "name = \"disksage-runtime-storage\"")
            && section
                .lines()
                .any(|line| line.trim() == "path = \"src/bin/disksage-runtime-storage.rs\"")
    });

    assert!(
        declared,
        "runtime-storage is an operational artifact and must be an explicit [[bin]] target"
    );
}
