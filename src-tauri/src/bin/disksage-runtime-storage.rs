use disksage_lib::runtime_storage::{self, RuntimeStorageKind};

const USAGE: &str = "Usage: disksage-runtime-storage --runtime <podman-machine|colima> [--pretty] [--execute --confirm EXACT_PHRASE --rationale TEXT]\nWithout --execute, prints the current guest-trim plan.";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    runtime: RuntimeStorageKind,
    pretty: bool,
    confirmation: Option<String>,
    rationale: Option<String>,
}

fn parse_args(values: &[String]) -> Result<Args, String> {
    if values == ["--help"] || values == ["-h"] {
        return Err(USAGE.into());
    }
    let mut runtime = None;
    let mut pretty = false;
    let mut execute = false;
    let mut confirmation = None;
    let mut rationale = None;
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--runtime" => {
                index += 1;
                if runtime.is_some() {
                    return Err("--runtime may be supplied once".into());
                }
                runtime = Some(match values.get(index).map(String::as_str) {
                    Some("podman-machine") => RuntimeStorageKind::PodmanMachine,
                    Some("colima") => RuntimeStorageKind::Colima,
                    _ => return Err("--runtime requires podman-machine or colima".into()),
                });
            }
            "--pretty" if !pretty => pretty = true,
            "--execute" if !execute => execute = true,
            "--confirm" if confirmation.is_none() => {
                index += 1;
                confirmation = Some(
                    values
                        .get(index)
                        .ok_or("--confirm requires a phrase")?
                        .clone(),
                );
            }
            "--rationale" if rationale.is_none() => {
                index += 1;
                rationale = Some(
                    values
                        .get(index)
                        .ok_or("--rationale requires text")?
                        .clone(),
                );
            }
            _ => return Err("unknown or duplicate option".into()),
        }
        index += 1;
    }
    let runtime = runtime.ok_or("--runtime is required")?;
    if execute != confirmation.is_some() || execute != rationale.is_some() {
        return Err("--execute, --confirm, and --rationale must be supplied together".into());
    }
    Ok(Args {
        runtime,
        pretty,
        confirmation,
        rationale,
    })
}

fn run(values: &[String]) -> Result<String, String> {
    let args = parse_args(values)?;
    let value = match (args.confirmation.as_deref(), args.rationale.as_deref()) {
        (Some(confirmation), Some(rationale)) => serde_json::to_value(
            runtime_storage::execute_trim(args.runtime, confirmation, rationale)?,
        ),
        _ => {
            let plan = runtime_storage::inspect()
                .into_iter()
                .find(|plan| plan.runtime == args.runtime)
                .ok_or("runtime-storage-plan-unavailable")?;
            serde_json::to_value(plan)
        }
    }
    .map_err(|error| error.to_string())?;
    if args.pretty {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    }
    .map_err(|error| error.to_string())
}

fn main() {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(output) => println!("{output}"),
        Err(error) if error == USAGE => println!("{USAGE}"),
        Err(error) => {
            eprintln!("disksage-runtime-storage: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_authority_is_all_or_nothing() {
        assert_eq!(
            parse_args(&[
                "--runtime".into(),
                "podman-machine".into(),
                "--execute".into(),
            ])
            .unwrap_err(),
            "--execute, --confirm, and --rationale must be supplied together"
        );
    }

    #[test]
    fn read_only_selection_is_exact() {
        let args = parse_args(&["--runtime".into(), "colima".into(), "--pretty".into()])
            .expect("valid read-only selection");
        assert_eq!(args.runtime, RuntimeStorageKind::Colima);
        assert!(args.pretty);
        assert!(args.confirmation.is_none());
    }
}
