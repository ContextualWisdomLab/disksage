use std::{env, fs, path::PathBuf, process::Command};

const CLOUD_PLAN_IMPLEMENTATION: &str = "cloud_plan_implementation.rs.inc";
const EMBED_PLIST_CALL: &str =
    "embed_plist::embed_info_plist!(\"../../disksage-cloud-plan.Info.plist\");";
const GENERATED_EMBED_PLIST_CALL: &str =
    "embed_plist::embed_info_plist!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/disksage-cloud-plan.Info.plist\"));";

fn generate_cloud_plan_implementation() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR"),
    );
    let source_path = manifest_dir.join(CLOUD_PLAN_IMPLEMENTATION);
    println!("cargo:rerun-if-changed={}", source_path.display());
    let source = fs::read_to_string(&source_path)
        .expect("cloud-plan implementation source must be readable by build.rs");
    assert!(
        source.starts_with("//!"),
        "cloud-plan implementation must keep its source-level module documentation marker"
    );
    assert_eq!(
        source.matches(EMBED_PLIST_CALL).count(),
        1,
        "cloud-plan implementation must contain exactly one Info.plist embedding call"
    );
    let generated =
        source
            .replacen("//!", "//", 1)
            .replacen(EMBED_PLIST_CALL, GENERATED_EMBED_PLIST_CALL, 1);
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR to build.rs"));
    fs::write(
        out_dir.join("disksage-cloud-plan-implementation.rs"),
        generated,
    )
    .expect("generated cloud-plan implementation must be writable in OUT_DIR");
}

fn compile_fileprovider_helper() {
    let target = env::var("TARGET").expect("Cargo must provide TARGET");
    if !target.ends_with("apple-darwin") {
        return;
    }
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest_dir.join("native/fileprovider-evict.swift");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("disksage-fileprovider-evict");
    let swift_target = match target.as_str() {
        "aarch64-apple-darwin" => "arm64-apple-macos11.0",
        "x86_64-apple-darwin" => "x86_64-apple-macos11.0",
        _ => panic!("unsupported macOS target for native File Provider helper: {target}"),
    };
    println!("cargo:rerun-if-changed={}", source.display());
    let status = Command::new("xcrun")
        .args(["swiftc", "-parse-as-library", "-O", "-target", swift_target])
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("xcrun swiftc must be available for the macOS build");
    assert!(
        status.success(),
        "native File Provider helper compilation failed"
    );
}

fn main() {
    generate_cloud_plan_implementation();
    compile_fileprovider_helper();
    tauri_build::build()
}
