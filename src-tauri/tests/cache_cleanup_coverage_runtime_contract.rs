#[test]
fn shipped_cache_cleanup_handlers_remain_available_to_instrumented_builds() {
    let _list_cache_targets = disksage_lib::cache_cleanup::list_cache_targets;
    let _clean_cache_contents = disksage_lib::cache_cleanup::clean_cache_contents;
}
