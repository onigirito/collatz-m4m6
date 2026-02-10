use num_bigint::BigUint;
use num_traits::One;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use crate::packed;
use crate::pair_number::PairNumber;
use crate::scan::{self, GpkInfo, GpkStats};

// ============================================================
// U256: スタック割当の256bit符号なし整数（Phase 1.5 用）
// ============================================================
#[derive(Clone, Copy)]
struct U256([u64; 4]); // [lo, lo-mid, hi-mid, hi]

impl U256 {
    #[inline]
    fn from_u128(v: u128) -> Self {
        U256([v as u64, (v >> 64) as u64, 0, 0])
    }

    /// x (小定数) との乗算。オーバーフローなら None。
    #[inline]
    fn mul_small_checked(self, x: u64) -> Option<Self> {
        let mut result = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let prod = self.0[i] as u128 * x as u128 + carry;
            result[i] = prod as u64;
            carry = prod >> 64;
        }
        if carry != 0 { return None; }
        Some(U256(result))
    }

    #[inline]
    fn add_one(mut self) -> Self {
        for i in 0..4 {
            let (val, overflow) = self.0[i].overflowing_add(1);
            self.0[i] = val;
            if !overflow { return self; }
            // overflow → carry to next limb
        }
        self // 256bit overflow (shouldn't happen in practice)
    }

    #[inline]
    fn trailing_zeros(self) -> u32 {
        for i in 0..4 {
            if self.0[i] != 0 {
                return i as u32 * 64 + self.0[i].trailing_zeros();
            }
        }
        256
    }

    #[inline]
    fn shr(self, d: u32) -> Self {
        if d == 0 { return self; }
        if d >= 256 { return U256([0; 4]); }
        let word_shift = (d / 64) as usize;
        let bit_shift = d % 64;
        let mut result = [0u64; 4];
        for i in 0..4 {
            let src = i + word_shift;
            if src < 4 {
                result[i] = self.0[src] >> bit_shift;
                if bit_shift > 0 && src + 1 < 4 {
                    result[i] |= self.0[src + 1] << (64 - bit_shift);
                }
            }
        }
        U256(result)
    }

    #[inline]
    fn is_one(self) -> bool {
        self.0[0] == 1 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    #[inline]
    fn lt_u128(self, v: u128) -> bool {
        if self.0[3] != 0 || self.0[2] != 0 { return false; }
        let self_lo = self.0[0] as u128 | ((self.0[1] as u128) << 64);
        self_lo < v
    }

    #[inline]
    fn to_biguint(self) -> BigUint {
        let bytes: Vec<u8> = self.0.iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        BigUint::from_bytes_le(&bytes)
    }

    #[inline]
    #[allow(dead_code)]
    fn bit_len(self) -> u32 {
        for i in (0..4).rev() {
            if self.0[i] != 0 {
                return i as u32 * 64 + (64 - self.0[i].leading_zeros());
            }
        }
        0
    }
}

/// m4/m6 ペアステップ情報
#[derive(Debug, Clone)]
pub struct PairStep {
    /// m4 ワード列 (パックドビット, LSBペア順, 64ペア/ワード)
    pub m4_words: Vec<u64>,
    /// m6 ワード列 (パックドビット, LSBペア順, 64ペア/ワード)
    pub m6_words: Vec<u64>,
    /// ペア数
    pub pair_count: usize,
    /// d 値
    pub d: u64,
    /// m4/m6 交換が発生したか
    pub exchanged: bool,
    /// postprocess前の偶数状態 xn+1 の m4 ワード列
    pub raw_m4_words: Vec<u64>,
    /// postprocess前の偶数状態 xn+1 の m6 ワード列
    pub raw_m6_words: Vec<u64>,
    /// postprocess前のペア数
    pub raw_pair_count: usize,
}

