use disksage_lib::parallels_disk_reclaim::{
    enforce_cli_platform, plan, validate_cli_argument_tokens,
};
use std::path::PathBuf;

fn value(args: &[String], flag: &str) -> Result<String, String> {
    let positions: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, value)| value.as_str() == flag)
        .collect();
    if positions.len() != 1 {
        return Err(format!("{flag}를 한 번 지정하세요."));
    }
    args.get(positions[0].0 + 1)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| format!("{flag} 값을 지정하세요."))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| {
        enforce_cli_platform()?;
        validate_cli_argument_tokens(&args)?;
        let prlctl = PathBuf::from(value(&args, "--prlctl")?);
        let disk_tool = PathBuf::from(value(&args, "--disk-tool")?);
        let vm_id = value(&args, "--vm-id")?;
        let bundle = PathBuf::from(value(&args, "--bundle")?);
        let disk = PathBuf::from(value(&args, "--disk")?);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "시스템 시간을 확인하세요.".to_string())?
            .as_millis() as u64;
        plan(&prlctl, &disk_tool, &vm_id, &bundle, &disk, now)
    })();
    match result {
        Ok(plan) => println!("{}", serde_json::to_string_pretty(&plan).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
