use std::path::PathBuf;

fn main() {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("다음 단계: 내보낸 로컬 PNG 파일 경로를 하나 이상 지정하세요. Photos 보관함과 클라우드 전용 파일은 열지 않습니다.");
        std::process::exit(2);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default();
    let audit = disksage_lib::photo_duplicate::audit_photos(&paths, now);
    println!("{}", serde_json::to_string_pretty(&audit).unwrap());
}
