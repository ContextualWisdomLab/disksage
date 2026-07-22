//! Read-only, bounded inventory of locally allocated blocks inside one discovered cloud root.

#[cfg(target_os = "macos")]
embed_plist::embed_info_plist!("../../disksage-cloud-plan.Info.plist");

#[cfg(not(coverage))]
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
#[cfg(not(coverage))]
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::process::{Command, Stdio};
#[cfg(not(coverage))]
use std::sync::{Arc, Mutex};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

#[cfg(not(coverage))]
use disksage_lib::cloud::{self, CloudRoot};
#[cfg(not(coverage))]
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, hard_timeout_inventory_from_checkpoint,
    inventory_cloud_local_allocations_with_checkpoints, CloudLocalAllocationInventory,
    CloudLocalInventoryOptions,
};

#[cfg(not(coverage))]
const WORKER_REPORT_GRACE_MS: u64 = 2_000;

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    cloud_root: PathBuf,
    relative_subpath: Option<PathBuf>,
    min_allocated_mib: u64,
    max_entries: u64,
    max_results: usize,
    max_depth: usize,
    max_duration_ms: u64,
    max_issues: usize,
}

#[cfg(not(coverage))]
fn usage() -> &'static str {
    "usage: disksage-cloud-local-inventory --cloud-root ABSOLUTE_PATH [--relative-subpath SAFE_RELATIVE_PATH] [--min-allocated-mib N] [--max-entries N] [--max-results N] [--max-depth N] [--max-duration-ms N] [--max-issues N]"
}

#[cfg(not(coverage))]
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

#[cfg(not(coverage))]
fn number<T: std::str::FromStr>(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, String> {
    value(args, index, flag)?
        .parse()
        .map_err(|_| format!("{flag}는 정수여야 함"))
}

#[cfg(not(coverage))]
fn parse_args(args: &[String]) -> Result<Args, String> {
    let defaults = CloudLocalInventoryOptions::default();
    let mut cloud_root = None;
    let mut relative_subpath = None;
    let mut min_allocated_mib = defaults.min_allocated_bytes / (1024 * 1024);
    let mut max_entries = defaults.max_entries;
    let mut max_results = defaults.max_results;
    let mut max_depth = defaults.max_depth;
    let mut max_duration_ms = defaults.max_duration_ms;
    let mut max_issues = defaults.max_issues;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cloud-root" => {
                cloud_root = Some(PathBuf::from(value(args, &mut index, "--cloud-root")?))
            }
            "--relative-subpath" => {
                relative_subpath = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--relative-subpath",
                )?))
            }
            "--min-allocated-mib" => {
                min_allocated_mib = number(args, &mut index, "--min-allocated-mib")?
            }
            "--max-entries" => max_entries = number(args, &mut index, "--max-entries")?,
            "--max-results" => max_results = number(args, &mut index, "--max-results")?,
            "--max-depth" => max_depth = number(args, &mut index, "--max-depth")?,
            "--max-duration-ms" => max_duration_ms = number(args, &mut index, "--max-duration-ms")?,
            "--max-issues" => max_issues = number(args, &mut index, "--max-issues")?,
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    let cloud_root = cloud_root.ok_or_else(|| "--cloud-root 값이 필요함".to_string())?;
    if !cloud_root.is_absolute() {
        return Err("--cloud-root는 절대 경로여야 함".into());
    }
    if let Some(relative) = &relative_subpath {
        if relative.is_absolute()
            || relative.components().next().is_none()
            || relative
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err("--relative-subpath는 안전한 상대 경로여야 함".into());
        }
    }
    Ok(Args {
        cloud_root,
        relative_subpath,
        min_allocated_mib,
        max_entries,
        max_results,
        max_depth,
        max_duration_ms,
        max_issues,
    })
}

#[cfg(not(coverage))]
fn home_dir() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| "HOME/USERPROFILE을 찾을 수 없음".into())
}

