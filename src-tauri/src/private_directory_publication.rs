use std::path::Path;

#[cfg(unix)]
use std::ffi::{CString, OsString};
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
fn discover_anchor(parent: &Path) -> Result<(PathBuf, Vec<OsString>, fs::Metadata), String> {
    let mut cursor = parent.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) => {
                private_directory(&metadata, None)?;
                missing.reverse();
                return Ok((cursor, missing, metadata));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let name = cursor
                    .file_name()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| "private-directory-publication-anchor-missing".to_string())?;
                missing.push(name.to_os_string());
                cursor = cursor
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .ok_or_else(|| "private-directory-publication-anchor-missing".to_string())?
                    .to_path_buf();
            }
            Err(_) => return Err("private-directory-publication-anchor-unavailable".into()),
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
fn open_child_directory(parent: &fs::File, name: &CString) -> Result<fs::File, String> {
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err("private-directory-publication-directory-identity-drift".into());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn revalidate_chain(
    anchor: &Path,
    directories: &[fs::File],
    names: &[CString],
    anchor_dev: u64,
    anchor_ino: u64,
    directory_mode: u32,
) -> Result<(), String> {
    let root = directories
        .first()
        .ok_or_else(|| "private-directory-publication-anchor-unavailable".to_string())?;
    revalidate_anchor(anchor, root, anchor_dev, anchor_ino)?;
    for (index, name) in names.iter().enumerate() {
        let visible = open_child_directory(&directories[index], name)?;
        let visible_metadata = visible
            .metadata()
            .map_err(|_| "private-directory-publication-directory-identity-drift".to_string())?;
        let admitted_metadata = directories[index + 1]
            .metadata()
            .map_err(|_| "private-directory-publication-directory-identity-drift".to_string())?;
        private_directory(&visible_metadata, Some(directory_mode))?;
        if visible_metadata.dev() != admitted_metadata.dev()
            || visible_metadata.ino() != admitted_metadata.ino()
        {
            return Err("private-directory-publication-directory-identity-drift".into());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn invalidate_exact_record(file: &fs::File, directory: &fs::File) -> Result<(), String> {
    file.set_len(0)
        .and_then(|_| file.sync_all())
        .and_then(|_| directory.sync_all())
        .map_err(|_| "private-directory-publication-invalidation-failed".to_string())
}

/// Publish one owner-private create-new record while provisioning missing private ancestors through
/// pinned directory descriptors. Existing ancestors are admission-only and are never chmodded.
/// Missing descendants are created at mode 0700 with `mkdirat`, opened with `O_NOFOLLOW`, and fsynced
/// before the final record is created relative to the pinned leaf directory. Namespace drift fails
/// closed; after record creation, failure invalidates only the exact open record.
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

    let (anchor, missing, anchor_metadata) = discover_anchor(parent)?;
    let anchor_dev = anchor_metadata.dev();
    let anchor_ino = anchor_metadata.ino();
    let anchor_file = open_anchor(&anchor, &anchor_metadata)?;
    let mut directories = vec![anchor_file];
    let mut names = Vec::with_capacity(missing.len());

    revalidate_chain(
        &anchor,
        &directories,
        &names,
        anchor_dev,
        anchor_ino,
        directory_mode,
    )?;

    for component in missing {
        let name = CString::new(component.as_bytes())
            .map_err(|_| "private-directory-publication-directory-name-invalid".to_string())?;
        let current = directories
            .last()
            .ok_or_else(|| "private-directory-publication-anchor-unavailable".to_string())?;
        let created = unsafe {
            libc::mkdirat(
                current.as_raw_fd(),
                name.as_ptr(),
                directory_mode as libc::mode_t,
            )
        };
        if created != 0 {
            return Err("private-directory-publication-directory-create-failed".into());
        }
        let child = open_child_directory(current, &name)
            .map_err(|_| "private-directory-publication-directory-open-failed".to_string())?;
        child
            .set_permissions(fs::Permissions::from_mode(directory_mode))
            .map_err(|_| "private-directory-publication-directory-mode-failed".to_string())?;
        child
            .sync_all()
            .map_err(|_| "private-directory-publication-directory-sync-failed".to_string())?;
        current
            .sync_all()
            .map_err(|_| "private-directory-publication-directory-sync-failed".to_string())?;
        let metadata = child
            .metadata()
            .map_err(|_| "private-directory-publication-directory-open-failed".to_string())?;
        private_directory(&metadata, Some(directory_mode))?;
        names.push(name);
        directories.push(child);
        revalidate_chain(
            &anchor,
            &directories,
            &names,
            anchor_dev,
            anchor_ino,
            directory_mode,
        )?;
    }

    after_parent_provision();
    revalidate_chain(
        &anchor,
        &directories,
        &names,
        anchor_dev,
        anchor_ino,
        directory_mode,
    )?;

    let final_parent = directories
        .last()
        .ok_or_else(|| "private-directory-publication-parent-missing".to_string())?;
    let final_parent_metadata = final_parent
        .metadata()
        .map_err(|_| "private-directory-publication-parent-missing".to_string())?;
    private_directory(&final_parent_metadata, Some(directory_mode))?;

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
        revalidate_chain(
            &anchor,
            &directories,
            &names,
            anchor_dev,
            anchor_ino,
            directory_mode,
        )?;
        let final_parent_metadata = final_parent
            .metadata()
            .map_err(|_| "private-directory-publication-parent-missing".to_string())?;
        private_directory(&final_parent_metadata, Some(directory_mode))?;

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
        invalidate_exact_record(&file, final_parent)?;
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