/// 軌道追跡の結果
#[derive(Debug, Clone)]
pub struct TrajectoryResult {
    /// 開始値
    pub start: BigUint,
    /// 軌道の各ステップ: (n, d) のペア
    pub steps: Vec<(BigUint, u64)>,
    /// 各ステップの m4/m6 ペア情報
    pub pair_steps: Vec<PairStep>,
    /// 各ステップの GPK 情報
    pub gpk_per_step: Vec<GpkInfo>,
    /// GPK 集約統計
    pub gpk_stats: GpkStats,
    /// 総ステップ数
    pub total_steps: u64,
    /// 最大値
    pub max_value: BigUint,
    /// 1 に到達したか
    pub reached_one: bool,
}

/// パックドワード列からビット文字列を生成 (MSB first)
pub fn words_to_bits_msb(words: &[u64], pair_count: usize) -> String {
    let mut s = String::with_capacity(pair_count);
    for i in (0..pair_count).rev() {
        let word_idx = i / 64;
        let bit_idx = i % 64;
        s.push(if (words[word_idx] >> bit_idx) & 1 != 0 { '1' } else { '0' });
    }
    s
}

/// 16述語のビット文字列を生成 (MSB first)
/// pred: 1〜16 (m1=FALSE, m2=AND, ..., m16=TRUE)
pub fn predicate_bits_msb(m4_words: &[u64], m6_words: &[u64], pair_count: usize, pred: u8) -> String {
    let word_count = m4_words.len();
    let mut pred_words = Vec::with_capacity(word_count);
    for w in 0..word_count {
        let m4 = m4_words[w];
        let m6 = m6_words[w];
        pred_words.push(match pred {
            1 => 0u64,
            2 => m4 & m6,
            3 => m4 & !m6,
            4 => m4,
            5 => !m4 & m6,
            6 => m6,
            7 => m4 ^ m6,
            8 => m4 | m6,
            9 => !m4 & !m6,
            10 => !(m4 ^ m6),
            11 => !m6,
            12 => m4 | !m6,
            13 => !m4,
            14 => !m4 | m6,
            15 => !(m4 & m6),
            16 => !0u64,
            _ => 0,
        });
    }
    words_to_bits_msb(&pred_words, pair_count)
}

/// 16述語の名称
pub const PREDICATE_NAMES: [&str; 16] = [
    "FALSE", "AND", "L>R", "LEFT", "R>L", "RIGHT", "XOR", "OR",
    "NOR", "XNOR", "NOT_R", "R→L", "NOT_L", "L→R", "NAND", "TRUE",
];

/// n=1 に到達するまで（または max_steps に達するまで）反復。
/// 内部は PairNumber のまま回して、BigUint 変換は記録時のみ行う。
pub fn trace_trajectory(start: &BigUint, x: u64, max_steps: u64) -> TrajectoryResult {
    trace_trajectory_with_callback(start, x, max_steps, |_, _, _| {})
}

/// 進捗コールバック付き軌道追跡。
/// callback(step, current_digits, d) を各ステップで呼ぶ。
pub fn trace_trajectory_with_callback(
    start: &BigUint,
    x: u64,
    max_steps: u64,
    callback: impl Fn(u64, usize, u64),
) -> TrajectoryResult {
    let mut pair = PairNumber::from_biguint(start);
    let mut steps: Vec<(BigUint, u64)> = Vec::new();
    let mut pair_steps: Vec<PairStep> = Vec::new();
    let mut gpk_per_step: Vec<GpkInfo> = Vec::new();
    let mut gpk_stats = GpkStats::new();
    let mut total_steps = 0u64;
    let mut max_value = start.clone();
    let mut reached_one = pair.is_one();

    // 初期値の m4/m6 を記録
    pair_steps.push(PairStep {
        m4_words: pair.m4_words().to_vec(),
        m6_words: pair.m6_words().to_vec(),
        pair_count: pair.pair_count(),
        d: 0, exchanged: false,
        raw_m4_words: Vec::new(), raw_m6_words: Vec::new(), raw_pair_count: 0,
    });

    while !reached_one && total_steps < max_steps {
        let result = if x == 3 {
            scan::collatz_step_3n1(&pair)
        } else if x == 5 {
            scan::collatz_step_5n1(&pair)
        } else {
            scan::collatz_step(&pair, x)
        };

        total_steps += 1;
        gpk_stats.accumulate(&result.gpk);
        gpk_per_step.push(result.gpk);

        // m4/m6 ワードを記録（偶数状態含む）
        pair_steps.push(PairStep {
            m4_words: result.next.m4_words().to_vec(),
            m6_words: result.next.m6_words().to_vec(),
            pair_count: result.next.pair_count(),
            d: result.d, exchanged: result.exchanged,
            raw_m4_words: result.raw_m4,
            raw_m6_words: result.raw_m6,
            raw_pair_count: result.raw_pair_count,
        });

        let n_val = result.next.to_biguint();

        if n_val > max_value {
            max_value = n_val.clone();
        }

        let digits = result.next.pair_count() * 2;
        callback(total_steps, digits, result.d);

        steps.push((n_val.clone(), result.d));

        if result.next.is_one() {
            reached_one = true;
        }

        // ビット長制限: 発散防止
        if result.next.pair_count() > MAX_PAIR_COUNT {
            break;
        }

        pair = result.next;
    }

    TrajectoryResult {
        start: start.clone(),
        steps,
        pair_steps,
        gpk_per_step,
        gpk_stats,
        total_steps,
        max_value,
        reached_one,
    }
}

