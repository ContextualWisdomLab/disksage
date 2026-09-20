#![cfg(windows)]

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

struct PathRestore(OsString);

impl Drop for PathRestore {
    fn drop(&mut self) {
        std::env::set_var("PATH", &self.0);
    }
}

fn prepend_path(path: &Path) -> PathRestore {
    let original = std::env::var_os("PATH").unwrap_or_default();
    let mut updated = OsString::from(path.as_os_str());
    updated.push(";");
    updated.push(&original);
    std::env::set_var("PATH", updated);
    PathRestore(original)
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "DiskSage Test")
        .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
        .env("GIT_COMMITTER_NAME", "DiskSage Test")
        .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
        .output()
        .expect("git must start");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_regular_directory(source: &Path, destination: &Path) {
    fs::create_dir(destination).expect("replacement directory must exist before source removal");
    for entry in fs::read_dir(source).expect("source worktree must remain readable") {
        let entry = entry.expect("worktree entry must be readable");
        let file_type = entry.file_type().expect("worktree entry type must be readable");
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_regular_directory(&entry.path(), &target);
        } else {
            assert!(file_type.is_file(), "fixture refuses symlink or special entries");
            fs::copy(entry.path(), target).expect("fixture copy must preserve worktree bytes");
        }
    }
}

fn secondary_fingerprint(
    repository: &Path,
    secondary: &Path,
    generated_at_ms: u64,
) -> (String, String) {
    let report = audit_git_worktrees(
        repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        generated_at_ms,
    )
    .expect("real linked worktree must be auditable");
    let canonical_secondary = fs::canonicalize(secondary).expect("secondary must exist");
    let entry = report
        .entries
        .iter()
        .find(|entry| Path::new(&entry.path) == canonical_secondary)
        .expect("linked worktree must remain registered at the same path");
    assert_eq!(
        entry.disposition,
        GitWorktreeDisposition::RemovalCandidate,
        "fixture must remain otherwise removal-eligible: {entry:#?}"
    );
    (entry.path_fingerprint.clone(), report.removal_plan_fingerprint)
}

fn compile_timeout_git(tools: &Path, common_dir: &Path, head: &str) -> PathBuf {
    fs::create_dir_all(tools).expect("fake git directory");
    let source_path = tools.join("fake_git.rs");
    let executable = tools.join("git.exe");
    let common_literal = format!("{:?}", common_dir.to_string_lossy());
    let head_literal = format!("{head:?}");
    let source = format!(
        r#"use std::{{env, thread, time::Duration}};
fn main() {{
    let args = env::args().skip(1).collect::<Vec<_>>();
    let argv = args.iter().map(String::as_str).collect::<Vec<_>>();
    if argv.as_slice() == ["rev-parse", "--path-format=absolute", "--git-common-dir"] {{
        println!({common_literal});
        return;
    }}
    if argv.first().copied() == Some("rev-list") {{
        println!({head_literal});
        return;
    }}
    if argv.starts_with(&["worktree", "list"]) {{
        loop {{ thread::sleep(Duration::from_secs(60)); }}
    }}
    eprintln!("unexpected fake-git invocation: {{argv:?}}");
    std::process::exit(2);
}}
"#
    );
    fs::write(&source_path, source).expect("fake git source");
    let output = Command::new("rustc")
        .args(["--edition=2021", "-O", "-o"])
        .arg(&executable)
        .arg(&source_path)
        .output()
        .expect("rustc must compile deterministic fake git");
    assert!(
        output.status.success(),
        "fake git compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    executable
}

#[test]
fn same_registered_path_replacement_changes_path_and_plan_identity() {
    let _guard = TEST_ENV_LOCK.lock().expect("test environment lock");
    let temp = tempfile::tempdir().expect("temporary filesystem fixture");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    let replacement = temp.path().join("replacement");
    fs::create_dir(&repository).expect("repository root");

    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").expect("first evidence");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").expect("second evidence");
    git(&repository, &["commit", "-am", "second"]);
    git(&repository, &["branch", "merged", "HEAD~1"]);
    git(
        &repository,
        &[
            "worktree",
            "add",
            secondary.to_str().expect("UTF-8 temp path"),
            "merged",
        ],
    );

    let (before_path_fingerprint, before_plan_fingerprint) =
        secondary_fingerprint(&repository, &secondary, 100);

    // The replacement exists concurrently before the approved directory is removed, so Windows
    // cannot legitimately describe both objects with one live filesystem identity.
    copy_regular_directory(&secondary, &replacement);
    fs::remove_dir_all(&secondary).expect("fixture removes the approved object");
    fs::rename(&replacement, &secondary).expect("replacement occupies the registered pathname");

    let (after_path_fingerprint, after_plan_fingerprint) =
        secondary_fingerprint(&repository, &secondary, 101);

    assert_ne!(
        before_path_fingerprint, after_path_fingerprint,
        "Windows path fingerprint must bind volume/file-index identity, not only canonical text"
    );
    assert_ne!(
        before_plan_fingerprint, after_plan_fingerprint,
        "deletion approval must become stale when the filesystem object at the registered path changes"
    );
}

#[test]
fn admin_fallback_preserves_lock_and_prunable_reason_evidence() {
    let _guard = TEST_ENV_LOCK.lock().expect("test environment lock");
    let temp = tempfile::tempdir().expect("temporary filesystem fixture");
    let repository = temp.path().join("repository");
    let common_dir = repository.join(".git");
    let admin = common_dir.join("worktrees").join("stale-linked");
    let missing_worktree = temp.path().join("missing-linked");
    fs::create_dir_all(&admin).expect("real git admin fallback tree");

    let head = "a".repeat(40);
    fs::write(
        admin.join("gitdir"),
        format!("{}\n", missing_worktree.join(".git").display()),
    )
    .expect("gitdir evidence");
    fs::write(admin.join("HEAD"), format!("{head}\n")).expect("HEAD evidence");
    fs::write(admin.join("locked"), b"agent-owned\n").expect("lock reason evidence");
    fs::write(admin.join("prunable"), b"missing target\n").expect("prunable reason evidence");

    let tools = temp.path().join("fake-bin");
    let fake_git = compile_timeout_git(&tools, &common_dir, &head);
    assert!(fake_git.is_file(), "deterministic git timeout proxy must exist");
    let _path_restore = prepend_path(&tools);

    let report = audit_git_worktrees(
        &repository,
        &[head],
        GitWorktreeAuditOptions {
            command_timeout_ms: 1_000,
            ..GitWorktreeAuditOptions::default()
        },
        200,
    )
    .expect("worktree-list timeout must fall back to bounded real admin metadata");

    assert_eq!(report.entries.len(), 1, "{report:#?}");
    let entry = &report.entries[0];
    assert!(entry.locked, "{entry:#?}");
    assert_eq!(entry.lock_reason.as_deref(), Some("agent-owned"), "{entry:#?}");
    assert!(entry.prunable, "{entry:#?}");
    assert_eq!(
        entry.prunable_reason.as_deref(),
        Some("missing target"),
        "{entry:#?}"
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::EvidenceGap);
    assert!(entry
        .blockers
        .iter()
        .any(|reason| reason == "git-worktree-admin-fallback-evidence-incomplete"));
    assert!(!report.filesystem_mutation_executed);
}
