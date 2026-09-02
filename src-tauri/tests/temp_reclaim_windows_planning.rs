#![cfg(target_os = "windows")]

use disksage_lib::temp_reclaim::{plan_temp_reclaim, TempReclaimOptions};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn windows_plan_keeps_old_temp_children_visible_without_granting_removal_authority() {
    let root = std::env::temp_dir();
    let candidate = root.join(format!(
        "000000-disksage-temp-reclaim-planning-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&candidate);
    std::fs::write(&candidate, b"temporary planning evidence").expect("temporary candidate");

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_millis() as u64;
    let mut options = TempReclaimOptions::default();
    options.min_age_seconds = 1;
    options.max_children = 10_000;
    let observed_at_ms = now_ms.saturating_add(5_000);

    let plan = plan_temp_reclaim(&root, options, observed_at_ms).expect("Windows planning succeeds");
    let visible = plan
        .candidates
        .iter()
        .find(|item| std::path::Path::new(&item.path) == candidate)
        .expect("old temporary child remains visible for operator review");

    assert!(!visible.active_use.evidence_complete);
    assert!(!plan.evidence_complete);
    assert!(plan.exact_approval_phrase.is_none());
    assert!(!plan.filesystem_mutation_executed);
    assert!(candidate.exists());

    std::fs::remove_file(candidate).expect("cleanup test candidate");
}
