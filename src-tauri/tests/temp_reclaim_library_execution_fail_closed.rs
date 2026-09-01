use disksage_lib::temp_reclaim::{remove_temp_candidates, TempReclaimOptions};
use std::path::Path;

#[test]
fn library_removal_fails_closed_before_path_observation() {
    let result = remove_temp_candidates(
        Path::new("/definitely-not-a-real-disksage-temp-root"),
        TempReclaimOptions::default(),
        "reviewed-plan",
        "reviewed-confirmation",
        "reviewed by operator",
        1,
    );

    assert_eq!(
        result,
        Err("temp-reclaim-removal-private-approval-unavailable".to_string())
    );
}