/// ビット長制限（ペア数上限）。これを超えたら発散とみなして打ち切る。
/// 5n+1 等の非収束写像で BigUint がメモリを食い潰すのを防ぐ。
const MAX_PAIR_COUNT: usize = 10_000;

/// u128 値から直接 GPK 統計を計算する。
fn accumulate_gpk_u128(n: u128, x: u64, stats: &mut GpkStats) {
    if n == 0 { return; }
    let bit_len = 128 - n.leading_zeros() as usize;
    let pair_count = (bit_len + 1) / 2;

    let xm1 = x - 1;
    let s = xm1.trailing_zeros();
    let t = (s / 2) as isize;
    let s_is_even = s % 2 == 0;

    let get_a = |i: isize| -> u8 {
        if i < 0 || (i as usize) >= pair_count { return 0; }
        ((n >> (2 * i as usize + 1)) & 1) as u8
    };
    let get_b = |i: isize| -> u8 {
        if i < 0 || (i as usize) >= pair_count { return 0; }
        ((n >> (2 * i as usize)) & 1) as u8
    };

    let mut g_count = 0u32;
    let mut p_count = 0u32;
    let mut k_count = 0u32;
    let mut carry = true;
    let mut chain = 0u32;
    let mut max_chain = 0u32;

    for i in 0..pair_count {
        let ii = i as isize;
        let ai = get_a(ii);
        let bi = get_b(ii);

        let (p_r, q_r, p_l, q_l) = if s_is_even {
            (get_b(ii - t), bi, get_a(ii - t), ai)
        } else {
            (get_a(ii - t - 1), bi, get_b(ii - t), ai)
        };

        let g_mid = p_r & q_r;
        let p_mid = p_r ^ q_r;
        let g_out = p_l & q_l;
        let p_out = p_l ^ q_l;
        let g_i = g_out | (p_out & g_mid);
        let p_i = p_out & p_mid;

        if g_i != 0 {
            g_count += 1;
            chain += 1;
            carry = true;
        } else if p_i != 0 {
            p_count += 1;
            if carry { chain += 1; }
        } else {
            k_count += 1;
            if chain > max_chain { max_chain = chain; }
            chain = 0;
            carry = false;
        }
    }
    if chain > max_chain { max_chain = chain; }

    stats.total_g += g_count as u64;
    stats.total_p += p_count as u64;
    stats.total_k += k_count as u64;
    stats.total_pairs += pair_count as u64;
    stats.total_steps += 1;
    let idx = (max_chain as usize).min(127);
    stats.carry_chain_hist[idx] += 1;
}

