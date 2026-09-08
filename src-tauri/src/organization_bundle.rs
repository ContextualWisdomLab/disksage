//! Content-bound movement of an existing small document bundle, without classifying its topic.
use std::{fs, io::Read, path::Path, time::UNIX_EPOCH};

use super::{organization_boundary, MovePlan};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub root_object_id: String,
    pub files: Vec<BundleFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleFile {
    pub name: String,
    pub object_id: String,
    pub bytes: u64,
    pub modified_ns: String,
    pub content_blake3: String,
}

// ponytail: flat, 32-file/8-MiB bundles; larger or recursive groups need separate I/O-budget validation.
const MAX_FILES: usize = 32;
const MAX_BYTES: u64 = 8 * 1024 * 1024;

fn unavailable(_: impl std::fmt::Display) -> String {
    "폴더 구성이나 파일 상태를 확인하지 못해 현재 위치에 보존합니다.".into()
}

pub fn observe(source: &Path) -> Result<BundleManifest, String> {
    let root = fs::symlink_metadata(source).map_err(unavailable)?;
    if !source.is_absolute()
        || !root.is_dir()
        || root.file_type().is_symlink()
        || crate::safety::agent_state_guard::is_agent_state(source)
        || organization_boundary::package_ancestor(source)
        || organization_boundary::package_ancestor(&fs::canonicalize(source).map_err(unavailable)?)
    {
        return Err("보호 대상이나 일반 폴더가 아닌 항목은 묶음으로 옮기지 않습니다.".into());
    }
    organization_boundary::validate_project_ancestors(source)?;
    let root_object_id = crate::safety::filesystem_object_id(source).map_err(unavailable)?;
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(source).map_err(unavailable)? {
        let entry = entry.map_err(unavailable)?;
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| unavailable("name"))?;
        if files.len() >= MAX_FILES
            || [
                "Cargo.toml",
                "package.json",
                "pyproject.toml",
                "go.mod",
                "CMakeLists.txt",
                "wscript",
                "SConstruct",
                "configure.ac",
            ]
            .contains(&name.as_str())
        {
            return Err("지원 범위를 넘거나 프로젝트 경계가 있는 폴더는 그대로 보존합니다.".into());
        }
        let before = fs::symlink_metadata(&path).map_err(unavailable)?;
        if !before.is_file()
            || before.file_type().is_symlink()
            || crate::cloud::metadata_is_dataless(&before)
            || crate::safety::agent_state_guard::is_agent_state(&path)
        {
            return Err(
                "하위 폴더·연결 파일·클라우드 전용 파일이 있어 묶음 이동을 보류합니다.".into(),
            );
        }
        let object_id = crate::safety::filesystem_object_id(&path).map_err(unavailable)?;
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
        }
        let file = options.open(&path).map_err(unavailable)?;
        let opened = file.metadata().map_err(unavailable)?;
        if !opened.is_file()
            || crate::cloud::metadata_is_dataless(&opened)
            || opened.len() > MAX_BYTES.saturating_sub(total)
        {
            return Err("로컬 내용 확인 범위를 넘어 묶음 이동을 보류합니다.".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if opened.file_attributes() & 0x400 != 0 {
                return Err(unavailable("reparse point"));
            }
        }
        #[cfg(unix)]
        if crate::safety::object_id_from_metadata(&opened).as_ref() != Some(&object_id) {
            return Err(unavailable("file replaced"));
        }
        let modified = opened.modified().map_err(unavailable)?;
        let mut reader = (&file).take(MAX_BYTES.saturating_sub(total) + 1);
        let mut buffer = [0u8; 64 * 1024];
        let mut hasher = blake3::Hasher::new();
        let mut bytes_read = 0u64;
        loop {
            let count = match reader.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result.map_err(unavailable)?,
            };
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            bytes_read += count as u64;
        }
        let after = file.metadata().map_err(unavailable)?;
        if bytes_read != opened.len()
            || after.len() != opened.len()
            || after.modified().map_err(unavailable)? != modified
            || crate::safety::filesystem_object_id(&path).map_err(unavailable)? != object_id
        {
            return Err("확인 중 파일이 바뀌어 묶음 이동을 보류합니다.".into());
        }
        total += bytes_read;
        files.push(BundleFile {
            name,
            object_id,
            bytes: opened.len(),
            modified_ns: modified
                .duration_since(UNIX_EPOCH)
                .map_err(unavailable)?
                .as_nanos()
                .to_string(),
            content_blake3: hasher.finalize().to_hex().to_string(),
        });
    }
    if files.is_empty()
        || fs::metadata(source)
            .map_err(unavailable)?
            .modified()
            .map_err(unavailable)?
            != root.modified().map_err(unavailable)?
        || crate::safety::filesystem_object_id(source).map_err(unavailable)? != root_object_id
    {
        return Err("비어 있거나 확인 중 구성이 바뀐 폴더는 보존합니다.".into());
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(BundleManifest {
        root_object_id,
        files,
    })
}

pub fn plan(source: &Path, target_parent: &Path) -> Result<MovePlan, String> {
    if !target_parent.is_absolute() || target_parent.starts_with(source) {
        return Err("원본 폴더 바깥의 절대 경로를 대상으로 지정하세요.".into());
    }
    let destination = target_parent.join(source.file_name().ok_or_else(|| unavailable("root"))?);
    match fs::symlink_metadata(&destination) {
        Ok(_) => return Err("대상 위치에 같은 이름이 있어 묶음 이동을 보류합니다.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(unavailable(error)),
    }
    let canonical_source = fs::canonicalize(source).map_err(unavailable)?;
    for ancestor in target_parent.ancestors() {
        if let Ok(resolved) = fs::canonicalize(ancestor) {
            if resolved.starts_with(&canonical_source) {
                return Err("원본 폴더 안으로는 묶음을 옮길 수 없습니다.".into());
            }
            break;
        }
    }
    organization_boundary::validate_destination(&destination)?;
    let bundle = observe(source)?;
    Ok(MovePlan {
        src: source.to_str().ok_or_else(|| unavailable("source"))?.into(),
        dst: destination
            .to_str()
            .ok_or_else(|| unavailable("destination"))?
            .into(),
        bundle: Some(bundle),
        ..Default::default()
    })
}
