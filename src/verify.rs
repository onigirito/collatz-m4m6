use num_bigint::BigUint;
use num_traits::One;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::scan::GpkStats;
use crate::trajectory;

/// 範囲検証の結果
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// 検証した奇数の総数
    pub total_checked: u64,
    /// 全て収束したか
    pub all_converged: bool,
    /// 最大停止時間
    pub max_stopping_time: u64,
    /// 最大停止時間を持つ数
    pub max_stopping_time_number: BigUint,
    /// 収束しなかった数（max_steps 超過）
    pub failures: Vec<BigUint>,
    /// GPK 統計情報
    pub gpk_stats: GpkStats,
}

/// [start, end] の全奇数を停止時間法で検証する（シングルスレッド版）。
/// progress_callback: (完了数, 総数) を定期的に呼ぶ。
pub fn verify_range(
    start: &BigUint,
    end: &BigUint,
    x: u64,
    max_steps: u64,
    progress_callback: impl Fn(u64, u64),
) -> VerifyResult {
    let two = BigUint::from(2u64);
    let one = BigUint::one();

    // start を奇数に調整
    let mut n = start.clone();
    if &n % &two == BigUint::ZERO {
        n += &one;
    }

    // 奇数の総数を概算
    let range = if end >= &n {
        end - &n
    } else {
        BigUint::ZERO
    };
    let total_estimate: u64 = (&range / &two).to_u64_digits().first().copied().unwrap_or(0) + 1;

    let mut total_checked = 0u64;
    let mut max_stopping_time = 0u64;
    let mut max_stopping_time_number = n.clone();
    let mut failures: Vec<BigUint> = Vec::new();
    let mut gpk_stats = GpkStats::new();

    while n <= *end {
        match trajectory::stopping_time_with_gpk(&n, x, max_steps, Some(&mut gpk_stats)) {
            Some(st) => {
                if st > max_stopping_time {
                    max_stopping_time = st;
                    max_stopping_time_number = n.clone();
                }
            }
            None => {
                failures.push(n.clone());
            }
        }

        total_checked += 1;

        if total_checked % 1000 == 0 {
            progress_callback(total_checked, total_estimate);
        }

        n += &two;
    }

    progress_callback(total_checked, total_estimate);

    VerifyResult {
        total_checked,
        all_converged: failures.is_empty(),
        max_stopping_time,
        max_stopping_time_number,
        failures,
        gpk_stats,
    }
}

/// [start, end] の全奇数を停止時間法で検証する（並列版）。
/// Rayon でチャンク分割して並列処理。
/// progress_callback: (完了数, 総数) を定期的に呼ぶ（スレッドセーフ）。
pub fn verify_range_parallel(
    start: &BigUint,
    end: &BigUint,
    x: u64,
    max_steps: u64,
    progress_callback: impl Fn(u64, u64) + Sync,
) -> VerifyResult {
    let two = BigUint::from(2u64);
    let one = BigUint::one();

    // start を奇数に調整
    let mut adj_start = start.clone();
    if &adj_start % &two == BigUint::ZERO {
        adj_start += &one;
    }

    // u64 に収まる範囲なら高速パスを使う
    let start_u64 = adj_start.to_u64_digits();
    let end_u64 = end.to_u64_digits();

    if start_u64.len() <= 1 && end_u64.len() <= 1 {
        let s = start_u64.first().copied().unwrap_or(1);
        let e = end_u64.first().copied().unwrap_or(0);
        return verify_range_parallel_u64(s, e, x, max_steps, true, &progress_callback);
    }

    // BigUint の場合はシングルスレッド版にフォールバック
    verify_range(&adj_start, end, x, max_steps, progress_callback)
}

