use std::path::Path;

#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{ErrorKind, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
fn validate_modes(file_mode: u32, directory_mode: u32) -> Result<(), String> {
    if !matches!(file_mode, 0o400 | 0o600) {
        return Err("private-directory-publication-file-mode-invalid".into());
    }
    if directory_mode != 0o700 {
        return Err("private-directory-publication-directory-mode-invalid".into());
    }
    Ok(())
}

#[cfg(unix)]
fn private_directory(metadata: &fs::Metadata, exact_mode: Option<u32>) -> Result<(), String> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("private-directory-publication-directory-unsafe".into());
    }
    let mode = metadata.permissions().mode() & 0o7777;
    if mode & 0o022 != 0 {
        return Err("private-directory-publication-directory-writable-by-others".into());
    }
    if let Some(expected) = exact_mode {
        if mode != expected {
            return Err("private-directory-publication-directory-mode-drift".into());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn discover_existing_parent(parent: &Path) -> Result<(PathBuf, fs::Metadata), String> {
    match fs::symlink_metadata(parent) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err("private-directory-publication-parent-provisioning-unavailable".into())
        }
        Err(_) => Err("private-directory-publication-anchor-unavailable".into()),
        Ok(metadata) => {
            private_directory(&metadata, None)?;
            Ok((parent.to_path_buf(), metadata))
        }
    }
}

