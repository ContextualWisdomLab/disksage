//! GPU 백엔드 선택 — 순수 우선순위 로직(테스트 100%)과 하드웨어 프로브(FFI, cfg(not(coverage)))를 분리.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cuda,
    Vulkan,
    Metal,
    Cpu,
}

/// 자동 우선순위: CUDA > Metal > Vulkan > CPU. override_가 available에 있으면 그것을,
/// 없으면 자동 우선순위. available이 비면 항상 Cpu(최종 폴백).
pub fn choose_backend(available: &[Backend], override_: Option<Backend>) -> Backend {
    if let Some(b) = override_ {
        if available.contains(&b) {
            return b;
        }
    }
    for pref in [Backend::Cuda, Backend::Metal, Backend::Vulkan] {
        if available.contains(&pref) {
            return pref;
        }
    }
    Backend::Cpu
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefers_cuda_then_vulkan_then_cpu() {
        assert_eq!(choose_backend(&[Backend::Cpu, Backend::Vulkan, Backend::Cuda], None), Backend::Cuda);
        assert_eq!(choose_backend(&[Backend::Cpu, Backend::Vulkan], None), Backend::Vulkan);
        assert_eq!(choose_backend(&[Backend::Cpu], None), Backend::Cpu);
    }
    #[test]
    fn empty_available_falls_back_to_cpu() {
        assert_eq!(choose_backend(&[], None), Backend::Cpu);
    }
    #[test]
    fn honors_override_when_available() {
        assert_eq!(choose_backend(&[Backend::Cpu, Backend::Cuda], Some(Backend::Cpu)), Backend::Cpu);
    }
    #[test]
    fn ignores_override_when_unavailable() {
        assert_eq!(choose_backend(&[Backend::Cpu], Some(Backend::Cuda)), Backend::Cpu);
    }
    #[test]
    fn prefers_metal_over_vulkan() {
        // Metal이 Vulkan보다 우선 — choose_backend의 Metal 분기 커버
        assert_eq!(choose_backend(&[Backend::Vulkan, Backend::Metal, Backend::Cpu], None), Backend::Metal);
    }
}
