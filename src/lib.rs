//! コラッツ型写像 m4/m6 走査アルゴリズム
//!
//! 論文「2ビットペア述語体系によるコラッツ型写像の構造分解」(織々 嶌, 2026)
//! のアルゴリズム7.1（層1: 逐次走査）の Rust 実装。
//!
//! T(n) = (xn+1)/2^d の「奇数→奇数」1ステップを、
//! 乗算なしで m4/m6 ビットペアの走査のみで計算する。

pub mod packed;
pub mod pair_number;
pub mod postprocess;
pub mod reference;
pub mod scan;
pub mod trajectory;
pub mod verify;

pub use pair_number::PairNumber;
pub use scan::{collatz_step, collatz_step_3n1, collatz_step_5n1, Gpk, GpkInfo, GpkStats, StepResult};
pub use trajectory::{stopping_time, stopping_time_with_gpk, stopping_time_u64_fast, trace_trajectory, trace_trajectory_with_callback, trace_trajectory_cancellable, words_to_bits_msb, predicate_bits_msb, PREDICATE_NAMES, PairStep, TrajectoryResult};
pub use verify::{verify_range, verify_range_parallel, verify_range_parallel_cancellable, VerifyResult};
