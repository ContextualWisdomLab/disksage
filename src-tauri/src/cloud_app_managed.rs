//! Default exclusions for app-managed libraries during cloud offload planning, and
//! operator free-space accounting that never credits a bare copy into CloudStorage.
//!
//! App-owned trees (reference managers, Photos bundles, Parallels VMs, macOS app
//! Containers) are not user cold-data candidates. Copying them into
//! `~/Library/CloudStorage` does not free local disk; DiskSage only credits an
//! observed local allocation reduction after provider confirmation plus a gated
//! online-only / source-eviction path.

use crate::cloud_transfer::CloudOffloadGoalState;
use std::path::Path;

/// Photos package members (existing stable code).
pub const REASON_SYSTEM_MANAGED_PHOTOS_LIBRARY: &str = "system-managed-photos-library-data";
/// Mendeley Desktop / Reference Manager application libraries.
pub const REASON_APP_MANAGED_MENDELEY: &str = "app-managed-mendeley-library";
/// Zotero profile and `storage/` attachment trees.
pub const REASON_APP_MANAGED_ZOTERO: &str = "app-managed-zotero-storage";
/// Parallels Desktop VM and support trees.
pub const REASON_APP_MANAGED_PARALLELS: &str = "app-managed-parallels-data";
/// macOS per-app `Library/Containers` sandboxes.
pub const REASON_APP_MANAGED_CONTAINERS: &str = "app-managed-macos-containers";

fn normalize_component(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<String>()
}

/// Return a non-overridable planner blocker when `path` sits inside a default
/// app-managed library exclusion. Photos bundles keep their historical reason code.
pub fn app_managed_library_blocker(path: &Path) -> Option<&'static str> {
    let components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(normalize_component(&name.to_string_lossy())),
            _ => None,
        })
        .collect();

    if components.iter().any(|name| {
        name.ends_with(".photoslibrary") || name.ends_with(".photolibrary")
    }) {
        return Some(REASON_SYSTEM_MANAGED_PHOTOS_LIBRARY);
    }

    if let Some(reason) = mendeley_blocker(&components) {
        return Some(reason);
    }
    if let Some(reason) = zotero_blocker(&components) {
        return Some(reason);
    }
    if let Some(reason) = parallels_blocker(&components) {
        return Some(reason);
    }
    if containers_blocker(&components) {
        return Some(REASON_APP_MANAGED_CONTAINERS);
    }
    None
}

fn mendeley_blocker(components: &[String]) -> Option<&'static str> {
    // Library/Application Support/Mendeley*
    for window in components.windows(3) {
        if window[0] == "library"
            && window[1] == "application support"
            && window[2].starts_with("mendeley")
        {
            return Some(REASON_APP_MANAGED_MENDELEY);
        }
    }
    // Bare "Mendeley Desktop" / "Mendeley Reference Manager" trees elsewhere.
    if components.iter().any(|name| name.starts_with("mendeley")) {
        return Some(REASON_APP_MANAGED_MENDELEY);
    }
    None
}

fn zotero_blocker(components: &[String]) -> Option<&'static str> {
    // Library/Application Support/Zotero
    for window in components.windows(3) {
        if window[0] == "library"
            && window[1] == "application support"
            && window[2] == "zotero"
        {
            return Some(REASON_APP_MANAGED_ZOTERO);
        }
    }
    // Users/<user>/Zotero/... or any .../Zotero/storage/...
    if let Some(index) = components.iter().position(|name| name == "zotero") {
        let under_users = index >= 2 && components[index - 2] == "users";
        let has_storage_child = components
            .get(index + 1)
            .is_some_and(|name| name == "storage");
        if under_users || has_storage_child {
            return Some(REASON_APP_MANAGED_ZOTERO);
        }
    }
    None
}

fn parallels_blocker(components: &[String]) -> Option<&'static str> {
    if components
        .iter()
        .any(|name| name.ends_with(".pvm") || name.ends_with(".macvm"))
    {
        return Some(REASON_APP_MANAGED_PARALLELS);
    }
    for window in components.windows(2) {
        if window[0] == "library" && window[1] == "parallels" {
            return Some(REASON_APP_MANAGED_PARALLELS);
        }
    }
    // Users/<user>/Parallels/...
    if let Some(index) = components.iter().position(|name| name == "parallels") {
        if index >= 2 && components[index - 2] == "users" {
            return Some(REASON_APP_MANAGED_PARALLELS);
        }
    }
    None
}

fn containers_blocker(components: &[String]) -> bool {
    components.windows(2).any(|window| {
        window[0] == "library" && window[1] == "containers"
    })
}

/// Local bytes that may be credited as freed for operator accounting.
///
/// A verified copy into `~/Library/CloudStorage` (or any CloudOffload goal short of
/// `source-evicted`) always credits **zero**. Only a DiskSage-gated source eviction
/// after the provider-confirmed goal path may credit an observed allocation reduction,
/// capped by the source allocation that existed before eviction.
pub fn credited_free_bytes_after_cloud_offload(
    goal_state: CloudOffloadGoalState,
    source_allocated_bytes_before: u64,
    observed_allocation_reduction_bytes: u64,
) -> u64 {
    match goal_state {
        CloudOffloadGoalState::SourceEvicted => observed_allocation_reduction_bytes.min(source_allocated_bytes_before),
        CloudOffloadGoalState::CopyVerified
        | CloudOffloadGoalState::PendingProviderSync
        | CloudOffloadGoalState::ProviderSyncConfirmed
        | CloudOffloadGoalState::EvictionReady => 0,
    }
}

