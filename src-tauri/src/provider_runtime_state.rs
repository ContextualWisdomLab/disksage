//! Preserves provider-client runtime state around temporary maintenance operations.

/// Restore a provider client only when DiskSage observed it running before the temporary stop.
///
/// The caller owns the actual restart operation. A client that was already stopped must remain
/// stopped; DiskSage must not create new background activity merely because maintenance completed.
pub fn restore_after_temporary_stop<F>(
    was_running: bool,
    restart: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    if was_running {
        restart()
    } else {
        Ok(())
    }
}
