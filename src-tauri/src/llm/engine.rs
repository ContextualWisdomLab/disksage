//! 실제 llama.cpp CPU 추론(llama-cpp-2). cfg(all(not(coverage), feature="llm-engine"))로 게이트/기본 빌드서 제외.
//! 스펙 §6: temperature 0(결정적), 메타데이터만, 강제 JSON. 백엔드는 M5에선 CPU 고정(GPU는 M6).
use std::num::NonZeroU32;
use std::path::Path;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;

use crate::llm::InferenceEngine;

const MAX_TOKENS: i32 = 256;
const N_CTX: u32 = 4096;

pub struct LlamaEngine {
    backend: LlamaBackend,
    model: LlamaModel,
}

impl LlamaEngine {
    /// 모델을 한 번 로드(백엔드+모델). GPU 층수는 기본 999(가능한 만큼 GPU로 오프로드) —
    /// GPU 백엔드(CUDA/Vulkan/Metal)가 컴파일돼 있으면 그걸 쓰고, 없으면(CPU 빌드) 무시되어 CPU.
    /// `DISKSAGE_GPU_LAYERS`로 오버라이드(0이면 CPU 강제 — GPU 대비 검증용). 실패는 Err(문자열).
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        let gpu_layers: u32 = std::env::var("DISKSAGE_GPU_LAYERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(999);
        let params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);
        let model = LlamaModel::load_from_file(&backend, model_path, &params).map_err(|e| e.to_string())?;
        Ok(Self { backend, model })
    }
}

impl InferenceEngine for LlamaEngine {
    fn infer(&self, prompt: &str) -> Result<String, String> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX));
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| e.to_string())?;

        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| e.to_string())?;

        let mut batch = LlamaBatch::new(512, 1);
        let last = tokens.len().saturating_sub(1);
        for (i, tok) in tokens.iter().enumerate() {
            batch.add(*tok, i as i32, &[0], i == last).map_err(|e| e.to_string())?;
        }
        ctx.decode(&mut batch).map_err(|e| e.to_string())?;

        // temperature 0 → greedy
        let mut sampler = LlamaSampler::greedy();
        let mut out = String::new();
        let mut n_cur = batch.n_tokens();
        let mut generated = 0i32;
        while generated < MAX_TOKENS {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if self.model.is_eog_token(token) {
                break;
            }
            // token_to_str/Special are deprecated since 0.1.0 in favor of token_to_piece, which
            // requires threading an encoding_rs::Decoder through the caller. Not worth a new
            // dependency just to silence a warning for a single-token-at-a-time greedy loop.
            #[allow(deprecated)]
            out.push_str(&self.model.token_to_str(token, Special::Tokenize).map_err(|e| e.to_string())?);
            batch.clear();
            batch.add(token, n_cur, &[0], true).map_err(|e| e.to_string())?;
            n_cur += 1;
            generated += 1;
            ctx.decode(&mut batch).map_err(|e| e.to_string())?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{verdict_for, FileMeta, Verdict};

    #[test]
    #[ignore = "requires --features llm-engine + DISKSAGE_MODEL env pointing at a GGUF; run manually"]
    fn real_engine_returns_a_rated_verdict() {
        let path = std::env::var("DISKSAGE_MODEL").expect("set DISKSAGE_MODEL to a .gguf path");
        let engine = LlamaEngine::new(std::path::Path::new(&path)).unwrap();
        let meta = FileMeta { path: "/tmp/x.log".into(), name: "x.log".into(), size: 10, mtime_days: 1, parent: "tmp".into() };
        let fv = verdict_for(&engine, &meta);
        assert_ne!(fv.verdict, Verdict::Unrated); // 실제 모델이면 safe/caution/keep 중 하나
    }
}
