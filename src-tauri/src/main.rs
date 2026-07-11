// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// coverage 빌드에서는 GUI 부트스트랩을 컴파일하지 않는다 (#[coverage(off)]는 아직 unstable)
#[cfg(not(coverage))]
fn main() {
    disksage_lib::run()
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(coverage, test))]
mod coverage_tests {
    // 커버리지 빌드의 no-op main도 라인으로 집계되므로 실행해 준다
    #[test]
    fn noop_main_runs() {
        super::main();
    }
}
