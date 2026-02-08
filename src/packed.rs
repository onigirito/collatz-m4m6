//! パックドビット演算による m4/m6 走査。
//!
//! 64ペアを1つの u64 ワードにパックし、ビット並列で処理する。
//! キャリー解決には Kogge-Stone 並列プリフィックススキャンを使用。
//!
//! 各ペアは m6段 + m4段 の2段加算器。
//! ペア全体としてのキャリー伝播特性（G/P/K）は:
//!   G_pair = G_out | (P_out & G_mid)  — m4段でキャリー生成、またはm4段が伝播かつm6段が生成
//!   P_pair = P_out & P_mid            — 両段とも伝播
//!   K_pair = !G_pair & !P_pair        — それ以外（キャリー消滅）
//!
//! Kogge-Stone でワード内64ペア分のキャリーを並列解決し、
//! ワード間キャリーは逐次伝播する。

use crate::pair_number::PairNumber;
use crate::postprocess;

/// パックドスキャンの結果
#[derive(Debug, Clone)]
pub struct PackedStepResult {
    pub new_m4: Vec<u64>,
    pub new_m6: Vec<u64>,
    pub new_pair_count: usize,
    pub d: u64,
    pub exchanged: bool,
    pub g_count: u32,
    pub p_count: u32,
    pub k_count: u32,
    pub max_carry_chain: u32,
    pub g_masks: Vec<u64>,
    pub p_masks: Vec<u64>,
}

/// Kogge-Stone 並列プリフィックススキャン（ワード内）。
///
/// 入力: generate (g), propagate (p) の64ペア分のビットマスク
/// 出力: プリフィックス適用後の (g_prefix, p_prefix)
///
/// g_prefix[i] = 1 は「位置 0..=i のどこかで生成され、
/// そこから位置 i まで伝播が途切れなかった」ことを意味する。
/// これにより carry_out[i] = g_prefix[i] | (p_prefix[i] & carry_in_bit0)
///
/// 6イテレーションで64ビット分のプリフィックスを並列解決。
#[inline]
fn kogge_stone_prefix(mut g: u64, mut p: u64) -> (u64, u64) {
    // キャリーは低ビット→高ビットに伝播する。
    // ステップ k: 位置 i の (g, p) を位置 i-2^k の (g, p) と合成する。
    // 合成則: (g_hi, p_hi) ∘ (g_lo, p_lo) = (g_hi | (p_hi & g_lo), p_hi & p_lo)
    // 「位置 i-shift の情報を位置 i にアライン」するには左シフト。
    //
    // 境界条件: 位置 i < shift には前任者がない。
    // (g, p) の単位元は (0, 1) なので、シフトで空いた下位ビットは
    // g_shifted は 0（左シフトのデフォルト）、
    // p_shifted は 1 にパディングする必要がある。
    for shift in [1u32, 2, 4, 8, 16, 32] {
        let g_shifted = g << shift;  // 位置 i-shift の generate を位置 i に配置
        // p_shifted の下位 shift ビットを 1 で埋める（単位元のp=1）
        let p_shifted = (p << shift) | ((1u64 << shift) - 1);
        g = g | (p & g_shifted);
        p = p & p_shifted;
    }
    (g, p)
}

/// m4/m6 ワードから指定オフセットでシフトされたワードを抽出。
///
/// pair_index `start` から64ペア分を抽出する。
/// start < 0 の場合、下位ビットは0パディング。
/// start >= pair_count の場合、全ビット0。
#[inline]
fn extract_window(words: &[u64], pair_count: usize, start: isize) -> u64 {
    if start >= pair_count as isize {
        return 0;
    }

    if start < 0 {
        let abs_start = (-start) as u32;
        if abs_start >= 64 {
            return 0;
        }
        // 下位 abs_start ビットが0、残りはワード0からの値
        let w0 = if words.is_empty() { 0 } else { words[0] };
        let mut val = w0 << abs_start;
        // 範囲外ビットをマスク
        let effective_end = pair_count as isize - start; // 有効ビット数の上限
        if effective_end < 64 {
            let remaining = effective_end as usize;
            if remaining < 64 {
                val &= (1u64 << remaining) - 1;
            }
        }
        return val;
    }

    let start_u = start as usize;
    let word_idx = start_u / 64;
    let bit_off = start_u % 64;

    if bit_off == 0 {
        if word_idx < words.len() {
            let mut val = words[word_idx];
            let remaining = pair_count.saturating_sub(start_u);
            if remaining < 64 {
                val &= (1u64 << remaining) - 1;
            }
            val
        } else {
            0
        }
    } else {
        let lo = if word_idx < words.len() { words[word_idx] } else { 0 };
        let hi = if word_idx + 1 < words.len() { words[word_idx + 1] } else { 0 };
        let mut val = (lo >> bit_off) | (hi << (64 - bit_off));
        let remaining = pair_count.saturating_sub(start_u);
        if remaining < 64 {
            val &= (1u64 << remaining) - 1;
        }
        val
    }
}

