//! Fail-closed, identity-bound orphan reclamation for Docker, Podman, and Colima runtimes.
//!
//! This module audits four orphan categories — stopped containers, unreferenced images,
//! dangling volumes, and unused custom networks — across three runtime targets:
//!
//! - `docker` with the default context (`DockerNative`)
//! - `docker --context colima` for Colima-managed Docker sockets (`DockerColimaContext`)
//! - `podman --connection <machine>` against a running Podman machine (`PodmanMachine`)
//!
//! Safety contract shared with [`crate::podman_reclaim`]:
//!
//! 1. The audit is read-only and bounded by wall-clock timeouts and output caps.
//! 2. Every execution requires a fresh audit at execution time; the approval phrase embeds
//!    a SHA-256 fingerprint of the exact sorted candidate identity set.
//! 3. Running or paused containers are never candidates. Built-in networks
//!    (`bridge`, `host`, `none`, `podman`) are never candidates. Image deletion targets only
//!    full identities returned by each runtime's authoritative `dangling=true` image filter
//!    after a bounded container-membership query proves no container references the image.
//! 4. Mutation targets only the exact identities observed by the fresh audit. Category-wide
//!    `prune` commands are never used, so a resource that becomes orphaned after the audit cannot
//!    be swept into the approved mutation set.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const CONTAINER_ORPHAN_SCHEMA_KIND: &str = "disksage.container-orphan-plan";
const CONTAINER_ORPHAN_SCHEMA_VERSION: u32 = 1;
/// Bounded per-command wall clock; matches the existing Podman prune bound.
const ORPHAN_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CAPTURE_BYTES: usize = 1_048_576;
const MAX_DOCKER_HOST_BYTES: usize = 2 * 1024;
const INDETERMINATE_MUTATION_OUTCOME: &str = "container-orphan-prune-outcome-indeterminate";

/// Maximum number of network-inspect probes per audit; keeps the read-only pass bounded.
pub const MAX_NETWORK_CANDIDATES: usize = 64;
/// Maximum number of records retained per category before the audit fails closed.
pub const MAX_CATEGORY_RECORDS: usize = 4_096;
/// Exact deletion is deliberately capped so a single runtime invocation remains bounded on every
/// supported host, including Windows command-line limits and 200-byte volume/network names.
const MAX_EXACT_DELETE_CANDIDATES: usize = 256;

/// Runtime target kinds supported by the orphan reclaim engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerRuntimeKind {
    /// Plain `docker` against the default context / local socket.
    DockerNative,
    /// `docker --context colima` against a Colima-managed socket.
    DockerColimaContext,
    /// `podman --connection <machine>` against a running Podman machine.
    PodmanMachine,
}

impl ContainerRuntimeKind {
    /// Stable lowercase identifier used in receipts and UI labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DockerNative => "docker-native",
            Self::DockerColimaContext => "docker-colima-context",
            Self::PodmanMachine => "podman-machine",
        }
    }

    fn is_docker(self) -> bool {
        matches!(self, Self::DockerNative | Self::DockerColimaContext)
    }
}

/// A concrete runtime target: binary plus optional scope name (context or machine).
///
/// Scope names are validated to reject option injection: they must be non-empty ASCII
/// alphanumeric plus `-_.`, must not start with `-`, `.`, or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerRuntimeTarget {
    pub kind: ContainerRuntimeKind,
    pub binary_path: PathBuf,
    pub scope_name: Option<String>,
    docker_host: Option<String>,
    docker_context: Option<String>,
}

fn valid_scope_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

impl ContainerRuntimeTarget {
    /// Builds a target after validating the scope name fail-closed.
    pub fn new(
        kind: ContainerRuntimeKind,
        binary_path: PathBuf,
        scope_name: Option<String>,
    ) -> Result<Self, String> {
        if let Some(scope) = &scope_name {
            if !valid_scope_name(scope) {
                return Err("unsafe-runtime-scope-name".into());
            }
        }
        Ok(Self {
            kind,
            binary_path,
            scope_name,
            docker_host: None,
            docker_context: None,
        })
    }

    /// Pins Docker-native commands to one resolved daemon endpoint.
    pub(crate) fn docker_native_host(binary_path: PathBuf, host: String) -> Result<Self, String> {
        if host.is_empty()
            || host.len() > MAX_DOCKER_HOST_BYTES
            || host.chars().any(char::is_control)
        {
            return Err("unsafe-docker-host".into());
        }
        Ok(Self {
            kind: ContainerRuntimeKind::DockerNative,
            binary_path,
            scope_name: None,
            docker_host: Some(host),
            docker_context: None,
        })
    }

    /// Pins Docker-native commands to a named CLI context, preserving its TLS material.
    pub(crate) fn docker_native_context(binary_path: PathBuf, context: String) -> Result<Self, String> {
        if !valid_scope_name(&context) {
            return Err("unsafe-runtime-scope-name".into());
        }
        Ok(Self {
            kind: ContainerRuntimeKind::DockerNative,
            binary_path,
            scope_name: None,
            docker_host: None,
            docker_context: Some(context),
        })
    }

    /// Human-readable display name for receipts and UI copy.
    pub fn display_name(&self) -> String {
        let base = self
            .binary_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "runtime".to_string());
        match (&self.scope_name, self.kind) {
            (Some(scope), _) => format!("{base} {scope}"),
            (None, kind) => format!("{base} ({})", kind.as_str()),
        }
    }

    /// Command-line prefix injected before every subcommand.
    ///
    /// The prefix is validated so no user-controlled bytes can introduce an option:
    /// only fixed flags (`--context`, `--connection`) plus the validated scope name.
    pub fn command_prefix(&self) -> Result<Vec<String>, String> {
        let binary = self.binary_path.to_string_lossy().into_owned();
        if binary.is_empty() || binary.contains('\0') {
            return Err("unsafe-runtime-binary-path".into());
        }
        let mut prefix = vec![binary];
        match self.kind {
            ContainerRuntimeKind::DockerNative => {
                if let Some(context) = &self.docker_context {
                    prefix.extend(["--context".to_string(), context.clone()]);
                } else if let Some(host) = &self.docker_host {
                    prefix.extend(["--host".to_string(), host.clone()]);
                }
            }
            ContainerRuntimeKind::DockerColimaContext | ContainerRuntimeKind::PodmanMachine => {
                let flag = match self.kind {
                    ContainerRuntimeKind::PodmanMachine => "--connection",
                    _ => "--context",
                };
                let scope = self
                    .scope_name
                    .as_ref()
                    .ok_or_else(|| format!("missing-scope-for-{}", self.kind.as_str()))?;
                if !valid_scope_name(scope) {
                    return Err("unsafe-runtime-scope-name".into());
                }
                prefix.push(flag.to_string());
                prefix.push(scope.clone());
            }
        }
        Ok(prefix)
    }
}

/// Resolves a named Docker context to the endpoint used by an explicit `--host` command.
pub(crate) fn resolve_docker_context_host(
    binary_path: &Path,
    context: &str,
) -> Result<String, String> {
    if !valid_scope_name(context) {
        return Err("unsafe-runtime-scope-name".into());
    }
    let output = command_text(
        binary_path,
        &["context", "inspect", context, "--format", "{{json .Endpoints.docker.Host}}"],
        ORPHAN_COMMAND_TIMEOUT,
        "docker-context-host-inspect",
    )?;
    let host: String = serde_json::from_str(output.trim())
        .map_err(|_| "docker-context-host-invalid".to_string())?;
    ContainerRuntimeTarget::docker_native_host(binary_path.to_path_buf(), host.clone())?;
    Ok(host)
}

