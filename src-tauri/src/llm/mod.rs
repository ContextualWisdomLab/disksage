mod backend;
mod prompt;
mod verdict;
// coverage 빌드에서는 아직 이 재-export를 쓰는 소비자(commands 등)가 없어 unused_imports 경고 발생 — dead_code와 함께 억제
#[cfg_attr(coverage, allow(unused_imports))]
pub use backend::{choose_backend, Backend};
#[cfg_attr(coverage, allow(unused_imports))]
pub use prompt::{classify_prompt, summary_prompt, verdict_prompt, FileMeta};
#[cfg_attr(coverage, allow(unused_imports))]
pub use verdict::{FileVerdict, Verdict};