/// majority(a, b, c) = (a & b) | (b & c) | (a & c)
#[inline]
fn majority(a: u64, b: u64, c: u64) -> u64 {
    (a & b) | (b & c) | (a & c)
}

/// パックドスキャンの共通処理。
/// 参照ビット p_r, q_r (m6段), p_l, q_l (m4段) のワードを受け取り、
/// Kogge-Stone でキャリーを解決して new_m4, new_m6 を計算する。
///
/// 各ペア i の演算:
///   m6段: sum_r = p_r[i] + q_r[i] + c_in[i]
///          new_m6[i] = sum_r & 1
///          c_mid[i] = sum_r >> 1
///   m4段: sum_l = p_l[i] + q_l[i] + c_mid[i]
///          new_m4[i] = sum_l & 1
///          c_out[i] = sum_l >> 1  → c_in[i+1]
///
/// ペアGPK（2段合成）:
///   G_mid = p_r & q_r,  P_mid = p_r ^ q_r  (m6段の GPK)
///   G_out = p_l & q_l,  P_out = p_l ^ q_l  (m4段の GPK)
///   G_pair = G_out | (P_out & G_mid)
///   P_pair = P_out & P_mid
fn packed_scan_word(
    p_r: u64, q_r: u64, p_l: u64, q_l: u64,
    carry_in: u64,  // 前ワードからの入力キャリー (0 or 1)
) -> (u64, u64, u64, u64, u64) {
    // m6段のビット単位GPK
    let g_mid = p_r & q_r;
    let p_mid = p_r ^ q_r;

    // m4段のビット単位GPK
    let g_out = p_l & q_l;
    let p_out = p_l ^ q_l;

    // ペアGPK (2段合成)
    let g_pair = g_out | (p_out & g_mid);
    let p_pair = p_out & p_mid;

    // Kogge-Stone でペアレベルのプリフィックスキャリーを解決
    let (g_pfx, p_pfx) = kogge_stone_prefix(g_pair, p_pair);

    // carry_after[i] = g_pfx[i] | (p_pfx[i] & carry_in)
    // carry_in はこのワードの最初のペアへの入力キャリー
    let carry_in_broadcast = if carry_in != 0 { u64::MAX } else { 0 };
    let carry_after = g_pfx | (p_pfx & carry_in_broadcast);

    // c_in[i]: ペア i への入力キャリー
    // c_in[0] = carry_in (前ワードから)
    // c_in[i] = carry_after[i-1] for i > 0
    let c_in_per_pair = (carry_after << 1) | carry_in;

    // m6段の全ビット加算
    // new_m6[i] = p_r[i] ^ q_r[i] ^ c_in[i]
    let new_m6 = p_mid ^ c_in_per_pair;

    // c_mid[i] = majority(p_r[i], q_r[i], c_in[i])
    let c_mid = majority(p_r, q_r, c_in_per_pair);

    // m4段の全ビット加算
    // new_m4[i] = p_l[i] ^ q_l[i] ^ c_mid[i]
    let new_m4 = p_out ^ c_mid;

    // 次ワードへのキャリー = carry_after の最上位ビット
    let carry_out = (carry_after >> 63) & 1;

    (new_m4, new_m6, carry_out, g_pair, p_pair)
}

/// x=3 専用パックドスキャン。
pub fn packed_step_3n1(pn: &PairNumber) -> PackedStepResult {
    packed_step_3n1_opt(pn, true)
}

