use disksage_lib::provider_runtime_state::restore_after_temporary_stop;
use std::cell::Cell;

#[test]
fn onedrive_unpin_preserves_an_initially_stopped_client() {
    let restart_called = Cell::new(false);

    restore_after_temporary_stop(false, || {
        restart_called.set(true);
        Ok::<(), String>(())
    })
    .expect("an initially stopped provider needs no restart");

    assert!(!restart_called.get());
}

#[test]
fn onedrive_unpin_restores_an_initially_running_client() {
    let restart_called = Cell::new(false);

    restore_after_temporary_stop(true, || {
        restart_called.set(true);
        Ok::<(), String>(())
    })
    .expect("a previously running provider is restored");

    assert!(restart_called.get());
}
