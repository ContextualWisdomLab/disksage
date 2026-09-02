#![cfg(target_os = "macos")]

use disksage_lib::rules::{cache_candidates, BaseDirs};
use std::fs;

struct FileDescriptorLimitGuard {
    original: libc::rlimit,
}

impl Drop for FileDescriptorLimitGuard {
    fn drop(&mut self) {
        let result = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &self.original) };
        assert_eq!(result, 0, "must restore process file-descriptor limit");
    }
}

fn constrain_file_descriptors() -> FileDescriptorLimitGuard {
    let mut original = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut original) },
        0,
        "must read process file-descriptor limit"
    );

    let baseline = fs::read_dir("/dev/fd")
        .expect("macOS must expose /dev/fd")
        .count() as libc::rlim_t;
    let constrained_soft = baseline
        .checked_add(32)
        .expect("file-descriptor limit arithmetic must remain bounded");
    assert!(
        original.rlim_max > constrained_soft,
        "test host must provide enough descriptor headroom"
    );

    let constrained = libc::rlimit {
        rlim_cur: constrained_soft,
        rlim_max: original.rlim_max,
    };
    assert_eq!(
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &constrained) },
        0,
        "must lower the soft file-descriptor limit"
    );
    FileDescriptorLimitGuard { original }
}

#[test]
fn parallel_cache_measurement_does_not_open_every_direct_child_before_workers_run() {
    let tmp = tempfile::tempdir().unwrap();
    let bases = BaseDirs {
        temp: tmp.path().join("tmp"),
        local_data: tmp.path().join("local"),
        home: tmp.path().join("home"),
    };
    let npm_cache = bases.home.join(".npm");

    for index in 0..128usize {
        let child = npm_cache.join(format!("child-{index:03}"));
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("payload.bin"), [index as u8]).unwrap();
    }

    let _limit_guard = constrain_file_descriptors();
    let npm = cache_candidates(&bases)
        .into_iter()
        .find(|candidate| candidate.id == "npm-cache")
        .expect("npm cache must remain in the catalog");

    assert_eq!(npm.bytes, 128, "all direct children must be measured even when the process cannot hold one descriptor per child");
}
