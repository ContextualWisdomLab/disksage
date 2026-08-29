use crate::commands::CleanResult;

const IDENTITY_BOUND_RECYCLE_UNAVAILABLE: &str =
    "generic-cleanup-identity-bound-recycle-unavailable";

fn clean_paths_inner(paths: &[String]) -> Vec<CleanResult> {
    paths
        .iter()
        .map(|path| CleanResult {
            path: path.clone(),
            ok: false,
            error: IDENTITY_BOUND_RECYCLE_UNAVAILABLE.into(),
        })
        .collect()
}

/// Fail generic cleanup closed until the final reversible recycle primitive can remain bound to
/// the exact filesystem object that was authorized. Path revalidation cannot close a same-user
/// rename/symlink replacement window when the operating-system trash API consumes a pathname.
///
/// The Rust function name is intentionally distinct from the legacy command wrapper. Tauri 2.11+
/// maps this handler back to the stable external `clean_paths` IPC name without generating the
/// duplicate command macro symbol that the former same-named Rust function produced. The handler
/// remains compiled in coverage builds so instrumentation measures the same fail-closed command
/// surface that ships to customers.
#[tauri::command(rename = "clean_paths")]
pub fn fail_closed_clean_paths(paths: Vec<String>) -> Result<Vec<CleanResult>, String> {
    Ok(clean_paths_inner(&paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn refusal_is_stable_for_every_requested_path() {
        let paths = vec!["first".to_string(), "second".to_string()];

        let results = clean_paths_inner(&paths);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].path, "first");
        assert!(!results[0].ok);
        assert_eq!(results[0].error, IDENTITY_BOUND_RECYCLE_UNAVAILABLE);
        assert_eq!(results[1].path, "second");
        assert!(!results[1].ok);
        assert_eq!(results[1].error, IDENTITY_BOUND_RECYCLE_UNAVAILABLE);
    }

    #[test]
    fn public_handler_returns_the_same_fail_closed_results() {
        let results = fail_closed_clean_paths(vec!["customer-file".to_string()]).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "customer-file");
        assert!(!results[0].ok);
        assert_eq!(results[0].error, IDENTITY_BOUND_RECYCLE_UNAVAILABLE);
    }

    #[test]
    fn refusal_does_not_mutate_the_named_filesystem_object() {
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("keep.bin");
        fs::write(&victim, b"keep").unwrap();
        let paths = vec![victim.to_string_lossy().into_owned()];

        let results = clean_paths_inner(&paths);

        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert_eq!(results[0].error, IDENTITY_BOUND_RECYCLE_UNAVAILABLE);
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
    }

    #[test]
    fn empty_request_is_a_noop() {
        assert!(clean_paths_inner(&[]).is_empty());
    }
}
