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
        fn ds_photos_delete(request_json: *const c_char) -> *mut c_char;
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
    let mut file = options
        .open(&path)
        .map_err(|_| "photos-receipt-create-failed")?;
    let bytes =
        serde_json::to_vec_pretty(&receipt).map_err(|_| "photos-receipt-serialization-failed")?;
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
    file.write_all(&bytes)
        .map_err(|_| "photos-receipt-write-failed")?;
    file.sync_all().map_err(|_| "photos-receipt-sync-failed")?;
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
        }
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
    fn native_boundary_selects_one_still_from_live_photos_and_bounds_callbacks() {
        let source = include_str!("../native/photos_bridge.m");
        assert!(source.contains("resource.type == PHAssetResourceTypePhoto"));
        assert!(source.contains("resource.type == PHAssetResourceTypeFullSizePhoto"));
        assert!(source.contains("photos.count == 1"));
        assert!(source.contains("fullSizePhotos.count == 1"));
        assert!(source.contains("compound-photo-still-resource-ambiguous"));
        assert!(source.contains("exclude-unsupported-compound-assets-and-review-again"));
        assert!(source.contains("networkAccessAllowed = NO"));
        assert!(source.contains("cancelDataRequest:requestID"));
        assert!(source.contains("DSAuthorizationTimeoutNanos"));
    }
}
