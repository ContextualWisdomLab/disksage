use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const PODMAN_RECLAIM_SCHEMA_KIND: &str = "disksage.podman-reclaim-plan";
pub const DEFAULT_PODMAN_MACHINE: &str = "podman-machine-default";
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_CAPTURE_BYTES: usize = 1_048_576;
const GIB: u64 = 1_073_741_824;
const CRITICAL_GUEST_AVAILABLE_BYTES: u64 = 2 * GIB;
const MATERIAL_ALLOCATION_GAP_BYTES: u64 = 512 * 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanMachineEvidence {
    pub name: String,
    pub state: String,
    pub configured_disk_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RawImageEvidence {
    pub path: String,
    pub logical_bytes: u64,
    /// st_blocks * 512 on Unix. This is observed host allocation, not reclaim proof.
    pub allocated_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuestFilesystemEvidence {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanStoreEvidence {
    pub graph_root: String,
    pub graph_root_allocated_bytes: u64,
    pub graph_root_used_bytes: u64,
    pub images: u64,
    pub containers_total: u64,
    pub containers_running: u64,
    pub containers_stopped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PodmanRecommendedActionKind {
    RestoreGuestHeadroom,
    InvestigateApi,
    ReviewGuestTrim,
    ReviewStoppedContainers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanRecommendedAction {
    pub kind: PodmanRecommendedActionKind,
    pub requires_human_approval: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanReclaimAssessment {
    /// Intentionally unknown until a before/after host free-space observation proves reclaim.
    pub physically_reclaimable_bytes: Option<u64>,
    /// Observed allocation gap only; filesystem metadata and sparse extents make it non-proof.
    pub raw_allocated_minus_guest_used_bytes: Option<u64>,
    pub status: String,
    pub reason_codes: Vec<String>,
    pub recommended_actions: Vec<PodmanRecommendedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanReclaimPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub platform: &'static str,
    pub evidence_complete: bool,
    pub elapsed_ms: u64,
    pub machine: Option<PodmanMachineEvidence>,
    pub raw_image: Option<RawImageEvidence>,
    pub guest_filesystem: Option<GuestFilesystemEvidence>,
    pub store: Option<PodmanStoreEvidence>,
    pub assessment: PodmanReclaimAssessment,
    pub issues: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PathField {
    #[serde(rename = "Path")]
    path: String,
}

#[derive(Debug, Deserialize)]
struct MachineResources {
    #[serde(rename = "DiskSize")]
    disk_size_gib: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MachineInspectRecord {
    #[serde(rename = "ConfigDir")]
    config_dir: PathField,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Resources")]
    resources: MachineResources,
}

#[derive(Debug, Deserialize)]
struct MachineConfig {
    #[serde(rename = "ImagePath")]
    image_path: PathField,
}

fn valid_machine_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn parse_machine_inspect(output: &str) -> Result<MachineInspectRecord, String> {
    let records: Vec<MachineInspectRecord> = serde_json::from_str(output)
        .map_err(|error| format!("invalid-machine-inspect-json:{error}"))?;
    if records.len() != 1 {
        return Err(format!("unexpected-machine-count:{}", records.len()));
    }
    let record = records.into_iter().next().unwrap();
    if !valid_machine_name(&record.name) {
        return Err("unsafe-machine-name".to_string());
    }
    if !Path::new(&record.config_dir.path).is_absolute() {
        return Err("machine-config-dir-not-absolute".to_string());
    }
    Ok(record)
}

fn parse_machine_config(output: &str) -> Result<PathBuf, String> {
    let config: MachineConfig = serde_json::from_str(output)
        .map_err(|error| format!("invalid-machine-config-json:{error}"))?;
    let path = PathBuf::from(config.image_path.path);
    if !path.is_absolute() {
        return Err("raw-image-path-not-absolute".to_string());
    }
    Ok(path)
}

fn parse_guest_df(output: &str) -> Result<GuestFilesystemEvidence, String> {
    let line = output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "guest-df-empty".to_string())?;
    let values = line
        .split_whitespace()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "guest-df-invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 3 {
        return Err("guest-df-field-count".to_string());
    }
    let evidence = GuestFilesystemEvidence {
        total_bytes: values[0],
        used_bytes: values[1],
        available_bytes: values[2],
    };
    if evidence.used_bytes > evidence.total_bytes
        || evidence.available_bytes > evidence.total_bytes
        || evidence.used_bytes.saturating_add(evidence.available_bytes) > evidence.total_bytes
    {
        return Err("guest-df-inconsistent".to_string());
    }
    Ok(evidence)
}

fn json_u64(value: &Value, path: &[&str]) -> Result<u64, String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor
            .get(*key)
            .ok_or_else(|| format!("podman-info-field-missing:{}", path.join(".")))?;
    }
    cursor
        .as_u64()
        .ok_or_else(|| format!("podman-info-field-invalid:{}", path.join(".")))
}

fn parse_podman_info(output: &str) -> Result<PodmanStoreEvidence, String> {
    let value: Value = serde_json::from_str(output)
        .map_err(|error| format!("invalid-podman-info-json:{error}"))?;
    let store = value
        .get("store")
        .ok_or_else(|| "podman-info-field-missing:store".to_string())?;
    let graph_root = store
        .get("graphRoot")
        .and_then(Value::as_str)
        .ok_or_else(|| "podman-info-field-invalid:store.graphRoot".to_string())?
        .to_string();
    Ok(PodmanStoreEvidence {
        graph_root,
        graph_root_allocated_bytes: json_u64(&value, &["store", "graphRootAllocated"])?,
        graph_root_used_bytes: json_u64(&value, &["store", "graphRootUsed"])?,
        images: json_u64(&value, &["store", "imageStore", "number"])?,
        containers_total: json_u64(&value, &["store", "containerStore", "number"])?,
        containers_running: json_u64(&value, &["store", "containerStore", "running"])?,
        containers_stopped: json_u64(&value, &["store", "containerStore", "stopped"])?,
    })
}

fn raw_image_evidence(path: &Path) -> Result<RawImageEvidence, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("raw-image-metadata:{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("raw-image-symbolic-link".to_string());
    }
    if !metadata.is_file() {
        return Err("raw-image-not-regular-file".to_string());
    }
    #[cfg(unix)]
    let allocated_bytes = {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.blocks().saturating_mul(512))
    };
    #[cfg(not(unix))]
    let allocated_bytes = None;
    Ok(RawImageEvidence {
        path: path.to_string_lossy().into_owned(),
        logical_bytes: metadata.len(),
        allocated_bytes,
    })
}

fn bounded_detail(value: &str) -> String {
    let flattened = value.replace(['\r', '\n'], " ");
    flattened.chars().take(512).collect()
}

fn command_text(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    let mut child = Command::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{label}-spawn:{error}"))?;
    let started = Instant::now();
    let status = loop {
        match child
            .try_wait()
            .map_err(|error| format!("{label}-wait:{error}"))?
        {
            Some(status) => break status,
            None if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{label}-timeout"));
            }
            None => thread::sleep(Duration::from_millis(25)),
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut handle) = child.stdout.take() {
        handle
            .read_to_end(&mut stdout)
            .map_err(|error| format!("{label}-stdout:{error}"))?;
    }
    if let Some(mut handle) = child.stderr.take() {
        handle
            .read_to_end(&mut stderr)
            .map_err(|error| format!("{label}-stderr:{error}"))?;
    }
    if stdout.len() > MAX_CAPTURE_BYTES || stderr.len() > MAX_CAPTURE_BYTES {
        return Err(format!("{label}-output-too-large"));
    }
    if !status.success() {
        let detail = bounded_detail(&String::from_utf8_lossy(&stderr));
        return Err(format!("{label}-failed:{detail}"));
    }
    String::from_utf8(stdout).map_err(|_| format!("{label}-stdout-not-utf8"))
}

fn assess(
    machine: Option<&PodmanMachineEvidence>,
    raw_image: Option<&RawImageEvidence>,
    guest: Option<&GuestFilesystemEvidence>,
    store: Option<&PodmanStoreEvidence>,
    issues: &[String],
) -> PodmanReclaimAssessment {
    let mut reason_codes = vec!["host-physical-reclaim-unverified".to_string()];
    let mut recommended_actions = Vec::new();
    let gap = raw_image
        .and_then(|raw| raw.allocated_bytes)
        .zip(guest.map(|value| value.used_bytes))
        .map(|(allocated, used)| allocated.saturating_sub(used));

    if let Some(guest) = guest {
        let critical_ratio = guest
            .available_bytes
            .saturating_mul(100)
            .checked_div(guest.total_bytes.max(1))
            .unwrap_or(0)
            < 2;
        if guest.available_bytes < CRITICAL_GUEST_AVAILABLE_BYTES || critical_ratio {
            reason_codes.push("guest-filesystem-critical".to_string());
            recommended_actions.push(PodmanRecommendedAction {
                kind: PodmanRecommendedActionKind::RestoreGuestHeadroom,
                requires_human_approval: true,
                rationale: "게스트의 재생성 가능 캐시와 오래된 로그를 검토해 API가 시작할 여유를 확보합니다."
                    .to_string(),
            });
        }
    }

    if gap.is_some_and(|bytes| bytes >= MATERIAL_ALLOCATION_GAP_BYTES) {
        reason_codes.push("raw-allocation-exceeds-guest-used".to_string());
        recommended_actions.push(PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::ReviewGuestTrim,
            requires_human_approval: true,
            rationale: "게스트에서 해제된 블록이 호스트 raw 할당으로 남았는지 TRIM 전후 관측으로 확인합니다."
                .to_string(),
        });
    }

    if let Some(store) = store {
        if store.containers_stopped > 0 {
            recommended_actions.push(PodmanRecommendedAction {
                kind: PodmanRecommendedActionKind::ReviewStoppedContainers,
                requires_human_approval: true,
                rationale: format!(
                    "중지 컨테이너 {}개가 참조하는 이미지와 볼륨을 사람 검토 대상으로 유지합니다.",
                    store.containers_stopped
                ),
            });
        }
    } else if machine.is_some_and(|value| value.state.eq_ignore_ascii_case("running")) {
        reason_codes.push("podman-api-evidence-missing".to_string());
        recommended_actions.push(PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::InvestigateApi,
            requires_human_approval: false,
            rationale: "머신은 실행 중이지만 API 증거가 없어 소켓과 게스트 여유 공간을 점검합니다."
                .to_string(),
        });
    }

    if !issues.is_empty() {
        reason_codes.push("partial-evidence".to_string());
    }
    reason_codes.sort();
    reason_codes.dedup();
    PodmanReclaimAssessment {
        physically_reclaimable_bytes: None,
        raw_allocated_minus_guest_used_bytes: gap,
        status: "unverified".to_string(),
        reason_codes,
        recommended_actions,
    }
}

