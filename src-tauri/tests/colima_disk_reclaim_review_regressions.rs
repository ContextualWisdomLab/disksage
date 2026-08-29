use disksage_lib::colima_disk_reclaim::{
    execute_unavailable, plan_with_runner, ColimaCommandRunner, ColimaDiskReclaimApproval,
    ColimaDiskReclaimPlan,
};
use std::fs::File;
use std::path::Path;
use std::time::Duration;

struct FakeRunner;

impl ColimaCommandRunner for FakeRunner {
    fn run(&self, _: &Path, args: &[&str], _: Duration) -> Result<String, String> {
        assert_eq!(args, ["list", "--json", "--profile", "work"]);
        Ok("{\"name\":\"colima-work\",\"status\":\"Stopped\",\"runtime\":\"docker\",\"disk\":107374182400}\n".into())
    }
}

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join(".colima");
    let instance = home.join("_lima/colima-work");
    std::fs::create_dir_all(&instance).unwrap();
    File::create(instance.join("diffdisk"))
        .unwrap()
        .set_len(4096)
        .unwrap();
    let profile_dir = home.join("work");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(
        profile_dir.join("colima.yaml"),
        "runtime: docker\ndisk: 100\nvmType: vz\n",
    )
    .unwrap();
    let bin = temp.path().join("colima");
    File::create(&bin).unwrap();
    (temp, home, bin)
}

fn plan_at(home: &Path, bin: &Path, observed_at_ms: u64) -> ColimaDiskReclaimPlan {
    plan_with_runner(&FakeRunner, bin, home, "work", observed_at_ms).unwrap()
}

#[test]
fn reviewed_fingerprint_survives_fresh_observation_when_authority_is_unchanged() {
    let (_temp, home, bin) = fixture();
    let reviewed = plan_at(&home, &bin, 100);
    let fresh = plan_at(&home, &bin, 200);

    assert_eq!(reviewed.plan_fingerprint, fresh.plan_fingerprint);
    assert_eq!(reviewed.exact_approval_phrase, fresh.exact_approval_phrase);
}

#[test]
fn missing_list_vm_type_stays_unavailable_despite_profile_config() {
    let (_temp, home, bin) = fixture();
    let plan = plan_at(&home, &bin, 100);

    assert_eq!(plan.vm_type, "unknown");
    assert!(plan
        .blockers
        .contains(&"colima-vm-type-evidence-unavailable".to_string()));
    assert!(plan.customer_next_action.contains("VM 유형을 확인"));
}

#[test]
fn bare_human_prefix_is_not_attributed_approval() {
    let (_temp, home, bin) = fixture();
    let plan = plan_at(&home, &bin, 100);
    let approval = ColimaDiskReclaimApproval {
        plan_fingerprint: plan.plan_fingerprint.clone(),
        exact_approval_phrase: plan.exact_approval_phrase.clone(),
        approved_at_ms: 110,
        approved_by: "human:".into(),
        rationale: "reviewed exact stopped profile evidence".into(),
    };

    assert_eq!(
        execute_unavailable(&plan, &approval, 120).unwrap_err(),
        "colima-disk-reclaim-approval-invalid-or-stale"
    );
}

#[test]
fn receipt_rejects_plan_whose_bound_fields_no_longer_match_its_fingerprint() {
    let (_temp, home, bin) = fixture();
    let mut plan = plan_at(&home, &bin, 100);
    let approval = ColimaDiskReclaimApproval {
        plan_fingerprint: plan.plan_fingerprint.clone(),
        exact_approval_phrase: plan.exact_approval_phrase.clone(),
        approved_at_ms: 110,
        approved_by: "human:operator".into(),
        rationale: "reviewed exact stopped profile evidence".into(),
    };
    plan.runtime = "containerd".into();

    assert_eq!(
        execute_unavailable(&plan, &approval, 120).unwrap_err(),
        "colima-disk-reclaim-approval-invalid-or-stale"
    );
}
