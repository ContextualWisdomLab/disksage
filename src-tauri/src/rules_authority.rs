use std::path::Path;

pub use crate::rules_catalog::{
    cache_candidates, cache_catalog_id, cache_catalog_path, clean_targets, is_catalog_path,
    BaseDirs, CacheCandidate, CacheTarget,
};
pub(crate) use crate::rules_catalog::{modified_ms, named_cache_targets, shared_temp_root};

const CACHE_MANIFEST_AUTHORITY_VERSION: &str = "v2";

fn update_manifest_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("cache manifest field length fits u64");
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
}

fn update_manifest_name(hasher: &mut blake3::Hasher, name: &std::ffi::OsStr) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        update_manifest_bytes(hasher, name.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut bytes = Vec::new();
        for unit in name.encode_wide() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        update_manifest_bytes(hasher, &bytes);
    }
}

fn cache_root_metadata_fingerprint_inner(
    metadata: &std::fs::Metadata,
    include_unix_ctime: bool,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cache-root-metadata-v2\0");
    hasher.update(&metadata.len().to_le_bytes());
    hasher.update(&modified_ms(metadata).to_le_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hasher.update(&metadata.dev().to_le_bytes());
        hasher.update(&metadata.ino().to_le_bytes());
        if include_unix_ctime {
            hasher.update(&metadata.ctime().to_le_bytes());
            hasher.update(&metadata.ctime_nsec().to_le_bytes());
        }
        hasher.update(&metadata.blocks().to_le_bytes());
        hasher.update(&metadata.mode().to_le_bytes());
        hasher.update(&metadata.nlink().to_le_bytes());
        hasher.update(&metadata.uid().to_le_bytes());
        hasher.update(&metadata.gid().to_le_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        hasher.update(&metadata.creation_time().to_le_bytes());
        hasher.update(&metadata.last_write_time().to_le_bytes());
        hasher.update(&metadata.file_attributes().to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Full reviewed root metadata. On Unix ctime binds chmod/chown/ACL/xattr transitions that do not
/// necessarily alter mtime or the device/inode identity used by the staging move.
pub(crate) fn cache_metadata_fingerprint(metadata: &std::fs::Metadata) -> String {
    cache_root_metadata_fingerprint_inner(metadata, true)
}

/// Root metadata that is expected to remain stable across an atomic rename into DiskSage staging.
pub(crate) fn cache_root_relocation_metadata_fingerprint(metadata: &std::fs::Metadata) -> String {
    cache_root_metadata_fingerprint_inner(metadata, false)
}

fn stable_file_manifest(
    path: &Path,
    metadata: &std::fs::Metadata,
    object_id: &str,
) -> Result<String, String> {
    let name = path
        .file_name()
        .ok_or_else(|| "cache-target-manifest-name-unavailable".to_string())?;
    let mut hasher = blake3::Hasher::new();
    update_manifest_name(&mut hasher, name);
    hasher.update(&[0]);
    let metadata_fingerprint = cache_root_relocation_metadata_fingerprint(metadata);
    update_manifest_bytes(&mut hasher, metadata_fingerprint.as_bytes());
    update_manifest_bytes(&mut hasher, object_id.as_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

fn stable_directory_manifest(legacy_manifest: &str, metadata: &std::fs::Metadata) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cache-directory-authority-v2\0");
    update_manifest_bytes(&mut hasher, legacy_manifest.as_bytes());
    update_manifest_bytes(
        &mut hasher,
        cache_root_relocation_metadata_fingerprint(metadata).as_bytes(),
    );
    hasher.finalize().to_hex().to_string()
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Returns `(reviewed_root_metadata, relocation_stable_tree)` only for the current manifest
/// version. Old 64-hex manifests intentionally fail closed at cache-specific mutation boundaries.
pub(crate) fn cache_manifest_components(manifest: &str) -> Option<(&str, &str)> {
    let mut parts = manifest.split(':');
    let version = parts.next()?;
    let reviewed_root = parts.next()?;
    let stable_tree = parts.next()?;
    if parts.next().is_some()
        || version != CACHE_MANIFEST_AUTHORITY_VERSION
        || !is_hex_digest(reviewed_root)
        || !is_hex_digest(stable_tree)
    {
        return None;
    }
    Some((reviewed_root, stable_tree))
}

fn upgrade_cache_target(mut target: CacheTarget) -> Result<CacheTarget, String> {
    let path = Path::new(&target.path);
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "cache-target-metadata-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err("cache-target-type-unsupported".into());
    }
    let object_id = crate::safety::filesystem_object_id(path)
        .map_err(|_| "cache-target-identity-unavailable".to_string())?;
    if object_id != target.object_id || modified_ms(&metadata) != target.modified_ms {
        return Err("cache-target-changed-during-authority-snapshot".into());
    }

    let reviewed_root = cache_metadata_fingerprint(&metadata);
    let stable_tree = if metadata.is_dir() {
        stable_directory_manifest(&target.manifest_fingerprint, &metadata)
    } else {
        stable_file_manifest(path, &metadata, &target.object_id)?
    };
    target.manifest_fingerprint =
        format!("{CACHE_MANIFEST_AUTHORITY_VERSION}:{reviewed_root}:{stable_tree}");
    Ok(target)
}

/// Preserve the original relocation-stable manifest for generic safety callers that are not part
/// of cache cleanup. Cache cleanup obtains the stronger reviewed authority via `cache_targets` or
/// `cache_authority_target` instead.
pub(crate) fn cache_target(path: &Path) -> Result<CacheTarget, String> {
    crate::rules_catalog::cache_target(path)
}

/// Re-snapshot one staged/original cache target with the v2 reviewed-root authority contract.
pub(crate) fn cache_authority_target(path: &Path) -> Result<CacheTarget, String> {
    upgrade_cache_target(crate::rules_catalog::cache_target(path)?)
}

/// Return exact direct children with versioned destructive-authority manifests.
pub fn cache_targets(dir: &Path) -> Result<Vec<CacheTarget>, String> {
    crate::rules_catalog::cache_targets(dir)?
        .into_iter()
        .map(upgrade_cache_target)
        .collect()
}
