//! Apple Photos duplicate review through PhotoKit; managed library files are never traversed.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use tauri::Manager;

const SCHEMA_VERSION: u32 = 1;
const MAX_INVENTORY_ASSETS: u32 = 10_000;
const MAX_RESOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PLAN_AGE_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosAuthorization {
    pub authorization: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosAssetEvidence {
    pub local_identifier: String,
    pub width_pixels: u64,
    pub height_pixels: u64,
    pub pixel_count: u64,
    pub creation_ms: Option<u64>,
    pub modification_ms: Option<u64>,
    pub state: String,
    pub blocker: Option<String>,
    pub content_sha256: Option<String>,
    pub encoded_bytes: Option<u64>,
    pub original_filename: Option<String>,
    pub uniform_type_identifier: Option<String>,
    pub resource_type: Option<i64>,
    pub metadata_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosExactGroup {
    pub content_sha256: String,
    pub members: Vec<PhotosAssetEvidence>,
    pub keeper_required: bool,
    pub automatic_delete_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosDuplicateInventory {
    pub authorization: String,
    pub observed_at_ms: Option<u64>,
    pub inventory_fingerprint: Option<String>,
    pub evidence_complete: bool,
    pub inventory_truncated: bool,
    pub next_action: String,
    pub assets: Vec<PhotosAssetEvidence>,
    pub exact_groups: Vec<PhotosExactGroup>,
    pub unavailable_count: u64,
    pub near_duplicate_evidence: Option<String>,
    #[serde(default)]
    pub inventory_total_count: Option<u64>,
    #[serde(default)]
    pub inventory_page_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosInventoryPage {
    pub authorization: String,
    pub observed_at_ms: u64,
    pub total_count: u64,
    pub inventory_identity: String,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub native_completion_observed: bool,
    pub page_duration_ms: u64,
    pub assets: Vec<PhotosAssetEvidence>,
    pub unavailable_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosInventoryCheckpoint {
    pub next_offset: u64,
    pub total_count: u64,
    pub inventory_identity: String,
}

/// Merge one native-completed PhotoKit page into a resumable read-only checkpoint.
pub fn merge_inventory_page(
    previous: Option<PhotosDuplicateInventory>,
    page: PhotosInventoryPage,
) -> Result<PhotosDuplicateInventory, String> {
    if !page.native_completion_observed || page.assets.len() > 1 {
        return Err("photos-page-native-completion-required".into());
    }
    let mut inventory = previous.unwrap_or(PhotosDuplicateInventory {
        authorization: page.authorization.clone(),
        observed_at_ms: Some(page.observed_at_ms),
        inventory_fingerprint: None,
        evidence_complete: false,
        inventory_truncated: true,
        next_action: "사진 확인을 계속하세요.".into(),
        assets: Vec::new(),
        exact_groups: Vec::new(),
        unavailable_count: 0,
        near_duplicate_evidence: Some("unavailable-without-measured-content-equivalence".into()),
        inventory_total_count: Some(page.total_count),
        inventory_page_identity: Some(page.inventory_identity.clone()),
    });
    if inventory.authorization != page.authorization
        || inventory.inventory_total_count != Some(page.total_count)
        || inventory.inventory_page_identity.as_deref() != Some(&page.inventory_identity)
        || inventory.assets.len() as u64 != page.offset
        || page
            .next_offset
            .is_some_and(|next| next != page.offset + page.assets.len() as u64)
        || page.total_count < page.offset + page.assets.len() as u64
    {
        return Err("photos-page-checkpoint-mismatch".into());
    }
    inventory.assets.extend(page.assets);
    inventory.unavailable_count = inventory
        .unavailable_count
        .checked_add(page.unavailable_count)
        .ok_or("photos-page-count-overflow")?;
    inventory.observed_at_ms = Some(page.observed_at_ms);
    let complete = page.next_offset.is_none() && inventory.assets.len() as u64 == page.total_count;
    inventory.inventory_truncated = !complete;
    inventory.evidence_complete = complete
        && inventory.unavailable_count == 0
        && matches!(inventory.authorization.as_str(), "authorized" | "limited");
    inventory.next_action = if complete {
        if inventory.unavailable_count > 0 {
            "사진 앱에서 이 Mac에 없는 원본을 다운로드한 뒤 다시 확인하세요.".into()
        } else {
            "검사가 끝났습니다. 정확한 사본 그룹을 검토하세요.".into()
        }
    } else {
        format!(
            "{}개 중 {}개를 확인했습니다. 계속하려면 사진 확인을 누르세요.",
            page.total_count,
            inventory.assets.len()
        )
    };
    let mut groups = BTreeMap::<String, Vec<PhotosAssetEvidence>>::new();
    for asset in &inventory.assets {
        if let Some(digest) = &asset.content_sha256 {
            groups
                .entry(digest.clone())
                .or_default()
                .push(asset.clone());
        }
    }
    inventory.exact_groups = groups
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(content_sha256, members)| PhotosExactGroup {
            content_sha256,
            members,
            keeper_required: true,
            automatic_delete_allowed: false,
        })
        .collect();
    inventory.inventory_fingerprint = if complete {
        Some(hash_json(&(
            inventory.assets.clone(),
            inventory.unavailable_count,
        ))?)
    } else {
        None
    };
    Ok(inventory)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosKeeperSelection {
    pub content_sha256: String,
    pub keeper_local_identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosDeletionPlan {
    pub schema_version: u32,
    pub inventory_fingerprint: String,
    pub observed_at_ms: u64,
    pub plan_fingerprint: String,
    pub delete_identifiers: Vec<String>,
    pub expected_metadata_fingerprints: BTreeMap<String, String>,
    pub expected_content_sha256: BTreeMap<String, String>,
    pub logical_candidate_bytes: u64,
    pub exact_approval_phrase: String,
    pub max_resource_bytes: u64,
    pub permanent_delete_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotosDeletionReceipt {
    pub schema_version: u32,
    pub receipt_id: String,
    pub plan_fingerprint: String,
    pub executed_at_ms: u64,
    pub rationale: String,
    pub deleted_count: usize,
    pub system_confirmation_completed: bool,
    pub permanent_delete_requested: bool,
    pub next_action: String,
}

#[derive(Serialize)]
struct PhotosDeletionReceiptRecord<'a> {
    phase: &'static str,
    receipt: &'a PhotosDeletionReceipt,
}

fn append_receipt_record(
    file: &mut std::fs::File,
    phase: &'static str,
    receipt: &PhotosDeletionReceipt,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(&PhotosDeletionReceiptRecord { phase, receipt })
        .map_err(|_| "photos-receipt-serialization-failed".to_string())?;
    bytes.push(b'\n');
    file.write_all(&bytes)
        .map_err(|_| "photos-receipt-write-failed".to_string())?;
    file.sync_all()
        .map_err(|_| "photos-receipt-sync-failed".to_string())
}

fn prepare_receipt_file_with(
    mut file: std::fs::File,
    path: &std::path::Path,
    append: impl FnOnce(&mut std::fs::File) -> Result<(), String>,
) -> Result<std::fs::File, String> {
    if let Err(error) = append(&mut file) {
        // Keep the create-new handle alive until its pathname is unlinked. Closing first would
        // let a concurrent retry create the deterministic path and have this attempt remove it.
        let _ = std::fs::remove_file(path);
        drop(file);
        return Err(error);
    }
    Ok(file)
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "photos-evidence-serialization-failed")?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub fn plan_deletion(
    inventory: &PhotosDuplicateInventory,
    selections: &[PhotosKeeperSelection],
) -> Result<PhotosDeletionPlan, String> {
    if inventory.authorization != "authorized" && inventory.authorization != "limited" {
        return Err("photos-access-not-authorized".into());
    }
    if !inventory.evidence_complete
        || inventory.inventory_truncated
        || inventory.unavailable_count != 0
    {
        return Err("photos-inventory-incomplete-review-again".into());
    }
    let inventory_fingerprint = inventory
        .inventory_fingerprint
        .clone()
        .ok_or("photos-inventory-fingerprint-missing")?;
    let observed_at_ms = inventory
        .observed_at_ms
        .ok_or("photos-inventory-observation-missing")?;
    let selected = selections
        .iter()
        .map(|item| {
            (
                item.content_sha256.as_str(),
                item.keeper_local_identifier.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if selected.len() != selections.len() || selected.len() != inventory.exact_groups.len() {
        return Err("photos-one-keeper-required-per-group".into());
    }
    let mut delete_identifiers = Vec::new();
    let mut expected_metadata_fingerprints = BTreeMap::new();
    let mut expected_content_sha256 = BTreeMap::new();
    let mut logical_candidate_bytes = 0u64;
    for group in &inventory.exact_groups {
        if !group.keeper_required || group.automatic_delete_allowed || group.members.len() < 2 {
            return Err("photos-duplicate-group-invalid".into());
        }
        let keeper = selected
            .get(group.content_sha256.as_str())
            .ok_or("photos-one-keeper-required-per-group")?;
        if !group
            .members
            .iter()
            .any(|member| member.local_identifier == *keeper)
        {
            return Err("photos-keeper-not-in-group".into());
        }
        for member in &group.members {
            let fingerprint = member
                .metadata_fingerprint
                .clone()
                .ok_or("photos-member-evidence-incomplete")?;
            let content_sha256 = member
                .content_sha256
                .clone()
                .ok_or("photos-member-evidence-incomplete")?;
            if content_sha256 != group.content_sha256 {
                return Err("photos-duplicate-content-evidence-invalid".into());
            }
            if expected_metadata_fingerprints
                .insert(member.local_identifier.clone(), fingerprint)
                .is_some()
                || expected_content_sha256
                    .insert(member.local_identifier.clone(), content_sha256)
                    .is_some()
            {
                return Err("photos-member-repeated-across-groups".into());
            }
            if member.local_identifier == *keeper {
                continue;
            }
            logical_candidate_bytes = logical_candidate_bytes
                .checked_add(
                    member
                        .encoded_bytes
                        .ok_or("photos-member-evidence-incomplete")?,
                )
                .ok_or("photos-logical-bytes-overflow")?;
            delete_identifiers.push(member.local_identifier.clone());
        }
    }
    if delete_identifiers.is_empty() {
        return Err("photos-no-duplicate-selected".into());
    }
    delete_identifiers.sort();
    let exact_approval_phrase = format!(
        "DELETE {} PHOTOS FROM PHOTOS {}",
        delete_identifiers.len(),
        &inventory_fingerprint[..inventory_fingerprint.len().min(12)]
    );
    let mut plan = PhotosDeletionPlan {
        schema_version: SCHEMA_VERSION,
        inventory_fingerprint,
        observed_at_ms,
        plan_fingerprint: String::new(),
        exact_approval_phrase,
        delete_identifiers,
        expected_metadata_fingerprints,
        expected_content_sha256,
        logical_candidate_bytes,
        max_resource_bytes: MAX_RESOURCE_BYTES,
        permanent_delete_requested: false,
    };
    plan.plan_fingerprint = hash_json(&plan)?;
    Ok(plan)
}

fn validate_execution(
    inventory: &PhotosDuplicateInventory,
    plan: &PhotosDeletionPlan,
    approval_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<(), String> {
    if plan.schema_version != SCHEMA_VERSION
        || plan.permanent_delete_requested
        || inventory.inventory_fingerprint.as_deref() != Some(&plan.inventory_fingerprint)
        || plan.plan_fingerprint.is_empty()
    {
        return Err("photos-plan-invalid".into());
    }
    let mut unsigned = plan.clone();
    unsigned.plan_fingerprint.clear();
    if hash_json(&unsigned)? != plan.plan_fingerprint {
        return Err("photos-plan-fingerprint-invalid".into());
    }
    if approval_phrase != plan.exact_approval_phrase {
        return Err("photos-exact-approval-required".into());
    }
    let rationale = rationale.trim();
    if rationale.is_empty() || rationale.chars().count() > 1_000 {
        return Err("photos-rationale-required".into());
    }
    if executed_at_ms < plan.observed_at_ms
        || executed_at_ms - plan.observed_at_ms > MAX_PLAN_AGE_MS
    {
        return Err("photos-review-expired-review-again".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        fn ds_photos_authorization_status() -> *mut c_char;
        fn ds_photos_request_authorization() -> *mut c_char;
        fn ds_photos_inventory(max_assets: u32, max_bytes: u64) -> *mut c_char;
        fn ds_photos_inventory_page(offset: u64, max_bytes: u64) -> *mut c_char;
        fn ds_photos_delete(request_json: *const c_char) -> *mut c_char;
        #[cfg(test)]
        fn ds_photos_select_still_resource_index(types: *const i64, count: usize) -> i64;
    }

    fn take_json<T: for<'de> Deserialize<'de>>(pointer: *mut c_char) -> Result<T, String> {
        if pointer.is_null() {
            return Err("photos-native-response-missing".into());
        }
        let bytes = unsafe { CStr::from_ptr(pointer).to_bytes().to_vec() };
        unsafe { libc::free(pointer.cast()) };
        serde_json::from_slice(&bytes).map_err(|_| "photos-native-response-invalid".into())
    }

    pub fn status() -> Result<PhotosAuthorization, String> {
        take_json(unsafe { ds_photos_authorization_status() })
    }

    pub fn authorize() -> Result<PhotosAuthorization, String> {
        take_json(unsafe { ds_photos_request_authorization() })
    }

    pub fn inventory() -> Result<PhotosDuplicateInventory, String> {
        take_json(unsafe { ds_photos_inventory(MAX_INVENTORY_ASSETS, MAX_RESOURCE_BYTES) })
    }

    pub fn inventory_page(offset: u64) -> Result<PhotosInventoryPage, String> {
        take_json(unsafe { ds_photos_inventory_page(offset, MAX_RESOURCE_BYTES) })
    }

    #[derive(Deserialize)]
    struct NativeDeleteResult {
        deleted_identifiers: Option<Vec<String>>,
        system_confirmation_completed: Option<bool>,
        error: Option<String>,
    }

    pub fn delete(plan: &PhotosDeletionPlan) -> Result<(), String> {
        let json =
            CString::new(serde_json::to_vec(plan).map_err(|_| "photos-delete-request-invalid")?)
                .map_err(|_| "photos-delete-request-invalid")?;
        let response: NativeDeleteResult = take_json(unsafe { ds_photos_delete(json.as_ptr()) })?;
        if let Some(error) = response.error {
            return Err(error);
        }
        let deleted = response
            .deleted_identifiers
            .ok_or("photos-delete-result-incomplete")?;
        if response.system_confirmation_completed != Some(true)
            || deleted.iter().cloned().collect::<BTreeSet<_>>()
                != plan.delete_identifiers.iter().cloned().collect()
        {
            return Err("photos-delete-result-incomplete".into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn select_still_resource(types: &[i64]) -> i64 {
        unsafe { ds_photos_select_still_resource_index(types.as_ptr(), types.len()) }
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;
    pub fn status() -> Result<PhotosAuthorization, String> {
        Err("photos-library-macos-only".into())
    }
    pub fn authorize() -> Result<PhotosAuthorization, String> {
        Err("photos-library-macos-only".into())
    }
    pub fn inventory() -> Result<PhotosDuplicateInventory, String> {
        Err("photos-library-macos-only".into())
    }
    pub fn inventory_page(_: u64) -> Result<PhotosInventoryPage, String> {
        Err("photos-library-macos-only".into())
    }
    pub fn delete(_: &PhotosDeletionPlan) -> Result<(), String> {
        Err("photos-library-macos-only".into())
    }
}

#[tauri::command]
pub async fn photos_authorization_status() -> Result<PhotosAuthorization, String> {
    tauri::async_runtime::spawn_blocking(native::status)
        .await
        .map_err(|_| "photos-operation-interrupted".to_string())?
}

/// Request read/write permission only when invoked by the customer's Photos connection action.
#[tauri::command]
pub async fn request_photos_authorization() -> Result<PhotosAuthorization, String> {
    tauri::async_runtime::spawn_blocking(native::authorize)
        .await
        .map_err(|_| "photos-operation-interrupted".to_string())?
}

#[tauri::command]
pub async fn inspect_photos_duplicates() -> Result<PhotosDuplicateInventory, String> {
    tauri::async_runtime::spawn_blocking(native::inventory)
        .await
        .map_err(|_| "photos-operation-interrupted".to_string())?
}

#[tauri::command]
pub async fn inspect_photos_duplicates_page(
    checkpoint: Option<PhotosInventoryCheckpoint>,
) -> Result<PhotosInventoryPage, String> {
    let offset = checkpoint.as_ref().map_or(0, |value| value.next_offset);
    let page = tauri::async_runtime::spawn_blocking(move || native::inventory_page(offset))
        .await
        .map_err(|_| "photos-operation-interrupted".to_string())??;
    if checkpoint.as_ref().is_some_and(|value| {
        value.total_count != page.total_count
            || value.inventory_identity != page.inventory_identity
            || value.next_offset != page.offset
    }) {
        return Err("photos-page-checkpoint-mismatch".into());
    }
    Ok(page)
}

#[tauri::command]
pub fn finalize_photos_duplicate_inventory(
    pages: Vec<PhotosInventoryPage>,
) -> Result<PhotosDuplicateInventory, String> {
    let mut inventory = None;
    for page in pages {
        inventory = Some(merge_inventory_page(inventory, page)?);
    }
    let inventory = inventory.ok_or("photos-page-checkpoint-empty")?;
    if inventory.inventory_truncated {
        return Err("photos-page-checkpoint-incomplete".into());
    }
    Ok(inventory)
}

#[tauri::command]
pub fn plan_photos_duplicate_deletion(
    inventory: PhotosDuplicateInventory,
    selections: Vec<PhotosKeeperSelection>,
) -> Result<PhotosDeletionPlan, String> {
    plan_deletion(&inventory, &selections)
}

#[tauri::command]
pub async fn execute_photos_duplicate_deletion(
    app: tauri::AppHandle,
    inventory: PhotosDuplicateInventory,
    plan: PhotosDeletionPlan,
    approval_phrase: String,
    rationale: String,
    executed_at_ms: u64,
) -> Result<PhotosDeletionReceipt, String> {
    validate_execution(
        &inventory,
        &plan,
        &approval_phrase,
        &rationale,
        executed_at_ms,
    )?;
    let mut receipt = PhotosDeletionReceipt {
        schema_version: SCHEMA_VERSION,
        receipt_id: String::new(),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        executed_at_ms,
        rationale: rationale.trim().to_string(),
        deleted_count: plan.delete_identifiers.len(),
        system_confirmation_completed: true,
        permanent_delete_requested: false,
        next_action: "open-recently-deleted-to-restore-or-review-space".into(),
    };
    receipt.receipt_id = hash_json(&receipt)?;
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "photos-receipt-directory-unavailable")?
        .join("photos-receipts");
    std::fs::create_dir_all(&directory).map_err(|_| "photos-receipt-directory-unavailable")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "photos-receipt-directory-unavailable")?;
    }
    let path = directory.join(format!("{}.json", receipt.receipt_id));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|_| "photos-receipt-create-failed")?;
    let mut prepared_receipt = receipt.clone();
    prepared_receipt.system_confirmation_completed = false;
    let mut file = prepare_receipt_file_with(file, &path, |file| {
        append_receipt_record(file, "prepared", &prepared_receipt)
    })?;
    let native_plan = plan.clone();
    let native_result =
        match tauri::async_runtime::spawn_blocking(move || native::delete(&native_plan)).await {
            Ok(result) => result,
            Err(_) => Err("photos-operation-interrupted".to_string()),
        };
    if let Err(error) = native_result {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    append_receipt_record(&mut file, "completed", &receipt)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, bytes: u64) -> PhotosAssetEvidence {
        PhotosAssetEvidence {
            local_identifier: id.into(),
            width_pixels: 4000,
            height_pixels: 3000,
            pixel_count: 12_000_000,
            creation_ms: Some(1),
            modification_ms: Some(2),
            state: "local-current".into(),
            blocker: None,
            content_sha256: Some("a".repeat(64)),
            encoded_bytes: Some(bytes),
            original_filename: Some(format!("{id}.heic")),
            uniform_type_identifier: Some("public.heic".into()),
            resource_type: Some(1),
            metadata_fingerprint: Some(format!("fingerprint-{id}")),
        }
    }

    fn inventory() -> PhotosDuplicateInventory {
        let members = vec![member("keep", 10), member("remove", 8)];
        PhotosDuplicateInventory {
            authorization: "authorized".into(),
            observed_at_ms: Some(100),
            inventory_fingerprint: Some("inventory".into()),
            evidence_complete: true,
            inventory_truncated: false,
            next_action: "choose-one-photo-to-keep-per-group".into(),
            assets: members.clone(),
            exact_groups: vec![PhotosExactGroup {
                content_sha256: "a".repeat(64),
                members,
                keeper_required: true,
                automatic_delete_allowed: false,
            }],
            unavailable_count: 0,
            near_duplicate_evidence: Some(
                "unavailable-without-measured-content-equivalence".into(),
            ),
            inventory_total_count: Some(2),
            inventory_page_identity: Some("stable-library".into()),
        }
    }

    fn page(offset: u64, total: u64, asset: PhotosAssetEvidence) -> PhotosInventoryPage {
        PhotosInventoryPage {
            authorization: "authorized".into(),
            observed_at_ms: 100 + offset,
            total_count: total,
            inventory_identity: "stable-library".into(),
            offset,
            next_offset: (offset + 1 < total).then_some(offset + 1),
            native_completion_observed: true,
            page_duration_ms: 7,
            assets: vec![asset],
            unavailable_count: 0,
        }
    }

    #[test]
    fn native_completed_pages_resume_and_only_finish_at_exact_total() {
        let first = merge_inventory_page(None, page(0, 2, member("keep", 10))).unwrap();
        assert!(first.inventory_truncated);
        assert!(!first.evidence_complete);
        assert!(first.inventory_fingerprint.is_none());
        let complete = merge_inventory_page(Some(first), page(1, 2, member("remove", 8))).unwrap();
        assert!(!complete.inventory_truncated);
        assert!(complete.evidence_complete);
        assert!(complete.inventory_fingerprint.is_some());
        assert_eq!(complete.exact_groups.len(), 1);
    }

    #[test]
    fn page_gap_or_missing_native_completion_is_rejected() {
        let first = merge_inventory_page(None, page(0, 2, member("keep", 10))).unwrap();
        assert_eq!(
            merge_inventory_page(Some(first.clone()), page(0, 2, member("remove", 8))).unwrap_err(),
            "photos-page-checkpoint-mismatch"
        );
        let mut incomplete = page(1, 2, member("remove", 8));
        incomplete.native_completion_observed = false;
        assert_eq!(
            merge_inventory_page(Some(first), incomplete).unwrap_err(),
            "photos-page-native-completion-required"
        );
    }

    #[test]
    fn equal_count_library_replacement_invalidates_checkpoint_identity() {
        let first = merge_inventory_page(None, page(0, 2, member("keep", 10))).unwrap();
        let mut replaced = page(1, 2, member("replacement", 8));
        replaced.inventory_identity = "changed-library".into();
        assert_eq!(
            merge_inventory_page(Some(first), replaced).unwrap_err(),
            "photos-page-checkpoint-mismatch"
        );
    }

    #[test]
    fn plan_requires_explicit_keeper_and_preserves_it() {
        let inventory = inventory();
        assert_eq!(
            plan_deletion(&inventory, &[]).unwrap_err(),
            "photos-one-keeper-required-per-group"
        );
        let plan = plan_deletion(
            &inventory,
            &[PhotosKeeperSelection {
                content_sha256: "a".repeat(64),
                keeper_local_identifier: "keep".into(),
            }],
        )
        .unwrap();
        assert_eq!(plan.delete_identifiers, ["remove"]);
        assert_eq!(plan.logical_candidate_bytes, 8);
        assert!(!plan.permanent_delete_requested);
    }

    #[test]
    fn icloud_only_asset_blocks_destructive_planning() {
        let mut inventory = inventory();
        inventory.unavailable_count = 1;
        let mut cloud = member("cloud", 20);
        cloud.state = "icloud-only-or-unavailable".into();
        cloud.blocker = Some("download-original-in-photos".into());
        cloud.content_sha256 = None;
        cloud.encoded_bytes = None;
        cloud.metadata_fingerprint = None;
        inventory.assets.push(cloud);
        inventory.evidence_complete = false;
        assert_eq!(
            plan_deletion(&inventory, &[]).unwrap_err(),
            "photos-inventory-incomplete-review-again"
        );
    }

    #[test]
    fn execution_rejects_stale_or_inexact_approval() {
        let inventory = inventory();
        let plan = plan_deletion(
            &inventory,
            &[PhotosKeeperSelection {
                content_sha256: "a".repeat(64),
                keeper_local_identifier: "keep".into(),
            }],
        )
        .unwrap();
        assert_eq!(
            validate_execution(&inventory, &plan, "wrong", "duplicate", 100).unwrap_err(),
            "photos-exact-approval-required"
        );
        assert_eq!(
            validate_execution(
                &inventory,
                &plan,
                &plan.exact_approval_phrase,
                "duplicate",
                MAX_PLAN_AGE_MS + 101
            )
            .unwrap_err(),
            "photos-review-expired-review-again"
        );
    }

    #[test]
    fn durable_receipt_records_prepared_then_completed_outcome() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("receipt.jsonl");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut receipt = PhotosDeletionReceipt {
            schema_version: SCHEMA_VERSION,
            receipt_id: "receipt".into(),
            plan_fingerprint: "plan".into(),
            executed_at_ms: 1,
            rationale: "reviewed duplicate".into(),
            deleted_count: 1,
            system_confirmation_completed: false,
            permanent_delete_requested: false,
            next_action: "open-recently-deleted-to-restore-or-review-space".into(),
        };
        append_receipt_record(&mut file, "prepared", &receipt).unwrap();
        receipt.system_confirmation_completed = true;
        append_receipt_record(&mut file, "completed", &receipt).unwrap();
        let records = std::fs::read_to_string(path).unwrap();
        let records = records
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["phase"], "prepared");
        assert_eq!(
            records[0]["receipt"]["system_confirmation_completed"],
            false
        );
        assert_eq!(records[1]["phase"], "completed");
        assert_eq!(records[1]["receipt"]["system_confirmation_completed"], true);
    }

    #[test]
    fn failed_receipt_preparation_removes_retry_blocking_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("receipt.jsonl");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let error = prepare_receipt_file_with(file, &path, |file| {
            file.write_all(b"partial").unwrap();
            assert!(OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .is_err());
            Err("photos-receipt-sync-failed".into())
        })
        .unwrap_err();
        assert_eq!(error, "photos-receipt-sync-failed");
        assert!(!path.exists());
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
    }

    #[test]
    fn native_boundary_bounds_callbacks() {
        let source = include_str!("../native/photos_bridge.m");
        assert!(source.contains("networkAccessAllowed = NO"));
        assert!(source.contains("cancelDataRequest:requestID"));
        assert!(source.contains("DSAuthorizationTimeoutNanos"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_still_selection_executes_live_photo_and_ambiguous_cases() {
        assert_eq!(native::select_still_resource(&[1, 9]), 0);
        assert_eq!(native::select_still_resource(&[9, 5]), 1);
        assert_eq!(native::select_still_resource(&[1, 1, 9]), -1);
        assert_eq!(native::select_still_resource(&[9]), -1);
    }
}
