//! Tauri registration boundary for privacy-safe Podman desktop evidence.
//!
//! DiskSage also exposes a separately governed Podman inspection/prune flow. This module keeps
//! the read-only privacy projection on its own command name so the two contracts cannot alias.

use crate::podman_desktop::PodmanDesktopEvidence;

/// Return the read-only, privacy-safe Podman evidence projection on a distinct IPC command.
///
/// The underlying projection performs no mutation. The notice intentionally describes this
/// evidence command rather than the entire Cleanup screen, which may expose separately governed
/// mutation actions with their own exact approval contracts.
#[tauri::command]
pub fn inspect_podman_desktop_evidence() -> PodmanDesktopEvidence {
    let mut evidence = crate::podman_desktop::inspect_podman_reclaim();
    evidence.notices = vec![
        "Podman-reported logical candidates are not verified host physical reclaimability."
            .to_string(),
        "This privacy-safe evidence command exposes no prune, remove, machine lifecycle, TRIM, or raw-image mutation operation."
            .to_string(),
    ];
    evidence
}