/// Returns a stable fingerprint of the complete context definition, including TLS metadata.
pub(crate) fn resolve_docker_context_fingerprint(
    binary_path: &Path,
    context: &str,
) -> Result<String, String> {
    if !valid_scope_name(context) {
        return Err("unsafe-runtime-scope-name".into());
    }
    let output = command_text(
        binary_path,
        &["context", "inspect", context, "--format", "{{json .}}"],
        ORPHAN_COMMAND_TIMEOUT,
        "docker-context-inspect",
    )?;
    let value: Value = serde_json::from_str(output.trim())
        .map_err(|_| "docker-context-invalid".to_string())?;
    let canonical = serde_json::to_vec(&value).map_err(|_| "docker-context-invalid".to_string())?;
    let digest = Sha256::digest(canonical);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").map_err(|_| "docker-context-invalid".to_string())?;
    }
    Ok(encoded)
}

/// Orphan categories audited and pruned by this engine, one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrphanCategory {
    /// Stopped containers (`exited`/`created`/`dead`/`stopped` states).
    Container,
    /// Runtime-reported dangling images.
    Image,
    /// Dangling volumes not referenced by any container.
    Volume,
    /// Custom networks with no attached container endpoint.
    Network,
}

impl OrphanCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Image => "image",
            Self::Volume => "volume",
            Self::Network => "network",
        }
    }

    fn domain_tag(self) -> &'static str {
        match self {
            Self::Container => "disksage.container-orphans.v1",
            Self::Image => "disksage.container-image-orphans.v1",
            Self::Volume => "disksage.container-volume-orphans.v1",
            Self::Network => "disksage.container-network-orphans.v1",
        }
    }

    fn exact_delete_subcommand(self) -> [&'static str; 2] {
        match self {
            Self::Container => ["container", "rm"],
            Self::Image => ["image", "rm"],
            Self::Volume => ["volume", "rm"],
            Self::Network => ["network", "rm"],
        }
    }
}

/// Per-category candidate evidence. Candidate identities are never rendered in reports;
/// only their SHA-256 set fingerprint is exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrphanCandidateEvidence {
    pub total_records: u64,
    pub candidate_records: u64,
    /// Sum of record sizes where the runtime reports them (images); otherwise null.
    pub candidate_size_sum_bytes: Option<u64>,
    pub candidate_set_sha256: String,
}

/// Read-only audit result for one category on one healthy runtime target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrphanCategoryPlan {
    pub category: OrphanCategory,
    pub evidence_complete: bool,
    /// Bounded failure reason when evidence is incomplete; empty when complete.
    pub issue: Option<String>,
    pub evidence: Option<OrphanCandidateEvidence>,
    /// Present only when fresh evidence contains at least one candidate.
    pub approval_phrase: Option<String>,
    /// Redacted exact-delete command shape; candidate identities never enter serialized reports.
    pub prune_command: Option<Vec<String>>,
    /// Exact validated identities bound to `evidence`; deliberately excluded from serialization.
    #[serde(skip_serializing)]
    candidate_ids: Vec<String>,
}

/// Health observation for the probed runtime target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeHealthEvidence {
    pub kind: ContainerRuntimeKind,
    pub display_name: String,
    pub healthy: bool,
    pub detail_issue: Option<String>,
}

/// Full read-only plan for one runtime target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerOrphanPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub platform: &'static str,
    pub evidence_complete: bool,
    pub elapsed_ms: u64,
    pub runtime: RuntimeHealthEvidence,
    pub categories: Vec<OrphanCategoryPlan>,
    pub issues: Vec<String>,
}

/// Execution receipt for one approved prune. Mirrors the Podman dangling-image receipt
/// shape so downstream consumers can treat both uniformly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerOrphanPruneExecution {
    pub schema_version: u32,
    pub runtime_display_name: String,
    pub category: OrphanCategory,
    pub candidate_set_sha256: String,
    pub command: Vec<String>,
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub before_available_bytes: Option<u64>,
    pub after_available_bytes: Option<u64>,
    /// Only a positive before/after available-space delta is reported; attribution-weak.
    pub observed_available_gain_bytes: Option<u64>,
    pub rationale: String,
}

// ---------------------------------------------------------------------------
// Tolerant record parsing. Docker emits NDJSON (one object per line) while Podman
// emits a JSON array; both use PascalCase keys except Podman's network listing,
// which uses lowercase keys. Parsers accept either envelope and key casing.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContainerRecord {
    id: String,
    state: String,
    names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageRecord {
    id: String,
    tags: Vec<String>,
    containers: Option<u64>,
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VolumeRecord {
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NetworkRecord {
    id: Option<String>,
    name: String,
    driver: String,
}

fn split_json_envelopes(output: &str) -> Result<Vec<Value>, String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(Value::Array(records)) = serde_json::from_str::<Value>(trimmed) {
        return Ok(records);
    }
    // NDJSON: skip blank lines, fail closed on any malformed line instead of skipping it,
    // because silently dropping a record could hide a referenced resource from evidence.
    let mut records = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(line)
            .map_err(|error| format!("invalid-json-record:{error}"))?;
        records.push(value);
    }
    if records.is_empty() {
        return Err("empty-json-output".to_string());
    }
    Ok(records)
}

fn string_field(record: &Value, keys: &[&str]) -> Result<String, String> {
    for key in keys {
        if let Some(value) = record.get(*key).and_then(Value::as_str) {
            return Ok(value.to_string());
        }
    }
    Err(format!("json-field-missing:{}", keys[0]))
}

/// Normalizes a runtime-reported ID to bare lowercase hex, rejecting anything else.
fn normalize_hex_id(raw: &str, label: &str) -> Result<String, String> {
    let stripped = raw.strip_prefix("sha256:").unwrap_or(raw);
    if stripped.len() == 64
        && stripped
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Ok(stripped.to_string());
    }
    Err(format!("{label}-invalid-id"))
}

fn parse_container_records(output: &str) -> Result<Vec<ContainerRecord>, String> {
    let values = split_json_envelopes(output)?;
    if values.len() > MAX_CATEGORY_RECORDS {
        return Err("record-count-exceeds-bound".to_string());
    }
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        let id = string_field(&value, &["ID", "Id"])?;
        let id = normalize_hex_id(&id, "container")?;
        let state = string_field(&value, &["State", "state"])?.to_lowercase();
        let names = match value.get("Names") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect(),
            // Podman may serialize Names as a JSON-encoded array string; Docker emits a plain
            // comma-joined string. Names are not used for candidacy, so accept both shapes.
            Some(Value::String(encoded)) => serde_json::from_str::<Vec<String>>(encoded)
                .unwrap_or_else(|_| encoded.split(',').map(str::to_string).collect()),
            None => Vec::new(),
            Some(_) => return Err("container-names-invalid".to_string()),
        };
        records.push(ContainerRecord { id, state, names });
    }
    Ok(records)
}

/// Containers are orphan candidates only when fully stopped: `exited`, `created`, `dead`,
/// or Podman's documented `stopped`. Known pre-start/transitional states are preserved; only
/// unrecognized states fail the category closed.
fn classify_container_candidates(
    records: &[ContainerRecord],
) -> Result<(u64, Vec<&ContainerRecord>), String> {
    let mut candidates = Vec::new();
    for record in records {
        match record.state.as_str() {
            "exited" | "created" | "dead" | "stopped" => candidates.push(record),
            "running" | "paused" | "restarting" | "removing" | "initialized" | "stopping"
            | "configured" => {}
            other => return Err(format!("unknown-container-state:{other}")),
        }
    }
    let total = u64::try_from(records.len()).map_err(|_| "record-count-overflow".to_string())?;
    Ok((total, candidates))
}

