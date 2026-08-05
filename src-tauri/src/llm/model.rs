//! 모델 레지스트리 + SHA-256 검증(순수, 게이트 대상) + 다운로드(cfg(not(coverage)), 게이트 제외).
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub name: &'static str,
    pub url: &'static str,
    pub sha256_hex: &'static str,
    pub bytes: u64,
}

// 기본 모델: Qwen2.5-1.5B-Instruct GGUF Q4_K_M (Apache-2.0). 첫 사용 시 다운로드.
// SHA/size 출처: HuggingFace LFS 메타데이터 (raw LFS pointer의 `oid sha256:`/`size`,
// resolve URL의 `X-Linked-ETag`/`X-Linked-Size`와 교차 확인 일치),
// repo=Qwen/Qwen2.5-1.5B-Instruct-GGUF, file=qwen2.5-1.5b-instruct-q4_k_m.gguf, 확인일=2026-07-11.
pub const DEFAULT: ModelSpec = ModelSpec {
    name: "Qwen2.5-1.5B-Instruct-Q4_K_M",
    url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
    sha256_hex: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
    bytes: 1_117_320_736,
};

/// 다운로드한 바이트의 SHA-256가 기대값과 일치하는지(대소문자 무관, fail-closed).
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> bool {
    let mut h = Sha256::new();
    h.update(bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    got.eq_ignore_ascii_case(expected_hex)
}

/// 모델을 dest로 다운로드 → 메모리 버퍼에서 SHA 검증(쓰기 전) → 통과 시에만 .part로 쓰고 원자적 rename으로 배치.
/// 검증이 쓰기보다 먼저 일어나므로 불일치 시 부분 파일 자체가 생기지 않는다(Err만 반환).
/// cfg(not(coverage)): 네트워크/파일 io는 게이트에서 실행 불가라 제외(스펙 §9: 실 모델은 --ignored/수동).
#[cfg(not(coverage))]
pub fn download_to(spec: &ModelSpec, dest: &std::path::Path) -> Result<(), String> {
    use std::io::Read;
    let part = dest.with_extension("part");
    let resp = ureq::get(spec.url).call().map_err(|e| e.to_string())?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if !verify_sha256(&buf, spec.sha256_hex) {
        return Err(format!("SHA-256 불일치 — 손상되었거나 예상과 다른 파일: {}", spec.name));
    }
    std::fs::write(&part, &buf).map_err(|e| e.to_string())?;
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(coverage))]
#[test]
#[ignore = "downloads ~1GB; run manually to verify the real URL/SHA"]
fn real_download_verifies() {
    let tmp = std::env::temp_dir().join("disksage-model-test.gguf");
    download_to(&DEFAULT, &tmp).unwrap();
    assert!(tmp.exists());
    let _ = std::fs::remove_file(&tmp);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verify_sha256_matches_known_vector() {
        // echo -n "abc" | sha256sum
        let want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", want));
        assert!(verify_sha256(b"abc", &want.to_uppercase())); // 대소문자 무관
    }
    #[test]
    fn verify_sha256_rejects_mismatch() {
        assert!(!verify_sha256(b"abc", "deadbeef")); // 길이/값 불일치
        assert!(!verify_sha256(
            b"xyz",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
    }
    #[test]
    fn default_spec_is_wellformed() {
        assert!(DEFAULT.url.starts_with("https://"));
        assert_eq!(DEFAULT.sha256_hex.len(), 64);
        assert!(DEFAULT.bytes > 0);
        assert!(!DEFAULT.name.is_empty());
    }
}
