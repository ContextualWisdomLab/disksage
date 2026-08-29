use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_client_runtime::{
    assess_provider_client_runtime, assess_provider_primary_runtime,
};

#[test]
fn onedrive_helper_does_not_keep_the_primary_app_running() {
    let helper_only = b"Finder\nOneDrive Sync Service\n";
    let broad = assess_provider_client_runtime(CloudProvider::Onedrive, Some(helper_only), 42);

    assert_eq!(broad.runtime_observed, Some(true));
    assert_eq!(
        assess_provider_primary_runtime(CloudProvider::Onedrive, Some(helper_only)),
        Some(false)
    );
    assert_eq!(
        assess_provider_primary_runtime(
            CloudProvider::Onedrive,
            Some(b"Finder\nOneDrive\nOneDrive Sync Service\n"),
        ),
        Some(true)
    );
    assert_eq!(
        assess_provider_primary_runtime(CloudProvider::Onedrive, Some(&[0xff])),
        None
    );
}
