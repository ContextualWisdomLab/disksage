use std::{env, fs, path::PathBuf};

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

fn main() {
    generate_cloud_plan_implementation();
    tauri_build::build()
}
