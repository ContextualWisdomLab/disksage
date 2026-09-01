#[test]
fn runtime_storage_commands_use_blocking_task_boundary() {
    let inspection_source = include_str!("../src/runtime_storage_commands.rs");
    let inspection_start = inspection_source
        .find("pub async fn inspect_runtime_storage(")
        .expect("async runtime-storage inspection command");
    let inspection_body = &inspection_source[inspection_start..];
    assert!(inspection_body.contains("tauri::async_runtime::spawn_blocking"));

    let command_source = include_str!("../src/commands.rs");
    for signature in [
        "pub async fn execute_runtime_storage_trim(",
        "pub async fn execute_runtime_storage_recovery(",
    ] {
        let start = command_source.find(signature).expect("async runtime-storage command");
        let body = &command_source[start
            ..command_source[start..]
                .find("\n}\n")
                .map(|end| start + end + 3)
                .unwrap()];
        assert!(body.contains("tauri::async_runtime::spawn_blocking"));
    }

    let app_source = include_str!("../src/lib.rs");
    assert!(app_source.contains("runtime_storage_commands::inspect_runtime_storage"));
    assert!(!app_source.contains("commands::inspect_runtime_storage,"));
}
