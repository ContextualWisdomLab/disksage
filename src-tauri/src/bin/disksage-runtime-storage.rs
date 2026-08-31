use disksage_lib::runtime_storage::{self, RuntimeStorageKind};

const USAGE: &str = "Usage: disksage-runtime-storage --runtime <colima|podman-machine> [--execute --confirm EXACT_PHRASE --rationale TEXT]";

fn runtime(value: &str) -> Result<RuntimeStorageKind, String> {
    match value {
        "colima" => Ok(RuntimeStorageKind::Colima),
        "podman-machine" => Ok(RuntimeStorageKind::PodmanMachine),
        _ => Err(format!("unsupported runtime\n{USAGE}")),
    }
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.as_slice() == ["--help"] || raw.as_slice() == ["-h"] {
        println!("{USAGE}");
        return Ok(());
    }
    let mut selected = None;
    let mut execute = false;
    let mut confirm = None;
    let mut rationale = None;
    let mut index = 0;
    while index < raw.len() {
        let value = |index: &mut usize, flag: &str| -> Result<String, String> {
            *index += 1;
            raw.get(*index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value\n{USAGE}"))
        };
        match raw[index].as_str() {
            "--runtime" if selected.is_none() => {
                selected = Some(runtime(&value(&mut index, "--runtime")?)?)
            }
            "--execute" if !execute => execute = true,
            "--confirm" if confirm.is_none() => confirm = Some(value(&mut index, "--confirm")?),
            "--rationale" if rationale.is_none() => {
                rationale = Some(value(&mut index, "--rationale")?)
            }
            _ => return Err(format!("invalid argument\n{USAGE}")),
        }
        index += 1;
    }
    let selected = selected.ok_or_else(|| format!("--runtime is required\n{USAGE}"))?;
    if execute {
        let output = runtime_storage::execute_trim(
            selected,
            confirm
                .as_deref()
                .ok_or_else(|| format!("--execute requires --confirm\n{USAGE}"))?,
            rationale
                .as_deref()
                .ok_or_else(|| format!("--execute requires --rationale\n{USAGE}"))?,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        );
    } else {
        if confirm.is_some() || rationale.is_some() {
            return Err(format!(
                "--confirm and --rationale require --execute\n{USAGE}"
            ));
        }
        let plan = runtime_storage::inspect()
            .into_iter()
            .find(|plan| plan.runtime == selected)
            .ok_or_else(|| "runtime-storage-plan-unavailable".to_string())?;
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())?
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_names_are_exact() {
        assert_eq!(runtime("colima").unwrap(), RuntimeStorageKind::Colima);
        assert_eq!(
            runtime("podman-machine").unwrap(),
            RuntimeStorageKind::PodmanMachine
        );
        assert!(runtime("docker").is_err());
    }
}