/// Online-only / local-allocation eviction credits observed allocation reduction only when
/// the provider sync is confirmed and the DiskSage-gated eviction request succeeded.
/// Mere presence of a CloudStorage copy never credits.
pub fn credited_free_bytes_after_online_only_eviction(
    provider_sync_confirmed: bool,
    disksage_gated_eviction_succeeded: bool,
    observed_allocation_reduction_bytes: u64,
) -> u64 {
    if provider_sync_confirmed && disksage_gated_eviction_succeeded {
        observed_allocation_reduction_bytes
    } else {
        0
    }
}

/// Explicitly documents that a successful copy into CloudStorage frees nothing.
pub fn credited_free_bytes_for_cloud_storage_copy_only(_copied_logical_bytes: u64) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_mendeley_zotero_photos_parallels_and_containers_by_default() {
        let cases = [
            (
                "/Users/a/Library/Application Support/Mendeley Desktop/www.mendeley.com.sqlite",
                REASON_APP_MANAGED_MENDELEY,
            ),
            (
                "/Users/a/Library/Application Support/Mendeley Ltd./data.db",
                REASON_APP_MANAGED_MENDELEY,
            ),
            (
                "/Users/a/Library/Application Support/Zotero/zotero.sqlite",
                REASON_APP_MANAGED_ZOTERO,
            ),
            (
                "/Users/a/Zotero/storage/ABCDEFGH/paper.pdf",
                REASON_APP_MANAGED_ZOTERO,
            ),
            (
                "/Users/a/Pictures/Photos Library.photoslibrary/database/Photos.sqlite",
                REASON_SYSTEM_MANAGED_PHOTOS_LIBRARY,
            ),
            (
                "/Users/a/Parallels/Windows 11.pvm/config.pvs",
                REASON_APP_MANAGED_PARALLELS,
            ),
            (
                "/Users/a/Library/Parallels/vm-support.dat",
                REASON_APP_MANAGED_PARALLELS,
            ),
            (
                "/Users/a/Library/Containers/com.apple.Notes/Data/tmp/x",
                REASON_APP_MANAGED_CONTAINERS,
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(
                app_managed_library_blocker(Path::new(path)),
                Some(expected),
                "path {path}"
            );
        }
    }

    #[test]
    fn does_not_exclude_ordinary_user_documents() {
        assert_eq!(
            app_managed_library_blocker(Path::new(
                "/Users/a/Documents/cold-archive/report.pdf"
            )),
            None
        );
        assert_eq!(
            app_managed_library_blocker(Path::new(
                "/Users/a/projects/research-zotero-notes/readme.md"
            )),
            None
        );
    }

    #[test]
    fn copy_and_pre_eviction_goal_states_credit_zero() {
        // A copy into CloudStorage / pre-eviction goals never credit free space.
        assert_eq!(
            credited_free_bytes_for_cloud_storage_copy_only(750_000_000),
            0
        );
        assert_eq!(
            credited_free_bytes_after_cloud_offload(
                CloudOffloadGoalState::CopyVerified,
                750_000_000,
                750_000_000,
            ),
            0
        );
        assert_eq!(
            credited_free_bytes_after_cloud_offload(
                CloudOffloadGoalState::PendingProviderSync,
                750_000_000,
                750_000_000
            ),
            0
        );
        assert_eq!(
            credited_free_bytes_after_cloud_offload(
                CloudOffloadGoalState::ProviderSyncConfirmed,
                750_000_000,
                750_000_000
            ),
            0
        );
        assert_eq!(
            credited_free_bytes_after_cloud_offload(
                CloudOffloadGoalState::EvictionReady,
                750_000_000,
                750_000_000,
            ),
            0
        );
        assert_eq!(
            credited_free_bytes_after_online_only_eviction(false, true, 100),
            0
        );
        assert_eq!(
            credited_free_bytes_after_online_only_eviction(true, false, 100),
            0
        );
        assert_eq!(
            credited_free_bytes_after_online_only_eviction(true, true, 42_000),
            42_000
        );
    }

    #[test]
    fn source_eviction_credit_uses_under_bound_observed_allocation_reduction() {
        assert_eq!(
            credited_free_bytes_after_cloud_offload(
                CloudOffloadGoalState::SourceEvicted,
                750_000_000,
                420_000_000,
            ),
            420_000_000
        );
    }

    #[test]
    fn source_eviction_credit_caps_over_bound_observed_allocation_reduction() {
        assert_eq!(
            credited_free_bytes_after_cloud_offload(
                CloudOffloadGoalState::SourceEvicted,
                750_000_000,
                900_000_000,
            ),
            750_000_000
        );
    }

    #[test]
    fn exclusion_list_covers_required_app_families() {
        let reasons = [
            app_managed_library_blocker(Path::new(
                "/Users/a/Library/Application Support/Mendeley Reference Manager/x",
            )),
            app_managed_library_blocker(Path::new("/Users/a/Zotero/storage/ABC/x.pdf")),
            app_managed_library_blocker(Path::new(
                "/Users/a/Pictures/Library.photolibrary/db/Photos.sqlite",
            )),
            app_managed_library_blocker(Path::new("/Users/a/Parallels/Linux.macvm/Data.img")),
            app_managed_library_blocker(Path::new(
                "/Users/a/Library/Containers/com.example.app/Data/x",
            )),
        ];
        assert_eq!(
            reasons,
            [
                Some(REASON_APP_MANAGED_MENDELEY),
                Some(REASON_APP_MANAGED_ZOTERO),
                Some(REASON_SYSTEM_MANAGED_PHOTOS_LIBRARY),
                Some(REASON_APP_MANAGED_PARALLELS),
                Some(REASON_APP_MANAGED_CONTAINERS),
            ]
        );
    }
}