#[cfg(not(coverage))]
fn select_root<'a>(roots: &'a [CloudRoot], requested: &Path) -> Result<&'a CloudRoot, String> {
    let matches: Vec<_> = roots
        .iter()
        .filter(|root| cloud::cloud_root_path_matches(Path::new(&root.path), requested))
        .collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err("요청한 경로가 현재 탐지된 클라우드 루트와 일치하지 않음".into()),
        _ => Err("요청한 경로와 일치하는 클라우드 루트가 여러 개임".into()),
    }
}

#[cfg(not(coverage))]
fn scan_root(discovered: &CloudRoot, relative_subpath: Option<&Path>) -> Result<CloudRoot, String> {
    let Some(relative) = relative_subpath else {
        return Ok(discovered.clone());
    };
    let mut path = PathBuf::from(&discovered.path);
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err("cloud-local-inventory-subpath-invalid".into());
        };
        path.push(segment);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "cloud-local-inventory-subpath-unavailable".to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("cloud-local-inventory-subpath-not-real-directory".into());
        }
    }
    let mut selected = discovered.clone();
    selected.id = format!("{}#{}", discovered.id, relative.to_string_lossy());
    selected.label = format!("{} / {}", discovered.label, relative.to_string_lossy());
    selected.path = path.to_string_lossy().into_owned();
    Ok(selected)
}

#[cfg(not(coverage))]
fn inventory_options(args: &Args) -> CloudLocalInventoryOptions {
    CloudLocalInventoryOptions {
        min_allocated_bytes: args.min_allocated_mib.saturating_mul(1024 * 1024),
        max_entries: args.max_entries,
        max_results: args.max_results,
        max_depth: args.max_depth,
        max_duration_ms: args.max_duration_ms,
        max_issues: args.max_issues,
    }
}

#[cfg(not(coverage))]
fn print_report(report: &CloudLocalAllocationInventory) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", content = "report", rename_all = "kebab-case")]
enum WorkerMessageRef<'a> {
    Checkpoint(&'a CloudLocalAllocationInventory),
    Complete(&'a CloudLocalAllocationInventory),
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", content = "report", rename_all = "kebab-case")]
enum WorkerMessage {
    Checkpoint(CloudLocalAllocationInventory),
    Complete(CloudLocalAllocationInventory),
}

#[cfg(not(coverage))]
fn write_worker_message(
    writer: &mut impl Write,
    message: &WorkerMessageRef<'_>,
) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, message).map_err(|_| "inventory-worker-json-failed")?;
    writer
        .write_all(b"\n")
        .map_err(|_| "inventory-worker-output-failed")?;
    writer
        .flush()
        .map_err(|_| "inventory-worker-output-failed".to_string())
}

#[cfg(not(coverage))]
fn run_worker(root: &CloudRoot, args: &Args) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let report = inventory_cloud_local_allocations_with_checkpoints(
        root,
        inventory_options(args),
        cloud::system_now_ms(),
        |checkpoint| write_worker_message(&mut writer, &WorkerMessageRef::Checkpoint(checkpoint)),
    )?;
    write_worker_message(&mut writer, &WorkerMessageRef::Complete(&report))
}

#[cfg(not(coverage))]
fn drain_pipe<R: Read + Send + 'static>(
    mut pipe: R,
) -> std::thread::JoinHandle<Result<String, String>> {
    std::thread::spawn(move || {
        let mut output = String::new();
        pipe.read_to_string(&mut output)
            .map_err(|_| "inventory-worker-output-failed".to_string())?;
        Ok(output)
    })
}

#[cfg(not(coverage))]
fn join_pipe(reader: std::thread::JoinHandle<Result<String, String>>) -> Result<String, String> {
    reader
        .join()
        .map_err(|_| "inventory-worker-output-thread-failed".to_string())?
}

#[cfg(not(coverage))]
fn drain_worker_stdout<R: Read + Send + 'static>(
    reader: R,
    latest_checkpoint: Arc<Mutex<Option<CloudLocalAllocationInventory>>>,
) -> std::thread::JoinHandle<Result<CloudLocalAllocationInventory, String>> {
    std::thread::spawn(move || {
        let mut complete = None;
        for line in BufReader::new(reader).lines() {
            let line = line.map_err(|_| "inventory-worker-output-failed".to_string())?;
            let message: WorkerMessage = serde_json::from_str(&line)
                .map_err(|_| "inventory-worker-json-invalid".to_string())?;
            match message {
                WorkerMessage::Checkpoint(report) => {
                    let mut latest = latest_checkpoint
                        .lock()
                        .map_err(|_| "inventory-worker-checkpoint-lock-failed".to_string())?;
                    *latest = Some(report);
                }
                WorkerMessage::Complete(report) => complete = Some(report),
            }
        }
        complete.ok_or_else(|| "inventory-worker-complete-missing".to_string())
    })
}