/// U256 値から直接 GPK 統計を計算する。
fn accumulate_gpk_u256(n: &U256, x: u64, stats: &mut GpkStats) {
    let bl = n.bit_len();
    if bl == 0 { return; }
    let bit_len = bl as usize;
    let pair_count = (bit_len + 1) / 2;

    let xm1 = x - 1;
    let s = xm1.trailing_zeros();
    let t = (s / 2) as isize;
    let s_is_even = s % 2 == 0;

    // U256 からビット取得
    let get_bit = |pos: usize| -> u8 {
        if pos >= 256 { return 0; }
        let limb = pos / 64;
        let bit = pos % 64;
        ((n.0[limb] >> bit) & 1) as u8
    };
    let get_a = |i: isize| -> u8 {
        if i < 0 || (i as usize) >= pair_count { return 0; }
        get_bit(2 * i as usize + 1)
    };
    let get_b = |i: isize| -> u8 {
        if i < 0 || (i as usize) >= pair_count { return 0; }
        get_bit(2 * i as usize)
    };

    let mut g_count = 0u32;
    let mut p_count = 0u32;
    let mut k_count = 0u32;
    let mut carry = true;
    let mut chain = 0u32;
    let mut max_chain = 0u32;

    for i in 0..pair_count {
        let ii = i as isize;
        let ai = get_a(ii);
        let bi = get_b(ii);

        let (p_r, q_r, p_l, q_l) = if s_is_even {
            (get_b(ii - t), bi, get_a(ii - t), ai)
        } else {
            (get_a(ii - t - 1), bi, get_b(ii - t), ai)
        };

        let g_mid = p_r & q_r;
        let p_mid = p_r ^ q_r;
        let g_out = p_l & q_l;
        let p_out = p_l ^ q_l;
        let g_i = g_out | (p_out & g_mid);
        let p_i = p_out & p_mid;

        if g_i != 0 {
            g_count += 1;
            chain += 1;
            carry = true;
        } else if p_i != 0 {
            p_count += 1;
            if carry { chain += 1; }
        } else {
            k_count += 1;
            if chain > max_chain { max_chain = chain; }
            chain = 0;
            carry = false;
        }
    }
    if chain > max_chain { max_chain = chain; }

    stats.total_g += g_count as u64;
    stats.total_p += p_count as u64;
    stats.total_k += k_count as u64;
    stats.total_pairs += pair_count as u64;
    stats.total_steps += 1;
    let idx = (max_chain as usize).min(127);
    stats.carry_chain_hist[idx] += 1;
}

/// 停止時間法: n 未満の値に到達するまでのステップ数を返す。
/// max_steps 以内に到達しなければ None を返す。
pub fn stopping_time(n: &BigUint, x: u64, max_steps: u64) -> Option<u64> {
    stopping_time_with_gpk(n, x, max_steps, None, true)
}

/// 停止時間法（GPK 統計収集対応版）。パックドスキャンで高速化。
/// gpk_stats が Some なら各ステップの GPK を集約する。None なら GPK 計算をスキップ。
/// use_stopping_time が false なら n 未満判定をスキップし n=1 まで追跡する。
pub fn stopping_time_with_gpk(
    n: &BigUint,
    x: u64,
    max_steps: u64,
    mut gpk_stats: Option<&mut GpkStats>,
    use_stopping_time: bool,
) -> Option<u64> {
    if *n == BigUint::one() {
        return Some(0);
    }

    let collect_gpk = gpk_stats.is_some();
    let initial_pn = PairNumber::from_biguint(n);
    let mut pn = initial_pn.clone();
    let mut steps = 0u64;

    while steps < max_steps {
        let result = if x == 3 {
            packed::packed_step_3n1_opt(&pn, collect_gpk)
        } else if x == 5 {
            packed::packed_step_5n1_opt(&pn, collect_gpk)
        } else {
            packed::packed_step_generic_opt(&pn, x, collect_gpk)
        };

        if let Some(ref mut stats) = gpk_stats {
            stats.total_g += result.g_count as u64;
            stats.total_p += result.p_count as u64;
            stats.total_k += result.k_count as u64;
            stats.total_pairs += pn.pair_count() as u64;
            stats.total_steps += 1;
            let idx = (result.max_carry_chain as usize).min(127);
            stats.carry_chain_hist[idx] += 1;
        }

        let next = PairNumber::from_packed(
            result.new_m4, result.new_m6, result.new_pair_count);
        steps += 1;

        if next.is_one() {
            return Some(steps);
        }
        if use_stopping_time && next < initial_pn {
            return Some(steps);
        }
        // ビット長制限: 発散防止
        if next.pair_count() > MAX_PAIR_COUNT {
            return None;
        }

        pn = next;
    }

    None
}

