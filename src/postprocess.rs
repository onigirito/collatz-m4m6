use crate::pair_number::PairNumber;

/// 後処理の中間結果
pub struct PostprocessResult {
    pub next: PairNumber,
    pub d: u64,
    pub exchanged: bool,
}

/// パックドワード列の後処理。
/// 1. MSB側の余分な (0,0) ペアを除去（最上位ゼロワード除去 + ビット精密トリム）
/// 2. 末尾ゼロペア計数 → d 計算
/// 3. d に応じてペア右シフトと m4⇔m6 交換
pub fn postprocess(new_m4: Vec<u64>, new_m6: Vec<u64>, raw_pair_count: usize) -> PostprocessResult {
    // 1. 実際のペア数を確定（MSB側 (0,0) トリム）
    let pair_count = trim_pair_count(&new_m4, &new_m6, raw_pair_count);

    if pair_count == 0 {
        return PostprocessResult {
            next: PairNumber::from_packed(vec![0], vec![0], 1),
            d: 0,
            exchanged: false,
        };
    }

    // 2. 末尾ゼロ計数（ファスナー展開ベース）
    // ファスナー展開: bit[2i] = m6[i], bit[2i+1] = m4[i]
    // 末尾ゼロ数 d を計算
    let d = count_trailing_zeros_packed(&new_m4, &new_m6, pair_count);

    // 3. d ビット右シフト → 再ペア化
    // d を「ペア単位シフト」と「ビット内オフセット」に分解
    // ファスナー展開でのビットシフトを直接 m4/m6 上で行う
    let (shifted_m4, shifted_m6, shifted_pair_count) = shift_right_bits(&new_m4, &new_m6, pair_count, d);

    let exchanged = d % 2 == 1;

    PostprocessResult {
        next: PairNumber::from_packed(shifted_m4, shifted_m6, shifted_pair_count),
        d,
        exchanged,
    }
}

/// 旧インターフェース互換: Vec<u8> per bit の入力を受け取る版
pub fn postprocess_legacy(new_m4_bits: Vec<u8>, new_m6_bits: Vec<u8>) -> PostprocessResult {
    // Vec<u8> → パックド変換
    let pair_count = new_m4_bits.len();
    let word_count = pair_count.div_ceil(64);
    let mut m4_words = vec![0u64; word_count];
    let mut m6_words = vec![0u64; word_count];

    for i in 0..pair_count {
        let word_idx = i / 64;
        let bit_idx = i % 64;
        m4_words[word_idx] |= (new_m4_bits[i] as u64) << bit_idx;
        m6_words[word_idx] |= (new_m6_bits[i] as u64) << bit_idx;
    }

    postprocess(m4_words, m6_words, pair_count)
}

/// MSBトリム: 最上位の非ゼロペアまでのペア数を返す
fn trim_pair_count(m4: &[u64], m6: &[u64], pair_count: usize) -> usize {
    if pair_count == 0 { return 0; }

    let mut k = pair_count;
    while k > 1 {
        let word_idx = (k - 1) / 64;
        let bit_idx = (k - 1) % 64;
        if word_idx >= m4.len() { k -= 1; continue; }
        let m4_bit = (m4[word_idx] >> bit_idx) & 1;
        let m6_bit = (m6[word_idx] >> bit_idx) & 1;
        if m4_bit == 0 && m6_bit == 0 {
            k -= 1;
        } else {
            break;
        }
    }
    k
}

/// ファスナー展開ベースの末尾ゼロ計数（パックド版・ワード並列）
/// ファスナー: bit[2i] = m6[i], bit[2i+1] = m4[i]
/// m8 (= m4|m6, OR) のワード演算で64ペア同時にゼロ判定。O(d/64)。
fn count_trailing_zeros_packed(m4: &[u64], m6: &[u64], pair_count: usize) -> u64 {
    let word_count = pair_count.div_ceil(64);
    let mut d = 0u64;
    for w in 0..word_count {
        let m4w = if w < m4.len() { m4[w] } else { 0 };
        let m6w = if w < m6.len() { m6[w] } else { 0 };
        let or_word = m4w | m6w; // m8: 非ゼロペアのビットが1
        if or_word == 0 {
            // 64ペア全部ゼロ → ファスナーで128ビット分の末尾ゼロ
            d += 128;
            continue;
        }
        // trailing_zeros = 連続ゼロペア数（このワード内）
        let tz = or_word.trailing_zeros() as u64;
        d += tz * 2; // 各ゼロペアはファスナーで2ビット
        // 境界ペア: m6ビットが0なら +1（a=1,b=0 → m3）
        if (m6w >> tz) & 1 == 0 {
            d += 1;
        }
        break;
    }
    d
}