/// x=3 専用パックドスキャン（GPK収集オプション付き）。
pub fn packed_step_3n1_opt(pn: &PairNumber, collect_gpk: bool) -> PackedStepResult {
    let k = pn.pair_count();
    let m4 = pn.m4_words();
    let m6 = pn.m6_words();

    let out_pairs = k + 2;
    let out_words = (out_pairs + 63) / 64;
    let gpk_word_count = if collect_gpk { (k + 63) / 64 } else { 0 };

    let mut new_m4 = vec![0u64; out_words];
    let mut new_m6 = vec![0u64; out_words];
    let mut g_masks = vec![0u64; gpk_word_count];
    let mut p_masks = vec![0u64; gpk_word_count];

    let mut carry = 1u64;

    for w in 0..out_words {
        let base = (w * 64) as isize;

        // x=3: ref_R(i) = (a[i-1], b[i]), ref_L(i) = (b[i], a[i])
        let a_cur = extract_window(m4, k, base);
        let b_cur = extract_window(m6, k, base);
        let a_prev = extract_window(m4, k, base - 1);

        let p_r = a_prev;
        let q_r = b_cur;
        let p_l = b_cur;
        let q_l = a_cur;

        let (m4w, m6w, c_out, g_pair, p_pair) =
            packed_scan_word(p_r, q_r, p_l, q_l, carry);

        new_m4[w] = m4w;
        new_m6[w] = m6w;

        if collect_gpk && w < gpk_word_count {
            g_masks[w] = g_pair;
            p_masks[w] = p_pair;
        }

        carry = c_out;
    }

    // 最上位ワードの余剰ビットをマスク
    mask_top_bits(&mut new_m4, out_pairs);
    mask_top_bits(&mut new_m6, out_pairs);

    let (g_count, p_count, k_count, max_carry_chain) = if collect_gpk {
        mask_top_bits(&mut g_masks, k);
        mask_top_bits(&mut p_masks, k);
        compute_gpk_stats(&g_masks, &p_masks, k)
    } else {
        (0, 0, 0, 0)
    };

    let pp = postprocess::postprocess(new_m4, new_m6, out_pairs);

    PackedStepResult {
        new_m4: pp.next.m4_words().to_vec(),
        new_m6: pp.next.m6_words().to_vec(),
        new_pair_count: pp.next.pair_count(),
        d: pp.d,
        exchanged: pp.exchanged,
        g_count,
        p_count,
        k_count,
        max_carry_chain,
        g_masks,
        p_masks,
    }
}

/// x=5 専用パックドスキャン。
pub fn packed_step_5n1(pn: &PairNumber) -> PackedStepResult {
    packed_step_5n1_opt(pn, true)
}

/// x=5 専用パックドスキャン（GPK収集オプション付き）。
pub fn packed_step_5n1_opt(pn: &PairNumber, collect_gpk: bool) -> PackedStepResult {
    let k = pn.pair_count();
    let m4 = pn.m4_words();
    let m6 = pn.m6_words();

    let out_pairs = k + 2;
    let out_words = (out_pairs + 63) / 64;
    let gpk_word_count = if collect_gpk { (k + 63) / 64 } else { 0 };

    let mut new_m4 = vec![0u64; out_words];
    let mut new_m6 = vec![0u64; out_words];
    let mut g_masks = vec![0u64; gpk_word_count];
    let mut p_masks = vec![0u64; gpk_word_count];

    let mut carry = 1u64;

    for w in 0..out_words {
        let base = (w * 64) as isize;

        // x=5: ref_R(i) = (b[i-1], b[i]), ref_L(i) = (a[i-1], a[i])
        let a_cur = extract_window(m4, k, base);
        let b_cur = extract_window(m6, k, base);
        let b_prev = extract_window(m6, k, base - 1);
        let a_prev = extract_window(m4, k, base - 1);

        let p_r = b_prev;
        let q_r = b_cur;
        let p_l = a_prev;
        let q_l = a_cur;

        let (m4w, m6w, c_out, g_pair, p_pair) =
            packed_scan_word(p_r, q_r, p_l, q_l, carry);

        new_m4[w] = m4w;
        new_m6[w] = m6w;

        if collect_gpk && w < gpk_word_count {
            g_masks[w] = g_pair;
            p_masks[w] = p_pair;
        }

        carry = c_out;
    }

    mask_top_bits(&mut new_m4, out_pairs);
    mask_top_bits(&mut new_m6, out_pairs);

    let (g_count, p_count, k_count, max_carry_chain) = if collect_gpk {
        mask_top_bits(&mut g_masks, k);
        mask_top_bits(&mut p_masks, k);
        compute_gpk_stats(&g_masks, &p_masks, k)
    } else {
        (0, 0, 0, 0)
    };

    let pp = postprocess::postprocess(new_m4, new_m6, out_pairs);

    PackedStepResult {
        new_m4: pp.next.m4_words().to_vec(),
        new_m6: pp.next.m6_words().to_vec(),
        new_pair_count: pp.next.pair_count(),
        d: pp.d,
        exchanged: pp.exchanged,
        g_count,
        p_count,
        k_count,
        max_carry_chain,
        g_masks,
        p_masks,
    }
}