/// u64 入力の高速停止時間計算。u128 演算を使い、オーバーフロー時はパックドスキャンにフォールバック。
/// use_phase1=false なら u128 フェーズをスキップし、最初からパックドスキャンで処理する。
/// use_stopping_time=false なら n 未満判定をスキップし n=1 まで追跡する。
pub fn stopping_time_u64_fast(
    n: u64,
    x: u64,
    max_steps: u64,
    mut gpk_stats: Option<&mut GpkStats>,
    use_phase1: bool,
    use_stopping_time: bool,
) -> Option<u64> {
    if n == 1 { return Some(0); }

    let x128 = x as u128;
    let n128 = n as u128;
    let mut current = n128;
    let overflow_limit = (u128::MAX - 1) / x128;
    let mut steps = 0u64;

    // Phase 1: u128 演算（use_phase1=false ならスキップ）
    while use_phase1 && steps < max_steps && current <= overflow_limit {
        if let Some(ref mut stats) = gpk_stats {
            accumulate_gpk_u128(current, x, stats);
        }

        let xn1 = current * x128 + 1;
        let d = xn1.trailing_zeros();
        current = xn1 >> d;
        steps += 1;

        if current == 1 {
            return Some(steps);
        }
        if use_stopping_time && current < n128 {
            return Some(steps);
        }
    }

    // Phase 1.5: U256 演算（u128 オーバーフロー時）
    if use_phase1 && steps < max_steps {
        let mut cur256 = U256::from_u128(current);

        while steps < max_steps {
            if let Some(ref mut stats) = gpk_stats {
                accumulate_gpk_u256(&cur256, x, stats);
            }

            let Some(xn1) = cur256.mul_small_checked(x).map(|v| v.add_one()) else {
                // U256 もオーバーフロー → Phase 2 へ
                let _ = current; // Phase 2 で cur256 から変換する
                let big_current = cur256.to_biguint();
                let collect_gpk = gpk_stats.is_some();
                let initial_pn = PairNumber::from_biguint(&BigUint::from(n));
                let mut pn = PairNumber::from_biguint(&big_current);

                while steps < max_steps {
                    let result = if x == 3 {
                        packed::packed_step_3n1_opt(&pn, collect_gpk)
                    } else if x == 5 {
                        packed::packed_step_5n1_opt(&pn, collect_gpk)
                    } else {
                        packed::packed_step_generic_opt(&pn, x, collect_gpk)
                    };

                    if let Some(ref mut stats) = gpk_stats {
                        stats.total_g += result.g_count as u64;
                        stats.total_p += result.p_count as u64;
                        stats.total_k += result.k_count as u64;
                        stats.total_pairs += pn.pair_count() as u64;
                        stats.total_steps += 1;
                        let idx = (result.max_carry_chain as usize).min(127);
                        stats.carry_chain_hist[idx] += 1;
                    }

                    let next = PairNumber::from_packed(
                        result.new_m4, result.new_m6, result.new_pair_count);
                    steps += 1;

                    if next.is_one() { return Some(steps); }
                    if use_stopping_time && next < initial_pn { return Some(steps); }
                    if next.pair_count() > MAX_PAIR_COUNT { return None; }
                    pn = next;
                }
                return None;
            };

            let d = xn1.trailing_zeros();
            cur256 = xn1.shr(d);
            steps += 1;

            if cur256.is_one() { return Some(steps); }
            if use_stopping_time && cur256.lt_u128(n128) { return Some(steps); }
        }
        return None;
    }

    // Phase 2: パックドスキャン フォールバック（use_phase1=false 時）
    let collect_gpk = gpk_stats.is_some();
    if steps < max_steps {
        let initial_pn = PairNumber::from_biguint(&BigUint::from(n));
        let big_current = BigUint::from(current);
        let mut pn = PairNumber::from_biguint(&big_current);

        while steps < max_steps {
            let result = if x == 3 {
                packed::packed_step_3n1_opt(&pn, collect_gpk)
            } else if x == 5 {
                packed::packed_step_5n1_opt(&pn, collect_gpk)
            } else {
                packed::packed_step_generic_opt(&pn, x, collect_gpk)
            };

            if let Some(ref mut stats) = gpk_stats {
                stats.total_g += result.g_count as u64;
                stats.total_p += result.p_count as u64;
                stats.total_k += result.k_count as u64;
                stats.total_pairs += pn.pair_count() as u64;
                stats.total_steps += 1;
                let idx = (result.max_carry_chain as usize).min(127);
                stats.carry_chain_hist[idx] += 1;
            }

            let next = PairNumber::from_packed(
                result.new_m4, result.new_m6, result.new_pair_count);
            steps += 1;

            if next.is_one() {
                return Some(steps);
            }
            if use_stopping_time && next < initial_pn {
                return Some(steps);
            }
            if next.pair_count() > MAX_PAIR_COUNT {
                return None;
            }

            pn = next;
        }
    }

    None
}

