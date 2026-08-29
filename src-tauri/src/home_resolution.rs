use std::path::PathBuf;

/// Select the first absolute home-directory candidate and fail closed when none are usable.
///
/// Callers may supply platform API results and native environment fallbacks in precedence order.
/// Relative values such as `.` are never accepted as path authority because they would make
/// `~/...` destinations depend on the process working directory.
pub(crate) fn select_absolute_home(
    candidates: impl IntoIterator<Item = Option<PathBuf>>,
) -> Result<PathBuf, String> {
    candidates
        .into_iter()
        .flatten()
        .find(|candidate| candidate.is_absolute())
        .ok_or_else(|| "home-directory-unavailable".to_string())
}

/// Build the Windows HOMEDRIVE + HOMEPATH fallback without lossy UTF-8 conversion.
#[cfg(windows)]
pub(crate) fn windows_home_drive_path() -> Option<PathBuf> {
    let drive = std::env::var_os("HOMEDRIVE")?;
    let path = std::env::var_os("HOMEPATH")?;
    let mut combined = PathBuf::from(drive);
    combined.push(path);
    combined.is_absolute().then_some(combined)
}
