//! Owner-controlled checkout lease for work that may go dormant between agent turns.

use disksage_lib::git_clone_reclaim::{acquire_git_checkout_lease, release_git_checkout_lease};
use disksage_lib::git_worktree::validate_reference;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: disksage-git-checkout-lease acquire --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] --owner OWNER [--expires-at-ms OWNER_TIMESTAMP] | disksage-git-checkout-lease release --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] --lease-fingerprint HEX64";

#[derive(Debug, PartialEq, Eq)]
enum Request {
    Acquire {
        repository_root: PathBuf,
        retention_references: Vec<String>,
        owner: String,
        expires_at_ms: Option<u64>,
    },
    Release {
        repository_root: PathBuf,
        retention_references: Vec<String>,
        lease_fingerprint: String,
    },
}

fn value(raw: &[OsString], index: &mut usize) -> Result<String, String> {
    *index += 1;
    raw.get(*index)
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "git-checkout-lease-option-value-missing-or-invalid".into())
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), String> {
    if slot.replace(value).is_some() {
        Err("git-checkout-lease-duplicate-option".into())
    } else {
        Ok(())
    }
}

fn parse_args(raw: &[OsString]) -> Result<Option<Request>, String> {
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("--help") | Some("-h")) {
        return Ok(None);
    }
    let action = raw
        .first()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "git-checkout-lease-action-missing".to_string())?;
    let mut root = None;
    let mut owner = None;
    let mut references = Vec::new();
    let mut expires = None;
    let mut fingerprint = None;
    let mut index = 1;
    while index < raw.len() {
        match raw[index].to_str() {
            Some("--repository-root") => {
                set_once(&mut root, PathBuf::from(value(raw, &mut index)?))?
            }
            Some("--owner") => set_once(&mut owner, value(raw, &mut index)?)?,
            Some("--reference-ref") => {
                let reference = value(raw, &mut index)?;
                validate_reference(&reference)?;
                references.push(reference);
            }
            Some("--expires-at-ms") => set_once(
                &mut expires,
                value(raw, &mut index)?
                    .parse()
                    .map_err(|_| "git-checkout-lease-expiry-invalid".to_string())?,
            )?,
            Some("--lease-fingerprint") => set_once(&mut fingerprint, value(raw, &mut index)?)?,
            _ => return Err("git-checkout-lease-unknown-option".into()),
        }
        index += 1;
    }
    let repository_root = root.ok_or_else(|| "git-checkout-lease-root-missing".to_string())?;
    if !repository_root.is_absolute() {
        return Err("git-checkout-lease-root-invalid".into());
    }
    if references.is_empty() {
        return Err("git-checkout-lease-reference-missing".into());
    }
    match action {
        "acquire" if fingerprint.is_none() => Ok(Some(Request::Acquire {
            repository_root,
            retention_references: references,
            owner: owner.ok_or_else(|| "git-checkout-lease-owner-missing".to_string())?,
            expires_at_ms: expires,
        })),
        "release" if owner.is_none() && expires.is_none() => Ok(Some(Request::Release {
            repository_root,
            retention_references: references,
            lease_fingerprint: fingerprint
                .ok_or_else(|| "git-checkout-lease-fingerprint-missing".to_string())?,
        })),
        "acquire" | "release" => Err("git-checkout-lease-action-options-invalid".into()),
        _ => Err("git-checkout-lease-action-invalid".into()),
    }
}

fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "git-checkout-lease-clock-unavailable".into())
}

fn run(request: Request) -> Result<serde_json::Value, String> {
    let observed_at_ms = now_ms()?;
    match request {
        Request::Acquire {
            repository_root,
            retention_references,
            owner,
            expires_at_ms,
        } => serde_json::to_value(acquire_git_checkout_lease(
            &repository_root,
            &retention_references,
            &owner,
            observed_at_ms,
            expires_at_ms,
        )?)
        .map_err(|error| error.to_string()),
        Request::Release {
            repository_root,
            retention_references,
            lease_fingerprint,
        } => {
            release_git_checkout_lease(
                &repository_root,
                &retention_references,
                &lease_fingerprint,
                observed_at_ms,
            )?;
            Ok(serde_json::json!({
                "released": true,
                "customer_next_action": "이 폴더를 다시 검사하세요."
            }))
        }
    }
}

fn main() {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    match parse_args(&raw).and_then(|request| request.map(run).transpose()) {
        Ok(None) => println!("{USAGE}"),
        Ok(Some(output)) => println!("{}", serde_json::to_string_pretty(&output).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_controls_expiry_and_release_authority() {
        let acquire = parse_args(&[
            "acquire".into(),
            "--repository-root".into(),
            "/tmp/clone".into(),
            "--reference-ref".into(),
            "refs/heads/main".into(),
            "--owner".into(),
            "agent/session-1".into(),
            "--expires-at-ms".into(),
            "99".into(),
        ])
        .unwrap()
        .unwrap();
        assert!(matches!(
            acquire,
            Request::Acquire {
                expires_at_ms: Some(99),
                ..
            }
        ));
        assert!(parse_args(&[
            "release".into(),
            "--repository-root".into(),
            "/tmp/clone".into(),
            "--reference-ref".into(),
            "refs/heads/main".into(),
            "--owner".into(),
            "someone-else".into(),
            "--lease-fingerprint".into(),
            "abc".into(),
        ])
        .is_err());
    }
}