/// u64 範囲の並列検証（高速パス）
fn verify_range_parallel_u64(
    start: u64,
    end: u64,
    x: u64,
    max_steps: u64,
    use_phase1: bool,
    progress_callback: &(impl Fn(u64, u64) + Sync),
) -> VerifyResult {
    // start を奇数に調整
    let start = if start % 2 == 0 { start + 1 } else { start };
    if start > end {
        return VerifyResult {
            total_checked: 0,
            all_converged: true,
            max_stopping_time: 0,
            max_stopping_time_number: BigUint::ZERO,
            failures: Vec::new(),
            gpk_stats: GpkStats::new(),
        };
    }

    let total_odd = (end - start) / 2 + 1;

    // チャンク分割: 各チャンク10000個の奇数
    let chunk_size: u64 = 10000;
    let num_chunks = (total_odd + chunk_size - 1) / chunk_size;

    let global_done = AtomicU64::new(0);
    let global_max_st = AtomicU64::new(0);
    let global_max_st_n = Mutex::new(start);
    let global_failures: Mutex<Vec<BigUint>> = Mutex::new(Vec::new());
    let global_gpk_stats: Mutex<GpkStats> = Mutex::new(GpkStats::new());

    (0..num_chunks).into_par_iter().for_each(|chunk_idx| {
        let chunk_start = start + chunk_idx * chunk_size * 2;
        let chunk_end = std::cmp::min(chunk_start + (chunk_size - 1) * 2, end);

        let mut local_max_st = 0u64;
        let mut local_max_st_n = chunk_start;
        let mut local_failures: Vec<BigUint> = Vec::new();
        let mut unreported = 0u64;
        let mut local_gpk = GpkStats::new();

        let mut n = chunk_start;
        while n <= chunk_end {
            match trajectory::stopping_time_u64_fast(n, x, max_steps, Some(&mut local_gpk), use_phase1) {
                Some(st) => {
                    if st > local_max_st {
                        local_max_st = st;
                        local_max_st_n = n;
                    }
                }
                None => {
                    local_failures.push(BigUint::from(n));
                }
            }
            unreported += 1;
            n += 2;

            // チャンク内でも定期的に進捗報告
            if unreported >= 100 {
                let done = global_done.fetch_add(unreported, Ordering::Relaxed) + unreported;
                progress_callback(done, total_odd);
                unreported = 0;
            }
        }

        // 残りをグローバルに反映
        if unreported > 0 {
            let done = global_done.fetch_add(unreported, Ordering::Relaxed) + unreported;
            progress_callback(done, total_odd);
        }

        // 最大停止時間を更新
        let prev_max = global_max_st.load(Ordering::Relaxed);
        if local_max_st > prev_max {
            global_max_st.fetch_max(local_max_st, Ordering::Relaxed);
            let mut guard = global_max_st_n.lock().unwrap();
            if local_max_st >= global_max_st.load(Ordering::Relaxed) {
                *guard = local_max_st_n;
            }
        }

        if !local_failures.is_empty() {
            global_failures.lock().unwrap().extend(local_failures);
        }

        global_gpk_stats.lock().unwrap().merge(&local_gpk);
    });

    let total_checked = global_done.load(Ordering::Relaxed);
    let max_stopping_time = global_max_st.load(Ordering::Relaxed);
    let max_stopping_time_number = BigUint::from(*global_max_st_n.lock().unwrap());
    let failures = global_failures.into_inner().unwrap();
    let gpk_stats = global_gpk_stats.into_inner().unwrap();

    VerifyResult {
        total_checked,
        all_converged: failures.is_empty(),
        max_stopping_time,
        max_stopping_time_number,
        failures,
        gpk_stats,
    }
}

/// キャンセル可能な並列検証。cancel が true になると途中結果を返す。
/// collect_gpk が false なら GPK 統計の収集をスキップして高速化。
pub fn verify_range_parallel_cancellable(
    start: &BigUint,
    end: &BigUint,
    x: u64,
    max_steps: u64,
    collect_gpk: bool,
    use_phase1: bool,
    cancel: &AtomicBool,
    progress_callback: impl Fn(u64, u64) + Sync,
) -> VerifyResult {
    let two = BigUint::from(2u64);
    let one = BigUint::one();

    let mut adj_start = start.clone();
    if &adj_start % &two == BigUint::ZERO {
        adj_start += &one;
    }

    let start_u64 = adj_start.to_u64_digits();
    let end_u64 = end.to_u64_digits();

    if start_u64.len() <= 1 && end_u64.len() <= 1 {
        let s = start_u64.first().copied().unwrap_or(1);
        let e = end_u64.first().copied().unwrap_or(0);
        return verify_range_parallel_u64_cancellable(s, e, x, max_steps, collect_gpk, use_phase1, cancel, &progress_callback);
    }

    // BigUint: シングルスレッド（キャンセル対応）
    let total_estimate: u64 = {
        let range = if end >= &adj_start { end - &adj_start } else { BigUint::ZERO };
        (&range / &two).to_u64_digits().first().copied().unwrap_or(0) + 1
    };

    let mut n = adj_start;
    let mut total_checked = 0u64;
    let mut max_stopping_time = 0u64;
    let mut max_stopping_time_number = n.clone();
    let mut failures: Vec<BigUint> = Vec::new();
    let mut gpk_stats = GpkStats::new();

    while n <= *end {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let gpk_arg = if collect_gpk { Some(&mut gpk_stats) } else { None };
        match trajectory::stopping_time_with_gpk(&n, x, max_steps, gpk_arg) {
            Some(st) => {
                if st > max_stopping_time {
                    max_stopping_time = st;
                    max_stopping_time_number = n.clone();
                }
            }
            None => {
                failures.push(n.clone());
            }
        }
        total_checked += 1;
        if total_checked % 1000 == 0 {
            progress_callback(total_checked, total_estimate);
        }
        n += &two;
    }

    progress_callback(total_checked, total_estimate);

    VerifyResult {
        total_checked,
        all_converged: failures.is_empty(),
        max_stopping_time,
        max_stopping_time_number,
        failures,
        gpk_stats,
    }
}