#[cfg(not(coverage))]
fn join_worker_stdout(
    reader: std::thread::JoinHandle<Result<CloudLocalAllocationInventory, String>>,
) -> Result<CloudLocalAllocationInventory, String> {
    reader
        .join()
        .map_err(|_| "inventory-worker-output-thread-failed".to_string())?
}

#[cfg(not(coverage))]
fn watchdog_deadline_ms(max_duration_ms: u64) -> u64 {
    max_duration_ms.saturating_add(WORKER_REPORT_GRACE_MS)
}

#[cfg(not(coverage))]
fn run_watchdog(
    raw: &[String],
    root: &CloudRoot,
    args: &Args,
) -> Result<CloudLocalAllocationInventory, String> {
    let mut child = Command::new(std::env::current_exe().map_err(|_| "inventory-exe-missing")?)
        .args(raw)
        .env("DISKSAGE_INTERNAL_INVENTORY_WORKER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "inventory-worker-spawn-failed".to_string())?;
    let latest_checkpoint = Arc::new(Mutex::new(None));
    let stdout_reader = drain_worker_stdout(
        child
            .stdout
            .take()
            .ok_or_else(|| "inventory-worker-stdout-missing".to_string())?,
        Arc::clone(&latest_checkpoint),
    );
    let stderr_reader = drain_pipe(
        child
            .stderr
            .take()
            .ok_or_else(|| "inventory-worker-stderr-missing".to_string())?,
    );
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "inventory-worker-wait-failed".to_string())?
        {
            let stdout = join_worker_stdout(stdout_reader);
            let stderr = join_pipe(stderr_reader)?;
            if !status.success() {
                let bounded: String = stderr.chars().take(4096).collect();
                return Err(if bounded.trim().is_empty() {
                    "inventory-worker-failed".into()
                } else {
                    format!("inventory-worker-failed:{bounded}")
                });
            }
            return stdout;
        }
        if u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
            >= watchdog_deadline_ms(args.max_duration_ms)
        {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_worker_stdout(stdout_reader);
            let _ = join_pipe(stderr_reader);
            let checkpoint = latest_checkpoint
                .lock()
                .ok()
                .and_then(|latest| latest.clone());
            if let Some(checkpoint) = checkpoint {
                if let Ok(report) = hard_timeout_inventory_from_checkpoint(
                    root,
                    inventory_options(args),
                    checkpoint,
                ) {
                    return Ok(report);
                }
            }
            return hard_timeout_inventory(root, inventory_options(args), cloud::system_now_ms());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let discovery = cloud::discover_cloud_roots_report(&home_dir()?);
    let discovered = select_root(&discovery.roots, &args.cloud_root)?;
    let root = scan_root(discovered, args.relative_subpath.as_deref())?;
    if std::env::var_os("DISKSAGE_INTERNAL_INVENTORY_WORKER").is_some() {
        return run_worker(&root, &args);
    }
    print_report(&run_watchdog(&raw, &root, &args)?)
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use disksage_lib::cloud::{CloudAccountScope, CloudProvider};

    #[test]
    fn parser_requires_absolute_cloud_root_and_accepts_bounds() {
        let args = parse_args(&[
            "--cloud-root".into(),
            "/Cloud".into(),
            "--relative-subpath".into(),
            "DiskSage Archive/2026".into(),
            "--min-allocated-mib".into(),
            "64".into(),
            "--max-entries".into(),
            "2000".into(),
            "--max-results".into(),
            "25".into(),
            "--max-depth".into(),
            "2".into(),
            "--max-duration-ms".into(),
            "5000".into(),
            "--max-issues".into(),
            "50".into(),
        ])
        .unwrap();
        assert_eq!(args.cloud_root, PathBuf::from("/Cloud"));
        assert_eq!(
            args.relative_subpath,
            Some(PathBuf::from("DiskSage Archive/2026"))
        );
        assert_eq!(args.min_allocated_mib, 64);
        assert_eq!(args.max_entries, 2000);
        assert_eq!(args.max_results, 25);
        assert_eq!(args.max_depth, 2);
        assert_eq!(args.max_duration_ms, 5000);
        assert_eq!(args.max_issues, 50);
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--cloud-root".into(), "relative".into()]).is_err());
        assert!(parse_args(&[
            "--cloud-root".into(),
            "/Cloud".into(),
            "--relative-subpath".into(),
            "../escape".into(),
        ])
        .is_err());
        assert!(parse_args(&["--unknown".into()]).is_err());
    }

    #[test]
    fn root_selection_requires_exact_discovered_match() {
        let roots = vec![CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud".into(),
            path: "/Cloud".into(),
            readable: true,
            access_issue: None,
        }];
        assert_eq!(
            select_root(&roots, Path::new("/Cloud")).unwrap().id,
            "icloud:test"
        );
        assert!(select_root(&roots, Path::new("/Elsewhere")).is_err());
        let duplicate = vec![roots[0].clone(), roots[0].clone()];
        assert!(select_root(&duplicate, Path::new("/Cloud")).is_err());
    }

    #[test]
    fn subpath_selection_stays_beneath_real_directory() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("Archive")).unwrap();
        let discovered = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud".into(),
            path: temp.path().to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let selected = scan_root(&discovered, Some(Path::new("Archive"))).unwrap();
        assert!(Path::new(&selected.path).starts_with(temp.path()));
        assert!(selected.id.ends_with("#Archive"));
        assert!(scan_root(&discovered, Some(Path::new("missing"))).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = tempfile::tempdir().unwrap();
            symlink(outside.path(), temp.path().join("linked")).unwrap();
            assert!(scan_root(&discovered, Some(Path::new("linked/child"))).is_err());
        }
    }

    #[test]
    fn watchdog_pipe_reader_drains_worker_output() {
        let payload = "x".repeat(256 * 1024);
        let reader = drain_pipe(std::io::Cursor::new(payload.clone()));
        assert_eq!(join_pipe(reader).unwrap(), payload);
    }

    #[test]
    fn worker_stdout_reader_retains_latest_checkpoint_and_complete_report() {
        let cloud = tempfile::tempdir().unwrap();
        let root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud".into(),
            path: cloud.path().to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let mut checkpoint = hard_timeout_inventory(
            &root,
            inventory_options(&Args {
                cloud_root: cloud.path().to_path_buf(),
                relative_subpath: None,
                min_allocated_mib: 32,
                max_entries: 100,
                max_results: 10,
                max_depth: 2,
                max_duration_ms: 1000,
                max_issues: 10,
            }),
            1,
        )
        .unwrap();
        checkpoint.stop_reasons.clear();
        checkpoint.notices.clear();
        checkpoint
            .notices
            .push("inventory-checkpoint-not-terminal".into());
        let mut complete = checkpoint.clone();
        complete.evidence_complete = true;
        complete.notices.clear();
        let mut bytes = Vec::new();
        write_worker_message(&mut bytes, &WorkerMessageRef::Checkpoint(&checkpoint)).unwrap();
        write_worker_message(&mut bytes, &WorkerMessageRef::Complete(&complete)).unwrap();
        let latest = Arc::new(Mutex::new(None));
        let reader = drain_worker_stdout(std::io::Cursor::new(bytes), Arc::clone(&latest));
        assert_eq!(join_worker_stdout(reader).unwrap(), complete);
        assert_eq!(*latest.lock().unwrap(), Some(checkpoint));
    }

    #[test]
    fn watchdog_deadline_adds_bounded_report_grace() {
        assert_eq!(watchdog_deadline_ms(60_000), 62_000);
        assert_eq!(watchdog_deadline_ms(u64::MAX - 1), u64::MAX);
    }
}