fn parse_u64_field(record: &Value, keys: &[&str]) -> Result<Option<u64>, String> {
    for key in keys {
        let field = match record.get(*key) {
            Some(field) => field,
            None => continue,
        };
        return match field {
            Value::Number(number) => {
                Ok(Some(number.as_u64().ok_or_else(|| {
                    format!("json-field-invalid:{}", keys[0])
                })?))
            }
            Value::String(text) if text == "-1" => Err(format!("json-field-invalid:{}", keys[0])),
            Value::String(text) => text
                .parse::<u64>()
                .map(Some)
                .map_err(|_| format!("json-field-invalid:{}", keys[0])),
            Value::Null => Ok(None),
            _ => Err(format!("json-field-invalid:{}", keys[0])),
        };
    }
    Ok(None)
}

fn parse_image_records(output: &str) -> Result<Vec<ImageRecord>, String> {
    let values = split_json_envelopes(output)?;
    if values.len() > MAX_CATEGORY_RECORDS {
        return Err("record-count-exceeds-bound".to_string());
    }
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        let raw_id = string_field(&value, &["ID", "Id", "id"])?;
        let id = normalize_hex_id(&raw_id, "image")?;
        let mut tags = Vec::new();
        for tag_key in ["RepoTags", "RepoDigests"] {
            if let Some(Value::Array(items)) = value.get(tag_key) {
                tags.extend(
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string)),
                );
            }
        }
        tags.sort();
        tags.dedup();
        let containers = parse_u64_field(&value, &["Containers", "containers"])?;
        let size_bytes = parse_u64_field(&value, &["Size", "size"])?
            .ok_or_else(|| "json-field-missing:Size".to_string())?;
        records.push(ImageRecord {
            id,
            tags,
            containers,
            size_bytes,
        });
    }
    Ok(records)
}

fn parse_docker_dangling_image_ids(output: &str) -> Result<Vec<String>, String> {
    let values = split_json_envelopes(output)?;
    if values.len() > MAX_CATEGORY_RECORDS {
        return Err("record-count-exceeds-bound".to_string());
    }
    values
        .into_iter()
        .map(|value| {
            let raw_id = string_field(&value, &["ID", "Id"])?;
            normalize_hex_id(&raw_id, "image")
        })
        .collect()
}

/// Parse the exact byte sizes returned by `docker image inspect` for the already-authorized
/// dangling image identities.  The list command's `Size` field is human-readable, so it is not
/// converted with a unit heuristic; inspect's numeric `Size` is the only accepted estimate.
fn parse_docker_image_sizes(output: &str) -> Result<BTreeMap<String, u64>, String> {
    let values = split_json_envelopes(output)?;
    if values.len() > MAX_CATEGORY_RECORDS {
        return Err("record-count-exceeds-bound".to_string());
    }
    let mut sizes = BTreeMap::new();
    for value in values {
        let raw_id = string_field(&value, &["Id", "ID", "id"])?;
        let id = normalize_hex_id(&raw_id, "image")?;
        let size = ["Size", "size"]
            .iter()
            .find_map(|key| value.get(*key).and_then(Value::as_u64))
            .ok_or_else(|| "json-field-invalid-or-missing:Size".to_string())?;
        if sizes.insert(id, size).is_some() {
            return Err("duplicate-image-id".to_string());
        }
    }
    Ok(sizes)
}

fn inspect_docker_image_sizes(
    target: &ContainerRuntimeTarget,
    prefix: &[String],
    image_ids: &[String],
) -> Result<BTreeMap<String, u64>, String> {
    if image_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut args: Vec<String> = prefix.iter().skip(1).cloned().collect();
    args.extend([
        "image".to_string(),
        "inspect".to_string(),
        "--format".to_string(),
        "{{json .}}".to_string(),
    ]);
    args.extend(image_ids.iter().cloned());
    let references: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = command_text(
        &target.binary_path,
        &references,
        ORPHAN_COMMAND_TIMEOUT,
        "orphan-docker-image-size-inspect",
    )?;
    let sizes = parse_docker_image_sizes(&output)?;
    if sizes.len() != image_ids.len()
        || image_ids
            .iter()
            .any(|image_id| !sizes.contains_key(image_id))
        || sizes
            .keys()
            .any(|image_id| !image_ids.iter().any(|expected| expected == image_id))
    {
        return Err("docker-image-size-identity-mismatch".to_string());
    }
    Ok(sizes)
}

/// Images are candidates only with proven zero references and no usable tag/digest.
/// A missing container-reference count fails closed for that record.
fn classify_image_candidates(records: &[ImageRecord]) -> Result<(u64, Vec<&ImageRecord>), String> {
    let mut candidates = Vec::new();
    for record in records {
        let references = record.containers.ok_or_else(|| {
            format!(
                "image-reference-count-unavailable:{}",
                &record.id[..8.min(record.id.len())]
            )
        })?;
        if references == 0 && record.tags.is_empty() {
            candidates.push(record);
        }
    }
    let total = u64::try_from(records.len()).map_err(|_| "record-count-overflow".to_string())?;
    Ok((total, candidates))
}

fn validate_resource_name(raw: &str, label: &str) -> Result<String, String> {
    if raw.is_empty()
        || raw.starts_with('-')
        || raw.len() > 200
        || raw.chars().any(char::is_control)
    {
        return Err(format!("{label}-invalid-name"));
    }
    Ok(raw.to_string())
}

fn parse_volume_records(output: &str) -> Result<Vec<VolumeRecord>, String> {
    let values = split_json_envelopes(output)?;
    if values.len() > MAX_CATEGORY_RECORDS {
        return Err("record-count-exceeds-bound".to_string());
    }
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        let raw_name = string_field(&value, &["Name", "name"])?;
        let name = validate_resource_name(&raw_name, "volume")?;
        records.push(VolumeRecord { name });
    }
    Ok(records)
}

const BUILTIN_NETWORK_NAMES: [&str; 4] = ["bridge", "host", "none", "podman"];

fn parse_network_records(output: &str) -> Result<Vec<NetworkRecord>, String> {
    let values = split_json_envelopes(output)?;
    if values.len() > MAX_CATEGORY_RECORDS {
        return Err("record-count-exceeds-bound".to_string());
    }
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        let raw_id = match value
            .get("ID")
            .or_else(|| value.get("Id"))
            .or_else(|| value.get("id"))
        {
            Some(Value::String(text)) => Some(text.clone()),
            _ => None,
        };
        let raw_name = string_field(&value, &["Name", "name"])?;
        let name = validate_resource_name(&raw_name, "network")?;
        let driver = string_field(&value, &["Driver", "driver"])?.to_lowercase();
        records.push(NetworkRecord {
            id: raw_id,
            name,
            driver,
        });
    }
    Ok(records)
}

fn classify_network_candidates<'a>(
    records: &'a [NetworkRecord],
    attached_network_names: &[String],
) -> Result<(u64, Vec<&'a NetworkRecord>), String> {
    let mut candidates = Vec::new();
    for record in records {
        if BUILTIN_NETWORK_NAMES.contains(&record.name.as_str())
            || matches!(record.driver.as_str(), "host" | "null")
        {
            continue;
        }
        if attached_network_names.contains(&record.name) {
            continue;
        }
        candidates.push(record);
    }
    let total = u64::try_from(records.len()).map_err(|_| "record-count-overflow".to_string())?;
    Ok((total, candidates))
}

fn network_has_attached_containers(output: &str) -> Result<bool, String> {
    let value = serde_json::from_str::<Value>(output.trim())
        .map_err(|error| format!("invalid-network-inspect-json:{error}"))?;
    let network = match &value {
        Value::Array(items) => items
            .first()
            .ok_or_else(|| "network-inspect-empty".to_string())?,
        object @ Value::Object(_) => object,
        _ => return Err("invalid-network-inspect-shape".to_string()),
    };
    let containers = network
        .get("Containers")
        .or_else(|| network.get("containers"))
        .ok_or_else(|| "network-inspect-containers-missing".to_string())?;
    Ok(match containers {
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Null => false,
        _ => return Err("network-inspect-containers-invalid".to_string()),
    })
}

