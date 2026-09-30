//! Operator-facing initial scan-root ordering for the desktop UX.
//!
//! The provider/cloud command owner may evolve independently underneath this stacked UX lane.
//! Keeping the first-scan preference here prevents a Storybook/UX restack from rewriting provider
//! mutation authority while still avoiding an accidental whole-filesystem scan on desktop startup.

use std::path::Path;

/// Prefer an existing Downloads directory, then the home directory, and keep `/` as an explicit
/// fallback. Windows retains the existing drive-root discovery contract.
#[tauri::command]
pub fn list_roots() -> Vec<String> {
    #[cfg(windows)]
    {
        ('A'..='Z')
            .filter_map(|drive| {
                let root = format!("{drive}:\\");
                Path::new(&root).exists().then_some(root)
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let mut roots = Vec::new();
        if let Ok(home) = std::env::var("HOME") {
            let home_path = Path::new(&home);
            let downloads = home_path.join("Downloads");
            if downloads.is_dir() {
                roots.push(downloads.to_string_lossy().into_owned());
            }
            if home != "/" {
                roots.push(home);
            }
        }
        if !roots.iter().any(|root| root == "/") {
            roots.push("/".to_string());
        }
        roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_roots_remain_available_without_promoting_filesystem_root() {
        let roots = list_roots();
        assert!(!roots.is_empty());
        #[cfg(windows)]
        assert!(roots.iter().any(|root| root.ends_with(":\\")));
        #[cfg(not(windows))]
        {
            assert!(roots.contains(&"/".to_string()));
            if let Ok(home) = std::env::var("HOME") {
                let downloads = Path::new(&home).join("Downloads");
                if downloads.is_dir() {
                    assert_eq!(roots.first(), Some(&downloads.to_string_lossy().into_owned()));
                }
                if home != "/" {
                    let home_index = roots.iter().position(|root| root == &home).unwrap();
                    let filesystem_index = roots.iter().position(|root| root == "/").unwrap();
                    assert!(home_index < filesystem_index);
                }
            }
        }
    }
}
