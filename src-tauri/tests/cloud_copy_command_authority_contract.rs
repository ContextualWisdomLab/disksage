const COMMANDS: &str = include_str!("../src/commands.rs");
const CLOUD_TRANSFER: &str = include_str!("../src/cloud_transfer.rs");

fn function_body(start: &str, end: &str) -> &'static str {
    let start_index = COMMANDS
        .find(start)
        .unwrap_or_else(|| panic!("missing command boundary: {start}"));
    let tail = &COMMANDS[start_index..];
    let end_index = tail
        .find(end)
        .unwrap_or_else(|| panic!("missing command boundary: {end}"));
    &tail[..end_index]
}

fn cloud_transfer_function_body(start: &str, end: &str) -> &'static str {
    let start_index = CLOUD_TRANSFER
        .find(start)
        .unwrap_or_else(|| panic!("missing cloud-transfer boundary: {start}"));
    let tail = &CLOUD_TRANSFER[start_index..];
    let end_index = tail
        .find(end)
        .unwrap_or_else(|| panic!("missing cloud-transfer boundary: {end}"));
    &tail[..end_index]
}

#[test]
fn failed_copy_evidence_never_shares_success_receipt_authority() {
    let body = function_body(
        "fn create_cloud_candidate_receipt(",
        "pub async fn copy_cloud_candidate(",
    );

    assert!(
        body.contains("cloud-copy-failures"),
        "copy failures need a dedicated private journal directory rather than cloud-receipts"
    );

    let failure_call = body
        .find("cloud_transfer::record_copy_failure(")
        .map(|index| &body[index..])
        .expect("copy failure recording call");
    let failure_call = failure_call
        .split_once(");")
        .map(|(call, _)| call)
        .expect("bounded copy failure call");

    assert!(
        !failure_call.contains("&receipt_dir"),
        "failure journals must not enter success-receipt reconciliation"
    );
}

#[test]
fn native_copy_cancellation_is_checked_during_preflight() {
    let body = function_body(
        "fn create_cloud_candidate_receipt(",
        "pub async fn copy_cloud_candidate(",
    );
    let checks = body
        .matches("require_native_copy_not_cancelled_with_failure(")
        .count();
    assert!(
        checks >= 5,
        "native copy preflight must honor cancellation at each bounded gate"
    );
    assert!(
        body.find("require_native_copy_not_cancelled_with_failure(")
            < body.find("require_local_copy_headroom(candidate)?"),
        "queued cancellation must be checked before headroom/provider preflight"
    );
    assert!(
        body.rfind("require_native_copy_not_cancelled_with_failure(")
            < body.find("prepare_cloud_copy_with_approval_cancelable("),
        "native copy must re-check cancellation immediately before mutation"
    );
}

fn assert_cancel_registration_precedes_serialization_lock(command_start: &str, command_end: &str) {
    let body = function_body(command_start, command_end);
    let blocking = body
        .find("spawn_blocking(move ||")
        .map(|index| &body[index..])
        .expect("spawn_blocking command body");
    let operation_lock = blocking
        .find("cloud_copy_operation\n                .lock()")
        .expect("copy operation registration lock");
    let lock_index = blocking
        .find("cloud_review\n                .lock()")
        .expect("cloud review serialization lock");
    let reset_index = blocking
        .find("cloud_copy_cancel.store(false, Ordering::SeqCst)")
        .expect("cancel-token reset");

    assert!(
        operation_lock < reset_index && reset_index < lock_index,
        "queued commands must register and reset cancellation before waiting for the serialization lock"
    );
    assert!(
        blocking.contains("if active.is_some()") && blocking.contains("cloud-copy-already-active"),
        "a second native copy must not overwrite the queued operation's cancellation token"
    );
}

#[test]
fn native_copy_cancel_registration_happens_before_serialization_lock() {
    assert_cancel_registration_precedes_serialization_lock(
        "pub async fn copy_cloud_candidate(",
        "pub async fn copy_cloud_candidate_via_provider_api(",
    );
}

#[test]
fn native_copy_cleanup_is_panic_safe() {
    let body = function_body(
        "pub async fn copy_cloud_candidate(",
        "pub async fn copy_cloud_candidate_via_provider_api(",
    );
    assert!(
        body.contains("struct NativeCopyReset") && body.contains("impl Drop for NativeCopyReset"),
        "native copy state must be released by an RAII guard when the blocking task panics"
    );
    assert!(
        body.contains("if active.as_deref() == Some(self.fingerprint.as_str())"),
        "panic-safe cleanup must not clear a newer operation"
    );
}

#[test]
fn adoption_is_not_registered_as_a_cancellable_native_copy() {
    let body = function_body(
        "pub async fn adopt_existing_cloud_candidate(",
        "pub struct CloudAttestationOutput",
    );

    assert!(
        !body.contains("cloud_copy_operation"),
        "adoption does not poll the native-copy cancel token and must not be advertised as an active cancellable copy"
    );
    assert!(
        !body.contains("cloud_copy_cancel.store(false, Ordering::SeqCst)"),
        "non-cancellable adoption must not reset or own the native-copy cancellation lifecycle"
    );
}

#[test]
fn failed_native_copy_cleanup_is_not_compiled_out_on_windows() {
    let body = cloud_transfer_function_body("fn copy_and_verify(", "fn verify_existing_destination(");

    let has_windows_cleanup = body.contains("#[cfg(windows)]")
        || body.contains("#[cfg(all(not(coverage), windows))]")
        || body.contains("#[cfg(all(not(coverage), not(target_os = \"macos\")))]");
    assert!(
        has_windows_cleanup,
        "a failed create_new copy must retain a Windows cleanup path instead of leaving a partial destination that blocks retries"
    );

    assert!(
        body.contains("if copy_result.is_err()"),
        "failed native copies must keep an explicit cleanup boundary"
    );
}

#[test]
fn windows_cleanup_uses_stable_handle_identity() {
    assert!(
        CLOUD_TRANSFER.contains("Handle::from_path(path)")
            && CLOUD_TRANSFER.contains("Handle::from_file"),
        "Windows cleanup must capture and compare stable same-file handles"
    );
    assert!(
        !CLOUD_TRANSFER.contains("use std::os::windows::fs::MetadataExt"),
        "unstable Windows metadata identity imports must not reach release builds"
    );
}
