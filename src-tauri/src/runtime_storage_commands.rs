use crate::runtime_storage::{self, RuntimeStoragePlan};

/// Reads VM-backed runtime storage without occupying Tauri's async worker with bounded subprocesses.
#[tauri::command(async)]
pub async fn inspect_runtime_storage() -> Result<Vec<RuntimeStoragePlan>, String> {
    tauri::async_runtime::spawn_blocking(runtime_storage::inspect)
        .await
        .map_err(|_| "runtime-storage-inspect-task-failed".to_string())
}
