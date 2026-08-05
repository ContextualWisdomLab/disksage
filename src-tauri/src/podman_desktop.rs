//! Read-only desktop adapter for the Podman reclaim evidence engine.
//!
//! This module deliberately contains no prune, remove, machine lifecycle, TRIM, or raw-image
//! mutation operation. It selects a bounded machine name and executable path, then delegates to the
//! headless evidence engine with argv-based process execution. The returned report is local-only
//! evidence; callers must not emit machine names, filesystem paths, image identifiers, tags, or
//! account-local context to telemetry, analytics, remote logs, or support bundles.

use crate::podman_reclaim::{PodmanReclaimPlan, DEFAULT_PODMAN_MACHINE};
#[cfg(test)]
use crate::podman_reclaim::{PodmanReclaimAssessment, PODMAN_RECLAIM_SCHEMA_KIND};
#[cfg(not(coverage))]
use crate::podman_reclaim::DEFAULT_PROBE_TIMEOUT;
use std::path::Path;
#[cfg(any(not(coverage), test))]
use std::path::PathBuf;
use std::time::Duration;

/// Collect a Podman reclaim report through an injected read-only probe.
///
/// The adapter preserves an explicitly supplied machine name and otherwise selects
/// [`DEFAULT_PODMAN_MACHINE`]. The `probe` receives a path and argv-safe machine value rather than a
/// shell command, which keeps shell interpolation outside the desktop contract and makes the
/// dispatch behavior fully testable.
pub fn collect_podman_reclaim_plan_with<F>(
    podman_bin: &Path,
    machine: Option<String>,
    timeout: Duration,
    probe: F,
) -> PodmanReclaimPlan
where
    F: FnOnce(&Path, &str, Duration) -> PodmanReclaimPlan,
{
    let requested_machine = machine.unwrap_or_else(|| DEFAULT_PODMAN_MACHINE.to_string());
    probe(podman_bin, &requested_machine, timeout)
}

/// Read-only Tauri command that returns local Podman reclaim evidence.
///
/// The command performs no destructive action. Probe execution is moved off the UI thread, uses a
/// path-valued executable plus argv values, and returns a stable join-error code without embedding
/// local process details in the error message.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn podman_reclaim_plan(machine: Option<String>) -> Result<PodmanReclaimPlan, String> {
    let podman_bin = std::env::var_os("DISKSAGE_PODMAN_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("podman"));
    tauri::async_runtime::spawn_blocking(move || {
        collect_podman_reclaim_plan_with(
            &podman_bin,
            machine,
            DEFAULT_PROBE_TIMEOUT,
            crate::podman_reclaim::probe_podman_reclaim,
        )
    })
    .await
    .map_err(|_| "podman-reclaim-probe-join-failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn report(platform: &'static str) -> PodmanReclaimPlan {
        PodmanReclaimPlan {
            schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
            schema_version: 3,
            platform,
            evidence_complete: false,
            elapsed_ms: 0,
            machine: None,
            raw_image: None,
            guest_filesystem: None,
            store: None,
            system_df: None,
            unused_images: None,
            assessment: PodmanReclaimAssessment {
                physically_reclaimable_bytes: None,
                podman_reported_reclaimable_bytes: None,
                raw_allocated_minus_guest_used_bytes: None,
                status: "unverified".to_string(),
                reason_codes: vec!["host-physical-reclaim-unverified".to_string()],
                recommended_actions: Vec::new(),
            },
            issues: vec!["test-evidence-incomplete".to_string()],
        }
    }

    #[test]
    fn absent_machine_uses_the_documented_default_without_shell_construction() {
        let observed = RefCell::new(None);
        let result = collect_podman_reclaim_plan_with(
            Path::new("/opt/podman/bin/podman"),
            None,
            Duration::from_secs(7),
            |binary, machine, timeout| {
                observed.replace(Some((
                    binary.to_path_buf(),
                    machine.to_string(),
                    timeout,
                )));
                report("test-default")
            },
        );

        assert_eq!(result.platform, "test-default");
        assert_eq!(
            observed.into_inner(),
            Some((
                PathBuf::from("/opt/podman/bin/podman"),
                DEFAULT_PODMAN_MACHINE.to_string(),
                Duration::from_secs(7),
            ))
        );
    }

    #[test]
    fn explicit_machine_binary_and_timeout_are_forwarded_unchanged() {
        let observed = RefCell::new(None);
        let result = collect_podman_reclaim_plan_with(
            Path::new("podman-test-double"),
            Some("engineering-machine".to_string()),
            Duration::from_millis(250),
            |binary, machine, timeout| {
                observed.replace(Some((
                    binary.to_path_buf(),
                    machine.to_string(),
                    timeout,
                )));
                report("test-explicit")
            },
        );

        assert_eq!(result.platform, "test-explicit");
        assert_eq!(
            observed.into_inner(),
            Some((
                PathBuf::from("podman-test-double"),
                "engineering-machine".to_string(),
                Duration::from_millis(250),
            ))
        );
    }
}