/// キャンセル可能な軌道追跡。cancel が true になると途中結果を返す。
pub fn trace_trajectory_cancellable(
    start: &BigUint,
    x: u64,
    max_steps: u64,
    cancel: &AtomicBool,
    callback: impl Fn(u64, usize, u64),
) -> TrajectoryResult {
    let mut pair = PairNumber::from_biguint(start);
    let mut steps: Vec<(BigUint, u64)> = Vec::new();
    let mut pair_steps: Vec<PairStep> = Vec::new();
    let mut gpk_per_step: Vec<GpkInfo> = Vec::new();
    let mut gpk_stats = GpkStats::new();
    let mut total_steps = 0u64;
    let mut max_value = start.clone();
    let mut reached_one = pair.is_one();

    // 初期値の m4/m6 を記録
    pair_steps.push(PairStep {
        m4_words: pair.m4_words().to_vec(),
        m6_words: pair.m6_words().to_vec(),
        pair_count: pair.pair_count(),
        d: 0, exchanged: false,
        raw_m4_words: Vec::new(), raw_m6_words: Vec::new(), raw_pair_count: 0,
    });

    while !reached_one && total_steps < max_steps {
        if cancel.load(AtomicOrdering::Relaxed) {
            break;
        }

        let result = if x == 3 {
            scan::collatz_step_3n1(&pair)
        } else if x == 5 {
            scan::collatz_step_5n1(&pair)
        } else {
            scan::collatz_step(&pair, x)
        };

        total_steps += 1;
        gpk_stats.accumulate(&result.gpk);
        gpk_per_step.push(result.gpk);

        // m4/m6 ワードを記録（偶数状態含む）
        pair_steps.push(PairStep {
            m4_words: result.next.m4_words().to_vec(),
            m6_words: result.next.m6_words().to_vec(),
            pair_count: result.next.pair_count(),
            d: result.d, exchanged: result.exchanged,
            raw_m4_words: result.raw_m4,
            raw_m6_words: result.raw_m6,
            raw_pair_count: result.raw_pair_count,
        });

        let n_val = result.next.to_biguint();

        if n_val > max_value {
            max_value = n_val.clone();
        }

        let digits = result.next.pair_count() * 2;
        callback(total_steps, digits, result.d);

        steps.push((n_val.clone(), result.d));

        if result.next.is_one() {
            reached_one = true;
        }

        // ビット長制限: 発散防止
        if result.next.pair_count() > MAX_PAIR_COUNT {
            break;
        }

        pair = result.next;
    }

    TrajectoryResult {
        start: start.clone(),
        steps,
        pair_steps,
        gpk_per_step,
        gpk_stats,
        total_steps,
        max_value,
        reached_one,
    }
}