#[cfg(unix)]
fn open_anchor(anchor: &Path, expected: &fs::Metadata) -> Result<fs::File, String> {
    let anchor_c = CString::new(anchor.as_os_str().as_bytes())
        .map_err(|_| "private-directory-publication-anchor-unavailable".to_string())?;
    let fd = unsafe {
        libc::open(
            anchor_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err("private-directory-publication-anchor-identity-drift".into());
    }
    let file = unsafe { fs::File::from_raw_fd(fd) };
    let opened = file
        .metadata()
        .map_err(|_| "private-directory-publication-anchor-unavailable".to_string())?;
    if opened.dev() != expected.dev() || opened.ino() != expected.ino() {
        return Err("private-directory-publication-anchor-identity-drift".into());
    }
    private_directory(&opened, None)?;
    Ok(file)
}

#[cfg(unix)]
fn revalidate_anchor(
    anchor: &Path,
    directory: &fs::File,
    expected_dev: u64,
    expected_ino: u64,
) -> Result<(), String> {
    let opened = directory
        .metadata()
        .map_err(|_| "private-directory-publication-anchor-unavailable".to_string())?;
    private_directory(&opened, None)?;
    if opened.dev() != expected_dev || opened.ino() != expected_ino {
        return Err("private-directory-publication-anchor-identity-drift".into());
    }
    let named = fs::symlink_metadata(anchor)
        .map_err(|_| "private-directory-publication-anchor-identity-drift".to_string())?;
    private_directory(&named, None)?;
    if named.dev() != expected_dev || named.ino() != expected_ino {
        return Err("private-directory-publication-anchor-identity-drift".into());
    }
    Ok(())
}

#[cfg(unix)]
fn invalidate_exact_record(
    file: &fs::File,
    directory: &fs::File,
    file_mode: u32,
) -> Result<(), String> {
    file.set_len(0)
        .and_then(|_| file.set_permissions(fs::Permissions::from_mode(file_mode)))
        .and_then(|_| file.sync_all())
        .and_then(|_| directory.sync_all())
        .map_err(|_| "private-directory-publication-invalidation-failed".to_string())
}

/// Publish one owner-private create-new record relative to an existing exact private parent.
///
/// The final parent must already exist and be exact mode 0700. DiskSage deliberately does not create
/// missing ancestors here: POSIX `mkdirat()` returns only status, not an opened handle for the newly
/// created directory, so a same-UID pathname replacement can occur before a later `openat()` binds
/// that name. Until a platform primitive can return or otherwise atomically bind the created object,
/// missing-parent provisioning fails before mutation. The existing parent is opened with
/// `O_NOFOLLOW`, bound by device/inode, and revalidated before and after record publication.
#[cfg(unix)]
pub(crate) fn write_private_bytes_create_new_with_parents(
    path: &Path,
    encoded: &[u8],
    file_mode: u32,
    directory_mode: u32,
) -> Result<(), String> {
    write_private_bytes_create_new_with_parents_with_hooks(
        path,
        encoded,
        file_mode,
        directory_mode,
        || {},
        || {},
    )
}

/// Deterministic test seam around the same existing-parent authority used in production.
///
/// `after_parent_provision` is retained as the historical hook name for dependent tests; no parent
/// provisioning occurs. The hook runs after the existing parent has been admitted and opened.
#[cfg(unix)]
pub(crate) fn write_private_bytes_create_new_with_parents_with_hooks<F, G>(
    path: &Path,
    encoded: &[u8],
    file_mode: u32,
    directory_mode: u32,
    after_parent_provision: F,
    before_finalize: G,
) -> Result<(), String>
where
    F: FnOnce(),
    G: FnOnce(),
{
    validate_modes(file_mode, directory_mode)?;
    if !path.is_absolute() {
        return Err("private-directory-publication-path-invalid".into());
    }
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .ok_or_else(|| "private-directory-publication-parent-missing".to_string())?;
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "private-directory-publication-file-name-invalid".to_string())?;
    let file_name_c = CString::new(file_name.as_bytes())
        .map_err(|_| "private-directory-publication-file-name-invalid".to_string())?;

    let (anchor, anchor_metadata) = discover_existing_parent(parent)?;
    let anchor_dev = anchor_metadata.dev();
    let anchor_ino = anchor_metadata.ino();
    let final_parent = open_anchor(&anchor, &anchor_metadata)?;

    revalidate_anchor(&anchor, &final_parent, anchor_dev, anchor_ino)?;
    private_directory(
        &final_parent
            .metadata()
            .map_err(|_| "private-directory-publication-parent-missing".to_string())?,
        Some(directory_mode),
    )?;

    after_parent_provision();
    revalidate_anchor(&anchor, &final_parent, anchor_dev, anchor_ino)?;
    private_directory(
        &final_parent
            .metadata()
            .map_err(|_| "private-directory-publication-parent-missing".to_string())?,
        Some(directory_mode),
    )?;

    let file_fd = unsafe {
        libc::openat(
            final_parent.as_raw_fd(),
            file_name_c.as_ptr(),
            libc::O_WRONLY
                | libc::O_CREAT
                | libc::O_EXCL
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW,
            file_mode as libc::mode_t,
        )
    };
    if file_fd < 0 {
        return Err("private-directory-publication-file-create-failed".into());
    }
    let mut file = unsafe { fs::File::from_raw_fd(file_fd) };

    let publication = (|| -> Result<(), String> {
        file.set_permissions(fs::Permissions::from_mode(file_mode))
            .map_err(|_| "private-directory-publication-file-mode-failed".to_string())?;
        file.write_all(encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "private-directory-publication-file-write-failed".to_string())?;
        let opened_metadata = file
            .metadata()
            .map_err(|_| "private-directory-publication-file-metadata-failed".to_string())?;
        if !opened_metadata.is_file()
            || opened_metadata.file_type().is_symlink()
            || opened_metadata.permissions().mode() & 0o7777 != file_mode
        {
            return Err("private-directory-publication-file-mode-drift".into());
        }
        final_parent
            .sync_all()
            .map_err(|_| "private-directory-publication-directory-sync-failed".to_string())?;

        before_finalize();
        revalidate_anchor(&anchor, &final_parent, anchor_dev, anchor_ino)?;
        private_directory(
            &final_parent
                .metadata()
                .map_err(|_| "private-directory-publication-parent-missing".to_string())?,
            Some(directory_mode),
        )?;

        let visible_fd = unsafe {
            libc::openat(
                final_parent.as_raw_fd(),
                file_name_c.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if visible_fd < 0 {
            return Err("private-directory-publication-file-identity-drift".into());
        }
        let visible = unsafe { fs::File::from_raw_fd(visible_fd) };
        let visible_metadata = visible
            .metadata()
            .map_err(|_| "private-directory-publication-file-identity-drift".to_string())?;
        if !visible_metadata.is_file()
            || visible_metadata.file_type().is_symlink()
            || visible_metadata.dev() != opened_metadata.dev()
            || visible_metadata.ino() != opened_metadata.ino()
        {
            return Err("private-directory-publication-file-identity-drift".into());
        }
        if visible_metadata.permissions().mode() & 0o7777 != file_mode {
            return Err("private-directory-publication-file-mode-drift".into());
        }
        Ok(())
    })();

    if let Err(error) = publication {
        invalidate_exact_record(&file, &final_parent, file_mode)?;
        return Err(error);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn write_private_bytes_create_new_with_parents(
    _path: &Path,
    _encoded: &[u8],
    _file_mode: u32,
    _directory_mode: u32,
) -> Result<(), String> {
    Err("private-directory-publication-unsupported".into())
}