/// u64 範囲のキャンセル可能な並列検証
fn verify_range_parallel_u64_cancellable(
    start: u64,
    end: u64,
    x: u64,
    max_steps: u64,
    collect_gpk: bool,
    use_phase1: bool,
    cancel: &AtomicBool,
    progress_callback: &(impl Fn(u64, u64) + Sync),
) -> VerifyResult {
    let start = if start % 2 == 0 { start + 1 } else { start };
    if start > end {
        return VerifyResult {
            total_checked: 0,
            all_converged: true,
            max_stopping_time: 0,
            max_stopping_time_number: BigUint::ZERO,
            failures: Vec::new(),
            gpk_stats: GpkStats::new(),
        };
    }

    let total_odd = (end - start) / 2 + 1;
    let chunk_size: u64 = 10000;
    let num_chunks = (total_odd + chunk_size - 1) / chunk_size;

    let global_done = AtomicU64::new(0);
    let global_max_st = AtomicU64::new(0);
    let global_max_st_n = Mutex::new(start);
    let global_failures: Mutex<Vec<BigUint>> = Mutex::new(Vec::new());
    let global_gpk_stats: Mutex<GpkStats> = Mutex::new(GpkStats::new());

    (0..num_chunks).into_par_iter().for_each(|chunk_idx| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let chunk_start = start + chunk_idx * chunk_size * 2;
        let chunk_end = std::cmp::min(chunk_start + (chunk_size - 1) * 2, end);

        let mut local_max_st = 0u64;
        let mut local_max_st_n = chunk_start;
        let mut local_failures: Vec<BigUint> = Vec::new();
        let mut unreported = 0u64;
        let mut local_gpk = GpkStats::new();

        let mut n = chunk_start;
        while n <= chunk_end {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let gpk_arg = if collect_gpk { Some(&mut local_gpk) } else { None };
            match trajectory::stopping_time_u64_fast(n, x, max_steps, gpk_arg, use_phase1) {
                Some(st) => {
                    if st > local_max_st {
                        local_max_st = st;
                        local_max_st_n = n;
                    }
                }
                None => {
                    local_failures.push(BigUint::from(n));
                }
            }
            unreported += 1;
            n += 2;

            // チャンク内でも定期的に進捗報告
            if unreported >= 100 {
                let done = global_done.fetch_add(unreported, Ordering::Relaxed) + unreported;
                progress_callback(done, total_odd);
                unreported = 0;
            }
        }

        // 残りをグローバルに反映
        if unreported > 0 {
            let done = global_done.fetch_add(unreported, Ordering::Relaxed) + unreported;
            progress_callback(done, total_odd);
        }

        let prev_max = global_max_st.load(Ordering::Relaxed);
        if local_max_st > prev_max {
            global_max_st.fetch_max(local_max_st, Ordering::Relaxed);
            let mut guard = global_max_st_n.lock().unwrap();
            if local_max_st >= global_max_st.load(Ordering::Relaxed) {
                *guard = local_max_st_n;
            }
        }

        if !local_failures.is_empty() {
            global_failures.lock().unwrap().extend(local_failures);
        }

        global_gpk_stats.lock().unwrap().merge(&local_gpk);
    });

    let total_checked = global_done.load(Ordering::Relaxed);
    let max_stopping_time = global_max_st.load(Ordering::Relaxed);
    let max_stopping_time_number = BigUint::from(*global_max_st_n.lock().unwrap());
    let failures = global_failures.into_inner().unwrap();
    let gpk_stats = global_gpk_stats.into_inner().unwrap();

    VerifyResult {
        total_checked,
        all_converged: failures.is_empty(),
        max_stopping_time,
        max_stopping_time_number,
        failures,
        gpk_stats,
    }
}
