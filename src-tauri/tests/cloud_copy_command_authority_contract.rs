const COMMANDS: &str = include_str!("../src/commands.rs");

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

fn assert_cancel_reset_is_serialized(command_start: &str, command_end: &str) {
    let body = function_body(command_start, command_end);
    let blocking = body
        .find("spawn_blocking(move ||")
        .map(|index| &body[index..])
        .expect("spawn_blocking command body");
    let lock_index = blocking
        .find("cloud_review\n            .lock()")
        .expect("cloud review serialization lock");
    let reset_index = blocking
        .find("cloud_copy_cancel.store(false, Ordering::SeqCst)")
        .expect("cancel-token reset");

    assert!(
        lock_index < reset_index,
        "queued commands must not clear an in-flight copy cancellation before acquiring the serialization lock"
    );
}

#[test]
fn native_copy_cancel_reset_happens_after_serialization_lock() {
    assert_cancel_reset_is_serialized(
        "pub async fn copy_cloud_candidate(",
        "pub async fn copy_cloud_candidate_via_provider_api(",
    );
}

#[test]
fn adoption_cancel_reset_happens_after_serialization_lock() {
    assert_cancel_reset_is_serialized(
        "pub async fn adopt_existing_cloud_candidate(",
        "pub struct CloudAttestationOutput",
    );
}
