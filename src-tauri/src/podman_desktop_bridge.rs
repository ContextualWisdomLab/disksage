//! Tauri registration boundary for privacy-safe Podman desktop evidence.
//!
//! DiskSage also exposes a separately governed Podman inspection/prune flow. This module keeps
//! the read-only privacy projection on its own command name so the two contracts cannot alias.

use crate::podman_desktop::PodmanDesktopEvidence;

/// Return the read-only, privacy-safe Podman evidence projection on a distinct IPC command.
///
/// The underlying projection performs no mutation. Its schema-bound notices describe this
/// evidence surface; separately governed Podman actions remain outside this command contract.
#[tauri::command]
pub fn inspect_podman_desktop_evidence() -> PodmanDesktopEvidence {
    crate::podman_desktop::inspect_podman_reclaim()
}
