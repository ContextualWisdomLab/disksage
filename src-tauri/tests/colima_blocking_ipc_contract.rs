use std::fs;
use std::path::PathBuf;

fn shipped_lib_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join("src/lib.rs"))
        .expect("DiskSage Tauri registration source should be readable")
}

#[test]
fn every_colima_ipc_uses_the_blocking_adapter_module() {
    let source = shipped_lib_source();

    for registered_adapter in [
        "colima_commands::inspect_colima_reclaim_configured",
        "colima_commands::execute_colima_cache_prune_configured",
        "colima_commands::inspect_colima_dangling_images_configured",
        "colima_commands::execute_colima_dangling_images_configured",
        "colima_commands::inspect_colima_empty_volumes_configured",
        "colima_commands::execute_colima_empty_volumes_configured",
        "colima_commands::inspect_colima_guest_trim_configured",
        "colima_commands::execute_colima_guest_trim_configured",
    ] {
        assert!(
            source.contains(registered_adapter),
            "shipped Colima IPC must route through the blocking adapter: {registered_adapter}"
        );
    }

    for direct_sync_adapter in [
        "commands::inspect_colima_reclaim",
        "commands::execute_colima_cache_prune",
        "commands::inspect_colima_dangling_images",
        "commands::execute_colima_dangling_images",
        "commands::inspect_colima_empty_volumes",
        "commands::execute_colima_empty_volumes",
        "commands::inspect_colima_guest_trim",
        "commands::execute_colima_guest_trim",
    ] {
        assert!(
            !source.contains(direct_sync_adapter),
            "shipped Colima IPC must not register synchronous provider work directly: {direct_sync_adapter}"
        );
    }
}