fn hash_frame(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn candidate_fingerprint(domain_tag: &str, ids: &[&str]) -> String {
    let mut ordered: Vec<&str> = ids.to_vec();
    ordered.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(domain_tag.as_bytes());
    for id in &ordered {
        hash_frame(&mut hasher, id.as_bytes());
    }
    lower_hex(&hasher.finalize())
}

fn approval_phrase(category: OrphanCategory, candidate_set_sha256: &str) -> String {
    format!(
        "DiskSage {} orphan prune 승인 {}",
        category.as_str(),
        candidate_set_sha256
    )
}

fn summarize_candidates(
    category: OrphanCategory,
    total_records: u64,
    candidate_ids: &[&str],
    size_sum: Option<u64>,
) -> Result<OrphanCandidateEvidence, String> {
    let candidate_records =
        u64::try_from(candidate_ids.len()).map_err(|_| "record-count-overflow".to_string())?;
    let mut sorted_ids: Vec<&str> = candidate_ids.to_vec();
    sorted_ids.sort_unstable();
    if sorted_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate-candidate-id".to_string());
    }
    Ok(OrphanCandidateEvidence {
        total_records,
        candidate_records,
        candidate_size_sum_bytes: size_sum,
        candidate_set_sha256: candidate_fingerprint(category.domain_tag(), &sorted_ids),
    })
}

fn bounded_exact_candidate_ids(mut candidate_ids: Vec<String>) -> Result<Vec<String>, String> {
    if candidate_ids.len() > MAX_EXACT_DELETE_CANDIDATES {
        return Err("exact-delete-candidate-count-exceeds-bound".to_string());
    }
    candidate_ids.sort_unstable();
    if candidate_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate-candidate-id".to_string());
    }
    Ok(candidate_ids)
}

fn redacted_exact_delete_command(
    prefix: &[String],
    category: OrphanCategory,
    has_candidates: bool,
) -> Vec<String> {
    let mut command = prefix.to_vec();
    command.extend(
        category
            .exact_delete_subcommand()
            .into_iter()
            .map(str::to_string),
    );
    if has_candidates {
        command.push("<candidate-set>".to_string());
    }
    command
}

fn drain_bounded<R: std::io::Read>(mut reader: R) -> std::io::Result<(Vec<u8>, bool)> {
    let mut buffer = [0u8; 65_536];
    let mut captured = Vec::new();
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let room = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        let retained = read.min(room);
        captured.extend_from_slice(&buffer[..retained]);
        if retained < read {
            truncated = true;
        }
    }
    Ok((captured, truncated))
}

fn join_capture(
    handle: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    label: &str,
    stream: &str,
) -> Result<(Vec<u8>, bool), String> {
    handle
        .join()
        .map_err(|_| format!("{label}-{stream}-reader-panicked"))?
        .map_err(|error| format!("{label}-{stream}:{error}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandCapture {
    status_code: i32,
    stdout: String,
    stderr: String,
}

fn command_capture(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<CommandCapture, String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("{label}-spawn:{error}"))?;
    let child_pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label}-stdout-pipe-unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label}-stderr-pipe-unavailable"))?;
    let stdout_reader = thread::spawn(move || drain_bounded(stdout));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_capture(stdout_reader, label, "stdout");
                let _ = join_capture(stderr_reader, label, "stderr");
                return Err(format!("{label}-timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_capture(stdout_reader, label, "stdout");
                let _ = join_capture(stderr_reader, label, "stderr");
                return Err(format!("{label}-wait:{error}"));
            }
        }
    };

    // The direct CLI may exit while a descendant still owns the capture pipes. The child was
    // isolated in its own process group, so terminate any such descendants before joining the
    // reader threads; otherwise a successful probe can hang until the descendant exits.
    #[cfg(unix)]
    unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    }

    let (stdout, stdout_truncated) = join_capture(stdout_reader, label, "stdout")?;
    let (stderr, stderr_truncated) = join_capture(stderr_reader, label, "stderr")?;
    if stdout_truncated || stderr_truncated {
        return Err(format!("{label}-output-too-large"));
    }
    Ok(CommandCapture {
        status_code: status.code().unwrap_or(-1),
        stdout: String::from_utf8(stdout).map_err(|_| format!("{label}-stdout-not-utf8"))?,
        stderr: String::from_utf8(stderr).map_err(|_| format!("{label}-stderr-not-utf8"))?,
    })
}

fn mutation_capture_result(
    result: Result<CommandCapture, String>,
    label: &str,
) -> Result<CommandCapture, String> {
    match result {
        Ok(output) => Ok(output),
        Err(error) if !error.starts_with(&format!("{label}-spawn:")) => Ok(CommandCapture {
            status_code: -1,
            stdout: String::new(),
            stderr: INDETERMINATE_MUTATION_OUTCOME.to_string(),
        }),
        Err(error) => Err(error),
    }
}

fn command_text(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    let output = command_capture(executable, args, timeout, label)?;
    if output.status_code != 0 {
        let flattened = output.stderr.replace(['\r', '\n'], " ");
        let detail: String = flattened.chars().take(512).collect();
        return Err(format!("{label}-failed:{detail}"));
    }
    Ok(output.stdout)
}

pub fn probe_runtime_health(target: &ContainerRuntimeTarget) -> RuntimeHealthEvidence {
    let detail_issue = (|| -> Result<(), String> {
        let prefix = target.command_prefix()?;
        let mut args: Vec<&str> = prefix.iter().skip(1).map(String::as_str).collect();
        args.push("info");
        command_text(
            &target.binary_path,
            &args,
            ORPHAN_COMMAND_TIMEOUT,
            "runtime-info",
        )
        .map(|_| ())
    })()
    .err();
    RuntimeHealthEvidence {
        kind: target.kind,
        display_name: target.display_name(),
        healthy: detail_issue.is_none(),
        detail_issue,
    }
}

