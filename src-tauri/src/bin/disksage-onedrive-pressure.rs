//! Read-only, path-free OneDrive provider-cache pressure diagnostic.

#[cfg(not(coverage))]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OneDrivePressureOutput {
    observation: disksage_lib::onedrive_internal_pressure::OneDriveInternalPressureObservation,
    report: disksage_lib::onedrive_internal_pressure::OneDriveInternalPressureReport,
    mutation_executed: bool,
}

#[cfg(not(coverage))]
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum PreviousObservationFile {
    Envelope(OneDrivePressureOutput),
    Bare(disksage_lib::onedrive_internal_pressure::OneDriveInternalPressureObservation),
}

#[cfg(not(coverage))]
fn decode_previous_observation(
    bytes: &[u8],
) -> Result<disksage_lib::onedrive_internal_pressure::OneDriveInternalPressureObservation, String> {
    match serde_json::from_slice(bytes).map_err(|_| "previous-observation-invalid".to_string())? {
        PreviousObservationFile::Envelope(value) => Ok(value.observation),
        PreviousObservationFile::Bare(value) => Ok(value),
    }
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn run() -> Result<(), String> {
    use disksage_lib::onedrive_internal_pressure::{
        assess, collect, OneDriveInternalPressureObservation,
    };
    use std::path::PathBuf;

    let mut previous = None;
    let mut stall_after_ms = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--previous") => {
                previous = Some(PathBuf::from(
                    args.next().ok_or("--previous requires JSON")?,
                ))
            }
            Some("--stall-after-ms") => {
                stall_after_ms = Some(
                    args.next()
                        .and_then(|value| value.to_str().and_then(|value| value.parse().ok()))
                        .filter(|value| *value > 0)
                        .ok_or("--stall-after-ms requires a positive integer")?,
                )
            }
            Some("--help" | "-h") => {
                println!("usage: disksage-onedrive-pressure [--previous ABSOLUTE_JSON --stall-after-ms POSITIVE_INTEGER]");
                return Ok(());
            }
            _ => return Err("unknown or invalid argument".into()),
        }
    }
    if previous.is_some() != stall_after_ms.is_some() {
        return Err("--previous and --stall-after-ms must be supplied together".into());
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("home-directory-unavailable")?;
    let now = disksage_lib::cloud::system_now_ms();
    let current = collect(&home, now)?;
    let prior: Option<OneDriveInternalPressureObservation> = previous
        .map(|path| {
            if !path.is_absolute() {
                return Err("--previous must be absolute".into());
            }
            let bytes =
                std::fs::read(path).map_err(|_| "previous-observation-unreadable".to_string())?;
            decode_previous_observation(&bytes)
        })
        .transpose()?;
    let report = assess(&current, prior.as_ref(), stall_after_ms);
    println!(
        "{}",
        serde_json::to_string_pretty(&OneDrivePressureOutput {
            observation: current,
            report,
            mutation_executed: false,
        })
        .map_err(|_| "onedrive-pressure-report-encode-failed")?
    );
    Ok(())
}

#[cfg(all(not(coverage), not(target_os = "macos")))]
fn run() -> Result<(), String> {
    Err("onedrive-pressure-native-observation-macos-only".into())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-onedrive-pressure: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;
    use disksage_lib::onedrive_internal_pressure::{
        OneDriveInternalPressureObservation, OneDriveInternalPressureReport,
        OneDriveInternalPressureState,
    };
    use disksage_lib::provider_global_sync::ProviderGlobalSyncState;

    fn observation() -> OneDriveInternalPressureObservation {
        OneDriveInternalPressureObservation {
            observed_at_ms: 10,
            provider_cache_allocated_bytes: 20,
            provider_cache_file_count: 1,
            provider_cache_fingerprint: "a".repeat(64),
            cache_scan_complete: true,
            active_reader_writer_count: 0,
            active_use_evidence_complete: true,
            global_sync_state: ProviderGlobalSyncState::Pending,
            provider_reported_local_disk_full: false,
        }
    }

    #[test]
    fn emitted_envelope_is_reusable_as_previous_observation() {
        let expected = observation();
        let encoded = serde_json::to_vec(&OneDrivePressureOutput {
            observation: expected.clone(),
            report: OneDriveInternalPressureReport {
                schema_version: 1,
                state: OneDriveInternalPressureState::ProviderBusy,
                observed_at_ms: 10,
                provider_cache_allocated_bytes: 20,
                provider_cache_file_count: 1,
                evidence_complete: true,
                blockers: vec!["provider-reader-writer-or-sync-active".into()],
                next_action: "다시 확인하세요.".into(),
                provider_internal_mutation_authorized: false,
                provider_restart_authorized: false,
            },
            mutation_executed: false,
        })
        .unwrap();
        assert_eq!(decode_previous_observation(&encoded).unwrap(), expected);
        assert_eq!(
            decode_previous_observation(&serde_json::to_vec(&expected).unwrap()).unwrap(),
            expected
        );
    }
}