/// 汎用パックドスキャン。
pub fn packed_step_generic(pn: &PairNumber, x: u64) -> PackedStepResult {
    packed_step_generic_opt(pn, x, true)
}

/// 汎用パックドスキャン（GPK収集オプション付き）。
pub fn packed_step_generic_opt(pn: &PairNumber, x: u64, collect_gpk: bool) -> PackedStepResult {
    let xm1 = x - 1;
    assert!(xm1.is_power_of_two(), "x-1 must be a power of 2");
    let s = xm1.trailing_zeros();
    let t = (s / 2) as isize;
    let s_is_even = s % 2 == 0;

    let k = pn.pair_count();
    let m4 = pn.m4_words();
    let m6 = pn.m6_words();

    let extra_pairs = ((s as usize + 1) / 2) + 1;
    let out_pairs = k + extra_pairs;
    let out_words = (out_pairs + 63) / 64;
    let gpk_word_count = if collect_gpk { (k + 63) / 64 } else { 0 };

    let mut new_m4 = vec![0u64; out_words];
    let mut new_m6 = vec![0u64; out_words];
    let mut g_masks = vec![0u64; gpk_word_count];
    let mut p_masks = vec![0u64; gpk_word_count];

    let mut carry = 1u64;

    for w in 0..out_words {
        let base = (w * 64) as isize;

        let a_cur = extract_window(m4, k, base);
        let b_cur = extract_window(m6, k, base);

        let (p_r, q_r, p_l, q_l) = if s_is_even {
            let b_shifted = extract_window(m6, k, base - t);
            let a_shifted = extract_window(m4, k, base - t);
            (b_shifted, b_cur, a_shifted, a_cur)
        } else {
            let a_shifted = extract_window(m4, k, base - t - 1);
            let b_shifted = extract_window(m6, k, base - t);
            (a_shifted, b_cur, b_shifted, a_cur)
        };

        let (m4w, m6w, c_out, g_pair, p_pair) =
            packed_scan_word(p_r, q_r, p_l, q_l, carry);

        new_m4[w] = m4w;
        new_m6[w] = m6w;

        if collect_gpk && w < gpk_word_count {
            g_masks[w] = g_pair;
            p_masks[w] = p_pair;
        }

        carry = c_out;
    }

    mask_top_bits(&mut new_m4, out_pairs);
    mask_top_bits(&mut new_m6, out_pairs);

    let (g_count, p_count, k_count, max_carry_chain) = if collect_gpk {
        mask_top_bits(&mut g_masks, k);
        mask_top_bits(&mut p_masks, k);
        compute_gpk_stats(&g_masks, &p_masks, k)
    } else {
        (0, 0, 0, 0)
    };

    let pp = postprocess::postprocess(new_m4, new_m6, out_pairs);

    PackedStepResult {
        new_m4: pp.next.m4_words().to_vec(),
        new_m6: pp.next.m6_words().to_vec(),
        new_pair_count: pp.next.pair_count(),
        d: pp.d,
        exchanged: pp.exchanged,
        g_count,
        p_count,
        k_count,
        max_carry_chain,
        g_masks,
        p_masks,
    }
}

/// GPK カウントを popcount で高速計算（max_carry_chain なし）
fn compute_gpk_counts(g_masks: &[u64], p_masks: &[u64], pair_count: usize) -> (u32, u32, u32) {
    let mut g_count = 0u32;
    let mut p_count = 0u32;
    for w in 0..g_masks.len() {
        g_count += g_masks[w].count_ones();
        p_count += p_masks[w].count_ones();
    }
    let k_count = pair_count as u32 - g_count - p_count;
    (g_count, p_count, k_count)
}