pub fn probe_podman_reclaim(
    podman_bin: &Path,
    requested_machine: &str,
    timeout: Duration,
) -> PodmanReclaimPlan {
    let started = Instant::now();
    let mut issues = Vec::new();
    if !valid_machine_name(requested_machine) {
        issues.push("unsafe-requested-machine-name".to_string());
    }

    let inspect = if issues.is_empty() {
        command_text(
            podman_bin,
            &["machine", "inspect", requested_machine],
            timeout,
            "podman-machine-inspect",
        )
        .and_then(|output| parse_machine_inspect(&output))
        .and_then(|record| {
            if record.name == requested_machine {
                Ok(record)
            } else {
                Err("machine-name-mismatch".to_string())
            }
        })
        .map_err(|error| issues.push(error))
        .ok()
    } else {
        None
    };

    let machine = inspect.as_ref().map(|record| PodmanMachineEvidence {
        name: record.name.clone(),
        state: record.state.clone(),
        configured_disk_bytes: record
            .resources
            .disk_size_gib
            .and_then(|gib| gib.checked_mul(GIB)),
    });

    let raw_image = inspect.as_ref().and_then(|record| {
        let config_path = Path::new(&record.config_dir.path).join(format!("{}.json", record.name));
        fs::read_to_string(&config_path)
            .map_err(|error| format!("machine-config-read:{error}"))
            .and_then(|output| parse_machine_config(&output))
            .and_then(|path| raw_image_evidence(&path))
            .map_err(|error| issues.push(error))
            .ok()
    });

    let guest_filesystem = machine
        .as_ref()
        .filter(|value| value.state.eq_ignore_ascii_case("running"))
        .and_then(|_| {
            command_text(
                podman_bin,
                &[
                    "machine",
                    "ssh",
                    requested_machine,
                    "--",
                    "df",
                    "-B1",
                    "--output=size,used,avail",
                    "/",
                ],
                timeout,
                "podman-guest-df",
            )
            .and_then(|output| parse_guest_df(&output))
            .map_err(|error| issues.push(error))
            .ok()
        });

    let store = inspect.as_ref().and_then(|_| {
        command_text(
            podman_bin,
            &[
                "--connection",
                requested_machine,
                "info",
                "--format",
                "json",
            ],
            timeout,
            "podman-info",
        )
        .and_then(|output| parse_podman_info(&output))
        .map_err(|error| issues.push(error))
        .ok()
    });

    let assessment = assess(
        machine.as_ref(),
        raw_image.as_ref(),
        guest_filesystem.as_ref(),
        store.as_ref(),
        &issues,
    );
    PodmanReclaimPlan {
        schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
        schema_version: 1,
        platform: std::env::consts::OS,
        evidence_complete: issues.is_empty()
            && machine.is_some()
            && raw_image.is_some()
            && guest_filesystem.is_some()
            && store.is_some(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        machine,
        raw_image,
        guest_filesystem,
        store,
        assessment,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSPECT: &str = r#"[{"ConfigDir":{"Path":"/tmp/podman"},"Name":"podman-machine-default","State":"running","Resources":{"DiskSize":100}}]"#;
    const INFO: &str = r#"{"store":{"graphRoot":"/var/home/core/.local/share/containers/storage","graphRootAllocated":106769133568,"graphRootUsed":36028432384,"imageStore":{"number":35},"containerStore":{"number":9,"running":0,"stopped":9}}}"#;

    #[test]
    fn parses_machine_and_guest_evidence() {
        let inspect = parse_machine_inspect(INSPECT).unwrap();
        assert_eq!(inspect.name, DEFAULT_PODMAN_MACHINE);
        assert_eq!(inspect.resources.disk_size_gib, Some(100));
        let guest =
            parse_guest_df("1B-blocks Used Avail\n106769133568 36028432384 70740701184\n").unwrap();
        assert_eq!(guest.available_bytes, 70_740_701_184);
    }

    #[test]
    fn rejects_inconsistent_or_ambiguous_snapshots() {
        assert!(parse_guest_df("10 9 9\n").is_err());
        assert!(parse_machine_inspect("[]").is_err());
        assert!(!valid_machine_name("../escape"));
        assert!(!valid_machine_name("--connection"));
        assert!(!valid_machine_name(".."));
        assert!(valid_machine_name("podman-machine_default.1"));
    }

    #[test]
    fn parses_store_counts_without_claiming_reclaim() {
        let store = parse_podman_info(INFO).unwrap();
        assert_eq!(store.images, 35);
        assert_eq!(store.containers_stopped, 9);
        let guest = GuestFilesystemEvidence {
            total_bytes: 100 * GIB,
            used_bytes: 30 * GIB,
            available_bytes: 69 * GIB,
        };
        let raw = RawImageEvidence {
            path: "/tmp/machine.raw".into(),
            logical_bytes: 100 * GIB,
            allocated_bytes: Some(70 * GIB),
        };
        let result = assess(None, Some(&raw), Some(&guest), Some(&store), &[]);
        assert_eq!(result.physically_reclaimable_bytes, None);
        assert_eq!(result.raw_allocated_minus_guest_used_bytes, Some(40 * GIB));
        assert!(result
            .reason_codes
            .contains(&"raw-allocation-exceeds-guest-used".to_string()));
        assert!(result.recommended_actions.iter().all(|action| action.kind
            == PodmanRecommendedActionKind::InvestigateApi
            || action.requires_human_approval));
    }

    #[test]
    fn critical_guest_and_partial_evidence_are_explicit() {
        let machine = PodmanMachineEvidence {
            name: DEFAULT_PODMAN_MACHINE.into(),
            state: "running".into(),
            configured_disk_bytes: Some(100 * GIB),
        };
        let guest = GuestFilesystemEvidence {
            total_bytes: 100 * GIB,
            used_bytes: 99 * GIB,
            available_bytes: GIB,
        };
        let result = assess(
            Some(&machine),
            None,
            Some(&guest),
            None,
            &["podman-info-timeout".into()],
        );
        assert!(result
            .reason_codes
            .contains(&"guest-filesystem-critical".to_string()));
        assert!(result
            .reason_codes
            .contains(&"partial-evidence".to_string()));
        assert!(result.recommended_actions.iter().any(|action| action.kind
            == PodmanRecommendedActionKind::InvestigateApi
            && !action.requires_human_approval));
    }

    #[test]
    fn machine_config_requires_an_absolute_raw_path() {
        assert_eq!(
            parse_machine_config(r#"{"ImagePath":{"Path":"/tmp/machine.raw"}}"#).unwrap(),
            PathBuf::from("/tmp/machine.raw")
        );
        assert!(parse_machine_config(r#"{"ImagePath":{"Path":"relative.raw"}}"#).is_err());
    }

    #[test]
    fn serialized_contract_keeps_reclaim_unknown_and_action_codes_stable() {
        let plan = PodmanReclaimPlan {
            schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
            schema_version: 1,
            platform: "macos",
            evidence_complete: false,
            elapsed_ms: 7,
            machine: None,
            raw_image: None,
            guest_filesystem: None,
            store: None,
            assessment: PodmanReclaimAssessment {
                physically_reclaimable_bytes: None,
                raw_allocated_minus_guest_used_bytes: None,
                status: "unverified".into(),
                reason_codes: vec!["host-physical-reclaim-unverified".into()],
                recommended_actions: vec![PodmanRecommendedAction {
                    kind: PodmanRecommendedActionKind::ReviewGuestTrim,
                    requires_human_approval: true,
                    rationale: "review".into(),
                }],
            },
            issues: vec!["partial-evidence".into()],
        };

        let value = serde_json::to_value(plan).unwrap();
        assert_eq!(value["schema_kind"], PODMAN_RECLAIM_SCHEMA_KIND);
        assert!(value["assessment"]["physically_reclaimable_bytes"].is_null());
        assert_eq!(
            value["assessment"]["recommended_actions"][0]["kind"],
            "review_guest_trim"
        );
        assert_eq!(
            value["assessment"]["recommended_actions"][0]["requires_human_approval"],
            true
        );
    }

    #[cfg(unix)]
    #[test]
    fn external_probe_timeout_is_bounded() {
        let started = Instant::now();
        let error = command_text(
            Path::new("/bin/sleep"),
            &["2"],
            Duration::from_millis(25),
            "slow-probe",
        )
        .unwrap_err();
        assert_eq!(error, "slow-probe-timeout");
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
