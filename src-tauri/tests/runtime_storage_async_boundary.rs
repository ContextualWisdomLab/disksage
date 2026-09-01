#[test]
fn runtime_storage_commands_use_blocking_task_boundary() {
    let source = include_str!("../src/commands.rs");
    for signature in [
        "pub async fn inspect_runtime_storage(",
        "pub async fn execute_runtime_storage_trim(",
        "pub async fn execute_runtime_storage_recovery(",
    ] {
        let start = source.find(signature).expect("async runtime-storage command");
        let body = &source[start
            ..source[start..]
                .find("\n}\n")
                .map(|end| start + end + 3)
                .unwrap()];
        assert!(body.contains("tauri::async_runtime::spawn_blocking"));
    }
}