/// GPK 統計を計算（popcount + キャリー連鎖長）
fn compute_gpk_stats(g_masks: &[u64], p_masks: &[u64], pair_count: usize) -> (u32, u32, u32, u32) {
    let (g_count, p_count, k_count) = compute_gpk_counts(g_masks, p_masks, pair_count);

    // max_carry_chain: 逐次走査が必要（キャリー状態に依存）
    let mut chain = 0u32;
    let mut max_chain = 0u32;
    let mut carry = true;

    for i in 0..pair_count {
        let word_idx = i / 64;
        let bit_idx = i % 64;
        let is_g = (g_masks[word_idx] >> bit_idx) & 1 != 0;
        let is_p = (p_masks[word_idx] >> bit_idx) & 1 != 0;

        if is_g {
            chain += 1;
            carry = true;
        } else if is_p {
            if carry { chain += 1; }
        } else {
            if chain > max_chain { max_chain = chain; }
            chain = 0;
            carry = false;
        }
    }
    if chain > max_chain { max_chain = chain; }

    (g_count, p_count, k_count, max_chain)
}

/// 最上位ワードの余剰ビットをマスク
fn mask_top_bits(words: &mut [u64], pair_count: usize) {
    if words.is_empty() { return; }
    let remainder = pair_count % 64;
    if remainder > 0 {
        let last = words.len() - 1;
        words[last] &= (1u64 << remainder) - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;
    use num_traits::One;

    /// Kogge-Stone 基本テスト
    #[test]
    fn test_kogge_stone_simple() {
        // 全 generate → プリフィックスも全 generate
        let (g, _p) = kogge_stone_prefix(u64::MAX, 0);
        assert_eq!(g, u64::MAX);

        // 全 kill → プリフィックスは generate なし
        let (g, p) = kogge_stone_prefix(0, 0);
        assert_eq!(g, 0);
        assert_eq!(p, 0);

        // 全 propagate → プリフィックスは all propagate
        let (g, p) = kogge_stone_prefix(0, u64::MAX);
        assert_eq!(g, 0);
        assert_eq!(p, u64::MAX);

        // ビット0だけ generate, 残り propagate → 全ビットに伝播
        let (g, _p) = kogge_stone_prefix(1, u64::MAX & !1);
        assert_eq!(g, u64::MAX); // bit0のgenerateが全位置に伝播
    }

    /// extract_window テスト
    #[test]
    fn test_extract_window() {
        let words = vec![0xFF00FF00FF00FF00u64, 0x0F0F0F0F0F0F0F0Fu64];
        assert_eq!(extract_window(&words, 128, 0), words[0]);
        assert_eq!(extract_window(&words, 128, 64), words[1]);
        // 負のオフセット
        assert_eq!(extract_window(&words, 128, -1), words[0] << 1);
        // 範囲外
        assert_eq!(extract_window(&words, 128, 128), 0);
    }

    /// パックド版と逐次版の結果一致テスト（3n+1）
    #[test]
    fn test_packed_3n1_vs_sequential() {
        for n_val in (1u64..=999).step_by(2) {
            let n = BigUint::from(n_val);
            let pn = PairNumber::from_biguint(&n);

            let packed = packed_step_3n1(&pn);
            let seq = crate::scan::collatz_step_3n1(&pn);

            let packed_next = PairNumber::from_packed(
                packed.new_m4.clone(), packed.new_m6.clone(), packed.new_pair_count);
            assert_eq!(
                packed_next.to_biguint(), seq.next.to_biguint(),
                "n' mismatch for 3n+1, n={}", n_val
            );
            assert_eq!(packed.d, seq.d, "d mismatch for 3n+1, n={}", n_val);
            assert_eq!(packed.exchanged, seq.exchanged, "exchanged mismatch for 3n+1, n={}", n_val);
            assert_eq!(packed.g_count, seq.gpk.g_count, "g_count mismatch for 3n+1, n={}", n_val);
            assert_eq!(packed.p_count, seq.gpk.p_count, "p_count mismatch for 3n+1, n={}", n_val);
            assert_eq!(packed.k_count, seq.gpk.k_count, "k_count mismatch for 3n+1, n={}", n_val);
            assert_eq!(packed.max_carry_chain, seq.gpk.max_carry_chain,
                "max_carry_chain mismatch for 3n+1, n={}", n_val);
        }
    }

    /// パックド版と逐次版の結果一致テスト（5n+1）
    #[test]
    fn test_packed_5n1_vs_sequential() {
        for n_val in (1u64..=999).step_by(2) {
            let n = BigUint::from(n_val);
            let pn = PairNumber::from_biguint(&n);

            let packed = packed_step_5n1(&pn);
            let seq = crate::scan::collatz_step_5n1(&pn);

            let packed_next = PairNumber::from_packed(
                packed.new_m4.clone(), packed.new_m6.clone(), packed.new_pair_count);
            assert_eq!(
                packed_next.to_biguint(), seq.next.to_biguint(),
                "n' mismatch for 5n+1, n={}", n_val
            );
            assert_eq!(packed.d, seq.d, "d mismatch for 5n+1, n={}", n_val);
            assert_eq!(packed.g_count, seq.gpk.g_count, "g_count mismatch for 5n+1, n={}", n_val);
            assert_eq!(packed.p_count, seq.gpk.p_count, "p_count mismatch for 5n+1, n={}", n_val);
            assert_eq!(packed.k_count, seq.gpk.k_count, "k_count mismatch for 5n+1, n={}", n_val);
        }
    }

    /// パックド汎用版の一致テスト
    #[test]
    fn test_packed_generic_vs_sequential() {
        for x in [3u64, 5, 9, 17, 33, 65] {
            for n_val in (1u64..=199).step_by(2) {
                let n = BigUint::from(n_val);
                let pn = PairNumber::from_biguint(&n);

                let packed = packed_step_generic(&pn, x);
                let seq = crate::scan::collatz_step(&pn, x);

                let packed_next = PairNumber::from_packed(
                    packed.new_m4.clone(), packed.new_m6.clone(), packed.new_pair_count);
                assert_eq!(
                    packed_next.to_biguint(), seq.next.to_biguint(),
                    "n' mismatch for {}n+1, n={}", x, n_val
                );
                assert_eq!(packed.d, seq.d, "d mismatch for {}n+1, n={}", x, n_val);
            }
        }
    }

    /// 大数のパックド一致テスト
    #[test]
    fn test_packed_large_3n1() {
        let n = (BigUint::one() << 1000u32) - BigUint::one();
        let pn = PairNumber::from_biguint(&n);

        let packed = packed_step_3n1(&pn);
        let seq = crate::scan::collatz_step_3n1(&pn);

        let packed_next = PairNumber::from_packed(
            packed.new_m4.clone(), packed.new_m6.clone(), packed.new_pair_count);
        assert_eq!(packed_next.to_biguint(), seq.next.to_biguint(), "large n' mismatch");
        assert_eq!(packed.d, seq.d, "large d mismatch");
        assert_eq!(packed.g_count, seq.gpk.g_count, "large g_count mismatch");
    }

    /// 連続ステップテスト（パックド版で軌道を追跡）
    #[test]
    fn test_packed_multi_step() {
        let mut n = BigUint::from(27u64);
        let mut pn = PairNumber::from_biguint(&n);

        for _step in 0..20 {
            let packed = packed_step_3n1(&pn);

            let xn1 = &n * BigUint::from(3u64) + BigUint::one();
            let d_arith = xn1.trailing_zeros().unwrap_or(0);
            let n_next_arith = &xn1 >> d_arith;

            let packed_next = PairNumber::from_packed(
                packed.new_m4.clone(), packed.new_m6.clone(), packed.new_pair_count);
            let n_next = packed_next.to_biguint();

            assert_eq!(packed.d, d_arith, "d mismatch");
            assert_eq!(n_next, n_next_arith, "n' mismatch");

            n = n_next;
            pn = packed_next;

            if pn.is_one() { break; }
        }
    }

    /// 非常に大きい数のテスト（ワード境界を跨ぐ）
    #[test]
    fn test_packed_large_5n1() {
        let n = (BigUint::one() << 10000u32) - BigUint::one();
        let pn = PairNumber::from_biguint(&n);

        let packed = packed_step_5n1(&pn);
        let seq = crate::scan::collatz_step_5n1(&pn);

        let packed_next = PairNumber::from_packed(
            packed.new_m4.clone(), packed.new_m6.clone(), packed.new_pair_count);
        assert_eq!(packed_next.to_biguint(), seq.next.to_biguint(), "large 5n+1 n' mismatch");
        assert_eq!(packed.d, seq.d, "large 5n+1 d mismatch");
    }
}