/// d ビットの右シフト（ファスナー展開ベース）
/// ファスナー展開して d ビット右シフトし、再ペア化する。
/// d が偶数: ペア単位でシフト（m4/m6 の位置関係保持）
/// d が奇数: m4/m6 が交換される
fn shift_right_bits(
    m4: &[u64], m6: &[u64], pair_count: usize, d: u64,
) -> (Vec<u64>, Vec<u64>, usize) {
    if d == 0 {
        // トリミングのみ
        let word_count = pair_count.div_ceil(64);
        let mut rm4 = m4[..word_count].to_vec();
        let mut rm6 = m6[..word_count].to_vec();
        mask_top(&mut rm4, pair_count);
        mask_top(&mut rm6, pair_count);
        return (rm4, rm6, pair_count);
    }

    let total_bits = 2 * pair_count as u64;
    let remaining_bits = total_bits.saturating_sub(d);
    if remaining_bits == 0 {
        return (vec![0], vec![0], 1);
    }
    let new_pair_count = remaining_bits.div_ceil(2) as usize;
    if new_pair_count == 0 {
        return (vec![0], vec![0], 1);
    }

    let new_word_count = new_pair_count.div_ceil(64);
    let mut new_m4 = vec![0u64; new_word_count];
    let mut new_m6 = vec![0u64; new_word_count];

    let d_usize = d as usize;

    // シフト後のビット列を読む
    for out_bit in 0..remaining_bits as usize {
        // シフト前のファスナービット位置
        let src_bit = out_bit + d_usize;
        let src_pair = src_bit / 2;
        let src_is_m4 = (src_bit % 2) == 1;

        let bit_val = if src_pair < pair_count {
            let w = src_pair / 64;
            let b = src_pair % 64;
            if src_is_m4 {
                if w < m4.len() { (m4[w] >> b) & 1 } else { 0 }
            } else if w < m6.len() { (m6[w] >> b) & 1 } else { 0 }
        } else {
            0
        };

        // 出力先のファスナービット位置
        let out_pair = out_bit / 2;
        let out_is_m4 = (out_bit % 2) == 1;
        let ow = out_pair / 64;
        let ob = out_pair % 64;

        if out_is_m4 {
            new_m4[ow] |= bit_val << ob;
        } else {
            new_m6[ow] |= bit_val << ob;
        }
    }

    // MSBトリム
    let mut k = new_pair_count;
    while k > 1 {
        let w = (k - 1) / 64;
        let b = (k - 1) % 64;
        if (new_m4[w] >> b) & 1 == 0 && (new_m6[w] >> b) & 1 == 0 {
            k -= 1;
        } else {
            break;
        }
    }
    let final_word_count = k.div_ceil(64);
    new_m4.truncate(final_word_count);
    new_m6.truncate(final_word_count);
    mask_top(&mut new_m4, k);
    mask_top(&mut new_m6, k);

    (new_m4, new_m6, k)
}

/// 最上位ワードの余剰ビットをマスク
fn mask_top(words: &mut [u64], pair_count: usize) {
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

    #[test]
    fn test_postprocess_136() {
        // xn+1 = 136 = 10001000₂
        // m4 = [0, 1, 0, 1], m6 = [0, 0, 0, 0]
        // d=3, n'=17
        let result = postprocess_legacy(
            vec![0, 1, 0, 1],
            vec![0, 0, 0, 0],
        );
        assert_eq!(result.d, 3);
        assert!(result.exchanged); // d=3 は奇数
        let n_prime = result.next.to_biguint();
        assert_eq!(n_prime, num_bigint::BigUint::from(17u64));
    }

    #[test]
    fn test_postprocess_82() {
        // xn+1 = 82 = 1010010₂
        // m4 = [1, 0, 0, 0], m6 = [0, 0, 1, 1]
        // d=1, n'=41
        let result = postprocess_legacy(
            vec![1, 0, 0, 0],
            vec![0, 0, 1, 1],
        );
        assert_eq!(result.d, 1);
        assert!(result.exchanged); // d=1 は奇数
        let n_prime = result.next.to_biguint();
        assert_eq!(n_prime, num_bigint::BigUint::from(41u64));
    }
}
