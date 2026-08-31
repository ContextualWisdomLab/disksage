use disksage_lib::podman_reclaim::{
    plan_empty_dangling_volumes, prune_empty_dangling_volumes, DEFAULT_PODMAN_MACHINE,
};
use std::path::PathBuf;

fn main() {
    let mut execute = false;
    let mut phrase = None;
    let mut rationale = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--execute" => execute = true,
            "--confirmation-phrase" => phrase = args.next(),
            "--rationale" => rationale = args.next(),
            "--help" | "-h" => {
                println!("usage: disksage-podman-empty-volumes [--execute --confirmation-phrase TEXT --rationale TEXT]");
                return;
            }
            _ => {
                eprintln!("disksage-podman-empty-volumes: unknown argument");
                std::process::exit(2);
            }
        }
    }
    let podman = PathBuf::from("/opt/homebrew/bin/podman");
    let result = if execute {
        prune_empty_dangling_volumes(
            &podman,
            DEFAULT_PODMAN_MACHINE,
            phrase.as_deref().unwrap_or_default(),
            rationale.as_deref().unwrap_or_default(),
        )
        .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
    } else {
        plan_empty_dangling_volumes(&podman, DEFAULT_PODMAN_MACHINE)
            .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
    };
    match result {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => {
            eprintln!("disksage-podman-empty-volumes: {error}");
            std::process::exit(2);
        }
    }
}
