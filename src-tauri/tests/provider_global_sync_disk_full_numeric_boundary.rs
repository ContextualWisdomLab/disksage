use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{parse_dump, ProviderGlobalSyncState};

fn dump(marker: &str) -> String {
    format!(
        "com.google.drivefs.fpext\nsync engine state:\n error:'{marker}'\n"
    )
}

#[test]
fn longer_errno_and_osstatus_codes_do_not_impersonate_disk_full() {
    for marker in [
        "NSError: ODResult_Errno 280",
        "NSError: errno 280",
        "NSError: OSStatus -3400",
    ] {
        let report = parse_dump(CloudProvider::GoogleDrive, &dump(marker)).unwrap();
        assert!(
            !report
                .blockers
                .iter()
                .any(|blocker| blocker == "provider-global-sync-local-disk-full"),
            "{marker}: {report:?}"
        );
    }
}

#[test]
fn exact_errno_and_osstatus_disk_full_codes_remain_classified() {
    for marker in [
        "NSError: ODResult_Errno 28",
        "NSError: errno 28",
        "NSError: OSStatus -34",
    ] {
        let report = parse_dump(CloudProvider::GoogleDrive, &dump(marker)).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error, "{marker}");
        assert!(
            report
                .blockers
                .iter()
                .any(|blocker| blocker == "provider-global-sync-local-disk-full"),
            "{marker}: {report:?}"
        );
    }
}