fn audit_category(target: &ContainerRuntimeTarget, category: OrphanCategory) -> OrphanCategoryPlan {
    let build_issue_plan = |issue: String| OrphanCategoryPlan {
        category,
        evidence_complete: false,
        issue: Some(issue),
        evidence: None,
        approval_phrase: None,
        prune_command: None,
        candidate_ids: Vec::new(),
    };
    let prefix = match target.command_prefix() {
        Ok(prefix) => prefix,
        Err(error) => return build_issue_plan(error),
    };
    let outcome = (|| -> Result<(Option<OrphanCandidateEvidence>, Vec<String>), String> {
        let list_label = format!("orphan-list-{}", category.as_str());
        let mut args: Vec<&str> = prefix.iter().skip(1).map(String::as_str).collect();
        match category {
            OrphanCategory::Container => {
                args.extend(["container", "ps", "--all"]);
                if target.kind.is_docker() {
                    args.push("--no-trunc");
                }
                args.extend(["--format", "json"]);
            }
            OrphanCategory::Image if target.kind.is_docker() => {
                args.extend([
                    "images",
                    "--filter",
                    "dangling=true",
                    "--no-trunc",
                    "--format",
                    "json",
                ]);
            }
            OrphanCategory::Image => {
                args.extend([
                    "images",
                    "--filter",
                    "dangling=true",
                    "--no-trunc",
                    "--format",
                    "json",
                ]);
            }
            OrphanCategory::Volume => {
                args.extend([
                    "volume",
                    "ls",
                    "--filter",
                    "dangling=true",
                    "--format",
                    "json",
                ]);
            }
            OrphanCategory::Network => {
                args.extend(["network", "ls", "--no-trunc", "--format", "json"]);
            }
        }
        let output = command_text(
            &target.binary_path,
            &args,
            ORPHAN_COMMAND_TIMEOUT,
            &list_label,
        )?;
        let image_has_container_reference = |image_id: &str| -> Result<bool, String> {
            let filter = format!("ancestor={image_id}");
            let mut membership_args: Vec<&str> =
                prefix.iter().skip(1).map(String::as_str).collect();
            membership_args.extend(["container", "ps", "--all", "--filter", &filter]);
            if target.kind == ContainerRuntimeKind::PodmanMachine {
                // Buildah working containers are hidden without --external but still retain images.
                membership_args.push("--external");
            }
            if target.kind.is_docker() {
                membership_args.push("--no-trunc");
            }
            membership_args.extend(["--format", "json"]);
            let membership = command_text(
                &target.binary_path,
                &membership_args,
                ORPHAN_COMMAND_TIMEOUT,
                "orphan-image-container-membership",
            )?;
            Ok(!split_json_envelopes(&membership)?.is_empty())
        };
        let (evidence, candidate_ids) = match category {
            OrphanCategory::Container => {
                let records = parse_container_records(&output)?;
                let (total, candidates) = classify_container_candidates(&records)?;
                let candidate_ids: Vec<String> =
                    candidates.iter().map(|candidate| candidate.id.clone()).collect();
                let ids: Vec<&str> = candidate_ids.iter().map(String::as_str).collect();
                (
                    Some(summarize_candidates(category, total, &ids, None)?),
                    candidate_ids,
                )
            }
            OrphanCategory::Image if target.kind.is_docker() => {
                let listed_ids =
                    bounded_exact_candidate_ids(parse_docker_dangling_image_ids(&output)?)?;
                let total = u64::try_from(listed_ids.len())
                    .map_err(|_| "record-count-overflow".to_string())?;
                let mut candidate_ids = Vec::with_capacity(listed_ids.len());
                for image_id in listed_ids {
                    if !image_has_container_reference(&image_id)? {
                        candidate_ids.push(image_id);
                    }
                }
                let sizes = inspect_docker_image_sizes(target, &prefix, &candidate_ids)?;
                let size_sum = candidate_ids.iter().try_fold(0u64, |sum, image_id| {
                    sum.checked_add(
                        *sizes
                            .get(image_id)
                            .ok_or_else(|| "docker-image-size-identity-mismatch".to_string())?,
                    )
                    .ok_or_else(|| "size-overflow".to_string())
                })?;
                let refs: Vec<&str> = candidate_ids.iter().map(String::as_str).collect();
                (
                    Some(summarize_candidates(category, total, &refs, Some(size_sum))?),
                    candidate_ids,
                )
            }
            OrphanCategory::Image => {
                let records = parse_image_records(&output)?;
                let total = u64::try_from(records.len())
                    .map_err(|_| "record-count-overflow".to_string())?;
                let listed_ids = bounded_exact_candidate_ids(
                    records.iter().map(|record| record.id.clone()).collect(),
                )?;
                let mut candidate_ids = Vec::with_capacity(listed_ids.len());
                for image_id in listed_ids {
                    if !image_has_container_reference(&image_id)? {
                        candidate_ids.push(image_id);
                    }
                }
                let ids: Vec<&str> = candidate_ids.iter().map(String::as_str).collect();
                let size_sum = records
                    .iter()
                    .filter(|record| candidate_ids.binary_search(&record.id).is_ok())
                    .try_fold(0u64, |sum, record| {
                        sum.checked_add(record.size_bytes)
                            .ok_or_else(|| "size-overflow".to_string())
                    })?;
                let mut evidence = summarize_candidates(category, total, &ids, None)?;
                evidence.candidate_size_sum_bytes = Some(size_sum);
                (Some(evidence), candidate_ids)
            }
            OrphanCategory::Volume => {
                let records = parse_volume_records(&output)?;
                let total = u64::try_from(records.len())
                    .map_err(|_| "record-count-overflow".to_string())?;
                let candidate_ids: Vec<String> =
                    records.iter().map(|record| record.name.clone()).collect();
                let ids: Vec<&str> = candidate_ids.iter().map(String::as_str).collect();
                (
                    Some(summarize_candidates(category, total, &ids, None)?),
                    candidate_ids,
                )
            }
            OrphanCategory::Network => {
                let records = parse_network_records(&output)?;
                let mut attached: Vec<String> = Vec::new();
                let mut inspected_candidates = 0usize;
                for record in &records {
                    if BUILTIN_NETWORK_NAMES.contains(&record.name.as_str())
                        || matches!(record.driver.as_str(), "host" | "null")
                    {
                        continue;
                    }
                    if inspected_candidates >= MAX_NETWORK_CANDIDATES {
                        return Err("network-candidate-count-exceeds-bound".to_string());
                    }
                    inspected_candidates = inspected_candidates.saturating_add(1);
                    let network_id = record
                        .id
                        .as_deref()
                        .ok_or_else(|| "network-id-missing".to_string())
                        .and_then(|id| normalize_hex_id(id, "network"))?;
                    let has_attached_containers = if target.kind
                        == ContainerRuntimeKind::PodmanMachine
                    {
                        let filter = format!("network={network_id}");
                        let mut membership_args: Vec<&str> =
                            prefix.iter().skip(1).map(String::as_str).collect();
                        membership_args.extend([
                            "container",
                            "ps",
                            "--all",
                            "--filter",
                            &filter,
                            "--format",
                            "json",
                        ]);
                        !split_json_envelopes(&command_text(
                            &target.binary_path,
                            &membership_args,
                            ORPHAN_COMMAND_TIMEOUT,
                            "orphan-network-membership",
                        )?)?
                        .is_empty()
                    } else {
                        let mut inspect_args: Vec<&str> =
                            prefix.iter().skip(1).map(String::as_str).collect();
                        inspect_args.extend(["network", "inspect", &network_id]);
                        network_has_attached_containers(&command_text(
                            &target.binary_path,
                            &inspect_args,
                            ORPHAN_COMMAND_TIMEOUT,
                            "orphan-network-inspect",
                        )?)?
                    };
                    if has_attached_containers {
                        attached.push(record.name.clone());
                    }
                }
                let (total, candidates) = classify_network_candidates(&records, &attached)?;
                let candidate_ids: Vec<String> = candidates
                    .iter()
                    .map(|candidate| {
                        candidate
                            .id
                            .as_deref()
                            .ok_or_else(|| "network-id-missing".to_string())
                            .and_then(|id| normalize_hex_id(id, "network"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let ids: Vec<&str> = candidate_ids.iter().map(String::as_str).collect();
                (
                    Some(summarize_candidates(category, total, &ids, None)?),
                    candidate_ids,
                )
            }
        };
        Ok((evidence, bounded_exact_candidate_ids(candidate_ids)?))
    })();
    match outcome {
        Ok((evidence, candidate_ids)) => {
            let has_candidates = evidence
                .as_ref()
                .is_some_and(|item| item.candidate_records > 0);
            let approval_phrase = match (&evidence, has_candidates) {
                (Some(item), true) => Some(approval_phrase(category, &item.candidate_set_sha256)),
                _ => None,
            };
            OrphanCategoryPlan {
                category,
                evidence_complete: true,
                issue: None,
                evidence,
                approval_phrase,
                prune_command: Some(redacted_exact_delete_command(
                    &prefix,
                    category,
                    has_candidates,
                )),
                candidate_ids,
            }
        }
        Err(issue) => build_issue_plan(issue),
    }
}

pub fn probe_container_orphans(target: &ContainerRuntimeTarget) -> ContainerOrphanPlan {
    let started = Instant::now();
    let runtime = probe_runtime_health(target);
    let categories: Vec<OrphanCategoryPlan> = if runtime.healthy {
        [
            OrphanCategory::Container,
            OrphanCategory::Image,
            OrphanCategory::Volume,
            OrphanCategory::Network,
        ]
        .into_iter()
        .map(|category| audit_category(target, category))
        .collect()
    } else {
        Vec::new()
    };
    let issues: Vec<String> = categories
        .iter()
        .filter_map(|plan| {
            plan.issue
                .clone()
                .map(|issue| format!("{}:{issue}", plan.category.as_str()))
        })
        .chain(runtime.detail_issue.clone())
        .collect();
    ContainerOrphanPlan {
        schema_kind: CONTAINER_ORPHAN_SCHEMA_KIND,
        schema_version: CONTAINER_ORPHAN_SCHEMA_VERSION,
        platform: std::env::consts::OS,
        evidence_complete: runtime.healthy && categories.iter().all(|plan| plan.evidence_complete),
        elapsed_ms: started.elapsed().as_millis() as u64,
        runtime,
        categories,
        issues,
    }
}

fn host_available_bytes(observed_at_ms: u64) -> Option<u64> {
    std::env::current_dir()
        .ok()
        .and_then(|path| crate::volume_pressure::snapshot_volume(&path, observed_at_ms).ok())
        .map(|snapshot| snapshot.available_bytes)
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn execute_container_orphan_prune(
    target: &ContainerRuntimeTarget,
    category: OrphanCategory,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ContainerOrphanPruneExecution, String> {
    if executed_at_ms == 0 {
        return Err("orphan-prune-time-invalid".into());
    }
    if rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("orphan-prune-rationale-invalid".into());
    }
    let prefix = target.command_prefix()?;
    let plan = audit_category(target, category);
    if !plan.evidence_complete {
        return Err(format!(
            "orphan-prune-evidence-incomplete:{}",
            plan.issue.unwrap_or_else(|| "unknown".into())
        ));
    }
    let evidence = plan
        .evidence
        .as_ref()
        .ok_or("orphan-prune-evidence-missing")?;
    if evidence.candidate_records == 0 {
        return Err("orphan-prune-empty-candidate-set".into());
    }
    let candidate_count = usize::try_from(evidence.candidate_records)
        .map_err(|_| "record-count-overflow".to_string())?;
    if plan.candidate_ids.len() != candidate_count {
        return Err("orphan-prune-candidate-set-internal-mismatch".into());
    }
    let expected_phrase = approval_phrase(category, &evidence.candidate_set_sha256);
    if confirmation_phrase != expected_phrase {
        return Err("orphan-prune-confirmation-mismatch".into());
    }

    let before_available_bytes = host_available_bytes(executed_at_ms);
    let mut owned_args: Vec<String> = prefix.iter().skip(1).cloned().collect();
    owned_args.extend(
        category
            .exact_delete_subcommand()
            .into_iter()
            .map(str::to_string),
    );
    owned_args.extend(plan.candidate_ids.iter().cloned());
    let args: Vec<&str> = owned_args.iter().map(String::as_str).collect();
    let label = format!("orphan-prune-{}", category.as_str());
    let output = mutation_capture_result(
        command_capture(&target.binary_path, &args, ORPHAN_COMMAND_TIMEOUT, &label),
        &label,
    )?;
    let after_observed_at_ms = current_epoch_ms().max(executed_at_ms);
    let after_available_bytes = host_available_bytes(after_observed_at_ms);
    let observed_available_gain_bytes = before_available_bytes
        .zip(after_available_bytes)
        .and_then(|(before, after)| after.checked_sub(before));
    Ok(ContainerOrphanPruneExecution {
        schema_version: CONTAINER_ORPHAN_SCHEMA_VERSION,
        runtime_display_name: target.display_name(),
        category,
        candidate_set_sha256: evidence.candidate_set_sha256.clone(),
        command: redacted_exact_delete_command(&prefix, category, true),
        status_code: output.status_code,
        stdout: output.stdout,
        stderr: output.stderr,
        output_truncated: false,
        executed: true,
        executed_at_ms,
        before_available_bytes,
        after_available_bytes,
        observed_available_gain_bytes,
        rationale: rationale.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCKER_ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DOCKER_ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn scope_name_rejects_option_injection() {
        assert!(!valid_scope_name(""));
        assert!(!valid_scope_name("-flag"));
        assert!(!valid_scope_name("."));
        assert!(!valid_scope_name(".."));
        assert!(!valid_scope_name("has space"));
        assert!(!valid_scope_name(&"x".repeat(129)));
        assert!(valid_scope_name("colima"));
        assert!(valid_scope_name("podman-machine-default"));
    }

    #[test]
    fn target_new_rejects_unsafe_scope() {
        let error = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerColimaContext,
            PathBuf::from("docker"),
            Some("-evil".to_string()),
        )
        .unwrap_err();
        assert_eq!(error, "unsafe-runtime-scope-name");
    }

    #[test]
    fn docker_native_prefix_has_no_flags() {
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerNative,
            PathBuf::from("docker"),
            None,
        )
        .unwrap();
        assert_eq!(target.command_prefix().unwrap(), vec!["docker".to_string()]);
        assert_eq!(target.display_name(), "docker (docker-native)");
    }

    #[test]
    fn colima_prefix_uses_context_flag_and_display_includes_scope() {
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerColimaContext,
            PathBuf::from("/usr/local/bin/docker"),
            Some("colima".to_string()),
        )
        .unwrap();
        assert_eq!(
            target.command_prefix().unwrap(),
            vec![
                "/usr/local/bin/docker".to_string(),
                "--context".to_string(),
                "colima".to_string()
            ]
        );
        assert_eq!(target.display_name(), "docker colima");
    }

    #[test]
    fn podman_prefix_uses_connection_flag() {
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::PodmanMachine,
            PathBuf::from("podman"),
            Some("podman-machine-default".to_string()),
        )
        .unwrap();
        assert_eq!(
            target.command_prefix().unwrap(),
            vec![
                "podman".to_string(),
                "--connection".to_string(),
                "podman-machine-default".to_string()
            ]
        );
    }

    #[test]
    fn scoped_kinds_require_a_scope_name() {
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::PodmanMachine,
            PathBuf::from("podman"),
            None,
        )
        .unwrap();
        assert_eq!(
            target.command_prefix().unwrap_err(),
            "missing-scope-for-podman-machine"
        );
    }

    #[test]
    fn empty_binary_path_fails_closed() {
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerNative,
            PathBuf::from(""),
            None,
        )
        .unwrap();
        assert_eq!(
            target.command_prefix().unwrap_err(),
            "unsafe-runtime-binary-path"
        );
    }

    #[test]
    fn envelopes_accept_array_and_ndjson() {
        let array = r#"[{"ID":"a"},{"ID":"b"}]"#;
        assert_eq!(split_json_envelopes(array).unwrap().len(), 2);
        let ndjson = "{\"ID\":\"a\"}\n{\"ID\":\"b\"}\n";
        assert_eq!(split_json_envelopes(ndjson).unwrap().len(), 2);
        assert!(split_json_envelopes("").unwrap().is_empty());
        assert!(split_json_envelopes("  \n\t").unwrap().is_empty());
        assert!(split_json_envelopes("{\"ID\":\"a\"\n{oops}")
            .unwrap_err()
            .starts_with("invalid-json-record:"));
    }

    #[test]
    fn docker_container_records_parse_from_ndjson() {
        let output = format!(
            "{{\"Command\":\"sleep\",\"CreatedAt\":\"now\",\"ID\":\"sha256:{DOCKER_ID_A}\",\"Image\":\"img\",\"Names\":[\"web\"],\"State\":\"exited\"}}\n{{\"Command\":\"top\",\"ID\":\"{DOCKER_ID_B}\",\"State\":\"running\",\"Names\":[\"db\"]}}\n"
        );
        let records = parse_container_records(&output).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, DOCKER_ID_A);
        assert_eq!(records[0].state, "exited");
        assert_eq!(records[0].names, vec!["web"]);
        let (total, candidates) = classify_container_candidates(&records).unwrap();
        assert_eq!(total, 2);
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn docker_container_plain_name_is_one_name() {
        let output = format!(
            "{{\"ID\":\"{DOCKER_ID_A}\",\"State\":\"exited\",\"Names\":\"web\"}}"
        );
        assert_eq!(parse_container_records(&output).unwrap()[0].names, ["web"]);
    }

    #[test]
    fn podman_container_records_parse_from_array_with_encoded_names() {
        let output = format!(
            "[{{\"Id\":\"{DOCKER_ID_A}\",\"State\":\"created\",\"Names\":\"[\\\"worker\\\"]\"}},{{\"Id\":\"{DOCKER_ID_B}\",\"State\":\"paused\",\"Names\":[\"x\"]}}]"
        );
        let records = parse_container_records(&output).unwrap();
        assert_eq!(records[0].names, vec!["worker"]);
        let (_, candidates) = classify_container_candidates(&records).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].state, "created");
    }

    #[test]
    fn unknown_container_state_fails_closed() {
        let output = format!("{{\"ID\":\"{DOCKER_ID_A}\",\"State\":\"zombie\",\"Names\":[]}}");
        let error =
            classify_container_candidates(&parse_container_records(&output).unwrap()).unwrap_err();
        assert_eq!(error, "unknown-container-state:zombie");
    }

    #[test]
    fn invalid_container_id_fails_closed() {
        let output = "{\"ID\":\"short\",\"State\":\"exited\",\"Names\":[]}";
        assert_eq!(
            parse_container_records(output).unwrap_err(),
            "container-invalid-id"
        );
    }

    #[test]
    fn uppercase_container_id_fails_closed() {
        let output =
            "{\"ID\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\"State\":\"exited\",\"Names\":[]}";
        assert_eq!(
            parse_container_records(output).unwrap_err(),
            "container-invalid-id"
        );
    }

    #[test]
    fn malformed_container_names_fail_closed() {
        let output = format!("{{\"ID\":\"{DOCKER_ID_A}\",\"State\":\"exited\",\"Names\":5}}");
        assert_eq!(
            parse_container_records(&output).unwrap_err(),
            "container-names-invalid"
        );
    }

    #[test]
    fn record_count_bound_is_enforced_for_containers() {
        let mut output = String::new();
        for index in 0..(MAX_CATEGORY_RECORDS + 1) {
            output.push_str(&format!(
                "{{\"ID\":\"{index:064x}\",\"State\":\"exited\",\"Names\":[]}}\n"
            ));
        }
        assert_eq!(
            parse_container_records(&output).unwrap_err(),
            "record-count-exceeds-bound"
        );
    }

    #[test]
    fn docker_image_records_coerce_string_numbers_and_fail_on_negative() {
        let ok = format!(
            "{{\"Containers\":\"0\",\"ID\":\"sha256:{DOCKER_ID_A}\",\"RepoTags\":[],\"RepoDigests\":[\"a@sha256:x\"],\"Size\":\"123\"}}\n{{\"Containers\":\"-1\",\"ID\":\"{DOCKER_ID_B}\",\"RepoTags\":[],\"RepoDigests\":[],\"Size\":\"5\"}}"
        );
        let error = parse_image_records(&ok).unwrap_err();
        assert_eq!(error, "json-field-invalid:Containers");
    }

    #[test]
    fn docker_dangling_image_records_bind_only_full_ids() {
        let documented = format!(
            "{{\"Containers\":\"N/A\",\"ID\":\"{DOCKER_ID_A}\",\"Repository\":\"<none>\",\"Size\":\"72.9MB\",\"Tag\":\"<none>\"}}"
        );
        assert_eq!(
            parse_docker_dangling_image_ids(&documented).unwrap(),
            vec![DOCKER_ID_A.to_string()]
        );
        assert_eq!(
            parse_docker_dangling_image_ids("{\"ID\":\"a762a2b37a1d\"}").unwrap_err(),
            "image-invalid-id"
        );
    }

    #[test]
    fn docker_image_size_parser_accepts_only_numeric_inspect_sizes() {
        let output = format!(
            "{{\"Id\":\"sha256:{DOCKER_ID_A}\",\"Size\":72900000}}\n{{\"ID\":\"{DOCKER_ID_B}\",\"Size\":5}}"
        );
        let sizes = parse_docker_image_sizes(&output).unwrap();
        assert_eq!(sizes.get(DOCKER_ID_A), Some(&72_900_000));
        assert_eq!(sizes.get(DOCKER_ID_B), Some(&5));
        let missing = format!("{{\"Id\":\"{DOCKER_ID_A}\"}}");
        assert_eq!(
            parse_docker_image_sizes(&missing).unwrap_err(),
            "json-field-invalid-or-missing:Size"
        );
        let human = format!("{{\"Id\":\"{DOCKER_ID_A}\",\"Size\":\"72.9MB\"}}");
        assert_eq!(
            parse_docker_image_sizes(&human).unwrap_err(),
            "json-field-invalid-or-missing:Size"
        );
        let duplicate = format!(
            "{{\"Id\":\"{DOCKER_ID_A}\",\"Size\":1}}\n{{\"Id\":\"{DOCKER_ID_A}\",\"Size\":2}}"
        );
        assert_eq!(
            parse_docker_image_sizes(&duplicate).unwrap_err(),
            "duplicate-image-id"
        );
    }

    #[test]
    fn image_orphans_require_zero_references_and_no_tags() {
        let referenced = format!(
            "{{\"Containers\":\"2\",\"ID\":\"{DOCKER_ID_A}\",\"RepoTags\":[\"img:latest\"],\"RepoDigests\":[],\"Size\":\"100\"}}"
        );
        let tagged_unused = format!(
            "{{\"Containers\":\"0\",\"ID\":\"{DOCKER_ID_A}\",\"RepoTags\":[\"img:v2\"],\"RepoDigests\":[],\"Size\":\"100\"}}"
        );
        let dangling = format!(
            "{{\"Containers\":0,\"ID\":\"{DOCKER_ID_A}\",\"RepoTags\":null,\"RepoDigests\":[],\"Size\":100}}"
        );
        let orphan_count = |text: &str| {
            let records = parse_image_records(text).unwrap();
            classify_image_candidates(&records).unwrap().1.len()
        };
        assert_eq!(orphan_count(&referenced), 0);
        assert_eq!(orphan_count(&tagged_unused), 0);
        assert_eq!(orphan_count(&dangling), 1);
    }

    #[test]
    fn missing_container_reference_count_fails_closed_per_record() {
        let output =
            format!("{{\"ID\":\"{DOCKER_ID_A}\",\"RepoTags\":[],\"RepoDigests\":[],\"Size\":10}}");
        let records = parse_image_records(&output).unwrap();
        assert_eq!(
            classify_image_candidates(&records).unwrap_err(),
            format!("image-reference-count-unavailable:{}", &DOCKER_ID_A[..8])
        );
    }

    #[test]
    fn missing_size_field_fails_closed() {
        let output = format!("{{\"ID\":\"{DOCKER_ID_A}\",\"Containers\":0}}");
        assert_eq!(
            parse_image_records(&output).unwrap_err(),
            "json-field-missing:Size"
        );
    }

    #[test]
    fn invalid_image_id_fails_closed() {
        let output = format!("{{\"ID\":\"zzzz\",\"Containers\":0,\"Size\":1}}");
        assert_eq!(
            parse_image_records(&output).unwrap_err(),
            "image-invalid-id"
        );
    }

    #[test]
    fn volume_names_parse_from_both_envelopes() {
        let ndjson = "{\"Availability\":\"active\",\"Driver\":\"local\",\"Name\":\"data-vol\"}\n";
        assert_eq!(parse_volume_records(ndjson).unwrap()[0].name, "data-vol");
        let array = "[{\"name\":\"cache-vol\",\"driver\":\"local\"}]";
        assert_eq!(parse_volume_records(array).unwrap()[0].name, "cache-vol");
    }

    #[test]
    fn unsafe_volume_names_fail_closed() {
        let overlong = format!("{{\"Name\":\"{}\"}}", "v".repeat(201));
        assert_eq!(
            parse_volume_records(&overlong).unwrap_err(),
            "volume-invalid-name"
        );
        assert_eq!(
            parse_volume_records("{\"Name\":\"\"}").unwrap_err(),
            "volume-invalid-name"
        );
    }

    #[test]
    fn network_records_parse_docker_casing_and_podman_casing() {
        let docker = format!(
            "[{{\"Driver\":\"bridge\",\"ID\":\"net-id-1\",\"Name\":\"app-net\"}},{{\"Driver\":\"host\",\"ID\":\"h\",\"Name\":\"host\"}}]"
        );
        let records = parse_network_records(&docker).unwrap();
        assert_eq!(records.len(), 2);
        let attached: Vec<String> = Vec::new();
        let (total, candidates) = classify_network_candidates(&records, &attached).unwrap();
        assert_eq!(total, 2);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "app-net");

        let podman = "[{\"driver\":\"bridge\",\"id\":\"p1\",\"name\":\"podman\"},{\"driver\":\"bridge\",\"id\":\"p2\",\"name\":\"custom-net\"}]";
        let records = parse_network_records(podman).unwrap();
        let (_, candidates) = classify_network_candidates(&records, &attached).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "custom-net");
    }

    #[test]
    fn attached_networks_are_not_candidates() {
        let docker = format!("[{{\"Driver\":\"bridge\",\"ID\":\"1\",\"Name\":\"used-net\"}}]");
        let records = parse_network_records(&docker).unwrap();
        let attached = vec!["used-net".to_string()];
        let (_, candidates) = classify_network_candidates(&records, &attached).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn network_attached_container_detection_covers_shapes() {
        let docker_empty = r#"[{"Containers":{},"Name":"x"}]"#;
        assert!(!network_has_attached_containers(docker_empty).unwrap());
        let docker_full =
            format!(r#"[{{"Containers":{{"endpoint-1":{{"Name":"web"}}}},"Name":"x"}}]"#);
        assert!(network_has_attached_containers(&docker_full).unwrap());
        let podman_empty = r#"{"containers":[],"name":"y"}"#;
        assert!(!network_has_attached_containers(podman_empty).unwrap());
        let podman_full = r#"{"containers":["c1"],"name":"y"}"#;
        assert!(network_has_attached_containers(podman_full).unwrap());
        let null_containers = r#"{"containers":null}"#;
        assert!(!network_has_attached_containers(null_containers).unwrap());
    }

    #[test]
    fn network_inspect_failures_are_typed() {
        assert!(network_has_attached_containers("not json")
            .unwrap_err()
            .starts_with("invalid-network-inspect-json:"));
        assert_eq!(
            network_has_attached_containers("[]").unwrap_err(),
            "network-inspect-empty"
        );
        assert_eq!(
            network_has_attached_containers("{\"Name\":\"x\"}").unwrap_err(),
            "network-inspect-containers-missing"
        );
        assert_eq!(
            network_has_attached_containers("{\"Containers\":true}").unwrap_err(),
            "network-inspect-containers-invalid"
        );
        assert_eq!(
            network_has_attached_containers("42").unwrap_err(),
            "invalid-network-inspect-shape"
        );
    }

    #[test]
    fn unsafe_network_names_fail_closed() {
        let overlong = format!(
            "[{{\"driver\":\"bridge\",\"id\":\"1\",\"name\":\"{}\"}}]",
            "n".repeat(201)
        );
        assert_eq!(
            parse_network_records(&overlong).unwrap_err(),
            "network-invalid-name"
        );
        assert_eq!(
            parse_network_records("[{\"driver\":\"bridge\",\"name\":\"-danger\"}]")
                .unwrap_err(),
            "network-invalid-name"
        );
    }

    #[test]
    fn fingerprint_binds_sorted_identity_set_and_domain() {
        let ids = vec![DOCKER_ID_B, DOCKER_ID_A];
        let first = candidate_fingerprint("domain-a", &ids);
        let reordered = candidate_fingerprint("domain-a", &[DOCKER_ID_A, DOCKER_ID_B]);
        assert_eq!(first, reordered);
        let other_domain = candidate_fingerprint("domain-b", &ids);
        assert_ne!(first, other_domain);
        let other_set = candidate_fingerprint("domain-a", &[DOCKER_ID_A, DOCKER_ID_A]);
        assert_ne!(first, other_set);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn approval_phrases_embed_category_and_fingerprint() {
        let phrase = approval_phrase(OrphanCategory::Volume, "abc123");
        assert_eq!(phrase, "DiskSage volume orphan prune 승인 abc123");
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_descendants_that_hold_capture_pipes() {
        let started = Instant::now();
        let result = command_capture(
            Path::new("/bin/sh"),
            &["-c", "sleep 30 & wait"],
            Duration::from_millis(100),
            "descendant-timeout",
        );

        assert_eq!(result.unwrap_err(), "descendant-timeout-timeout");
        assert!(started.elapsed() < Duration::from_secs(2));

        let receipt = mutation_capture_result(
            Err("descendant-timeout-timeout".into()),
            "descendant-timeout",
        )
        .unwrap();
        assert_eq!(receipt.status_code, -1);
        assert_eq!(receipt.stderr, INDETERMINATE_MUTATION_OUTCOME);
        assert!(mutation_capture_result(
            Err("descendant-timeout-spawn:unavailable".into()),
            "descendant-timeout",
        )
        .is_err());
    }

    #[test]
    fn summarize_candidates_sorts_and_detects_duplicates() {
        let evidence = summarize_candidates(
            OrphanCategory::Container,
            3,
            &[DOCKER_ID_B, DOCKER_ID_A],
            Some(7),
        )
        .unwrap();
        assert_eq!(evidence.total_records, 3);
        assert_eq!(evidence.candidate_records, 2);
        assert_eq!(evidence.candidate_size_sum_bytes, Some(7));
        let duplicate = summarize_candidates(
            OrphanCategory::Container,
            2,
            &[DOCKER_ID_A, DOCKER_ID_A],
            None,
        )
        .unwrap_err();
        assert_eq!(duplicate, "duplicate-candidate-id");
    }

    #[test]
    fn exact_delete_candidate_bound_is_enforced() {
        let ids: Vec<String> = (0..=MAX_EXACT_DELETE_CANDIDATES)
            .map(|index| format!("{index:064x}"))
            .collect();
        assert_eq!(
            bounded_exact_candidate_ids(ids).unwrap_err(),
            "exact-delete-candidate-count-exceeds-bound"
        );
    }

    #[test]
    fn category_metadata_is_stable() {
        assert_eq!(OrphanCategory::Container.as_str(), "container");
        assert_eq!(OrphanCategory::Image.as_str(), "image");
        assert_eq!(OrphanCategory::Volume.as_str(), "volume");
        assert_eq!(OrphanCategory::Network.as_str(), "network");
        assert_eq!(
            OrphanCategory::Container.exact_delete_subcommand(),
            ["container", "rm"]
        );
        assert_eq!(
            OrphanCategory::Image.exact_delete_subcommand(),
            ["image", "rm"]
        );
        assert_eq!(
            OrphanCategory::Volume.exact_delete_subcommand(),
            ["volume", "rm"]
        );
        assert_eq!(
            OrphanCategory::Network.exact_delete_subcommand(),
            ["network", "rm"]
        );
        assert_eq!(
            serde_json::to_value(OrphanCategory::Container).unwrap(),
            serde_json::json!("container")
        );
        assert_eq!(
            ContainerRuntimeKind::PodmanMachine.as_str(),
            "podman-machine"
        );
    }
}
