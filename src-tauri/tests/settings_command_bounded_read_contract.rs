const COMMANDS_SOURCE: &str = include_str!("../src/commands.rs");

fn get_settings_body() -> &'static str {
    let start = COMMANDS_SOURCE
        .find("pub fn get_settings")
        .expect("get_settings command must remain present");
    let remainder = &COMMANDS_SOURCE[start..];
    let end = remainder
        .find("pub fn set_settings")
        .expect("set_settings must follow get_settings");
    &remainder[..end]
}

#[test]
fn get_settings_uses_bounded_settings_file_loader() {
    let body = get_settings_body();
    assert!(
        body.contains("crate::settings::load_settings_file(&path)"),
        "the shipped settings command must delegate disk reads to the bounded loader"
    );
    assert!(
        !body.contains("std::fs::read_to_string"),
        "the shipped settings command must not read an unbounded settings document"
    );
}
