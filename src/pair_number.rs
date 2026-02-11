use std::cmp::Ordering;

use num_bigint::BigUint;
use num_traits::Zero;

/// 2ビットペア分解された自然数。
/// 内部表現は LSB順の m4/m6 パックドビット列（Vec<u64>、各ワード64ペア分）。
#[derive(Debug, Clone, Eq)]
pub struct PairNumber {
    /// m4 (左ビット列) パックド。ビット位置 i のペアは word[i/64] の (i%64) ビット目
    m4_words: Vec<u64>,
    /// m6 (右ビット列) パックド
    m6_words: Vec<u64>,
    /// 実ペア数
    pair_count: usize,
}

impl PartialEq for PairNumber {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl PartialOrd for PairNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PairNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1. pair_count が異なれば、多い方が大きい（MSBトリム済み前提）
        match self.pair_count.cmp(&other.pair_count) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // 2. MSBワードから順に、最上位の差分ペアで比較
        let words = self.m4_words.len();
        for w in (0..words).rev() {
            let diff_m4 = self.m4_words[w] ^ other.m4_words[w];
            let diff_m6 = self.m6_words[w] ^ other.m6_words[w];
            let diff_any = diff_m4 | diff_m6;
            if diff_any == 0 {
                continue;
            }
            // 最上位の差分ペア位置
            let top_bit = 63 - diff_any.leading_zeros();
            let mask = 1u64 << top_bit;
            // m4（上位ビット 2i+1）を先に比較
            let a_m4 = self.m4_words[w] & mask;
            let b_m4 = other.m4_words[w] & mask;
            if a_m4 != b_m4 {
                return if a_m4 != 0 { Ordering::Greater } else { Ordering::Less };
            }
            // m4同値ならm6（下位ビット 2i）で決定
            let a_m6 = self.m6_words[w] & mask;
            let b_m6 = other.m6_words[w] & mask;
            if a_m6 != b_m6 {
                return if a_m6 != 0 { Ordering::Greater } else { Ordering::Less };
            }
            unreachable!();
        }
        Ordering::Equal
    }
}

impl PairNumber {
    /// BigUint からペア数に変換。
    /// n の2進表現を偶数桁にパディングし、LSB側から2ビットずつペア分解する。
    pub fn from_biguint(n: &BigUint) -> Self {
        if n.is_zero() {
            return PairNumber {
                m4_words: vec![0],
                m6_words: vec![0],
                pair_count: 1,
            };
        }

        let bytes = n.to_bytes_le();
        let bit_len = n.bits() as usize;
        // 偶数ビット長にする
        let padded_bit_len = if !bit_len.is_multiple_of(2) { bit_len + 1 } else { bit_len };
        let pair_count = padded_bit_len / 2;
        let word_count = pair_count.div_ceil(64);

        let mut m4_words = vec![0u64; word_count];
        let mut m6_words = vec![0u64; word_count];

        for i in 0..pair_count {
            let bit_pos_m6 = 2 * i;     // 偶数ビット位置 = m6 (右)
            let bit_pos_m4 = 2 * i + 1; // 奇数ビット位置 = m4 (左)

            let m6_bit = if bit_pos_m6 / 8 < bytes.len() {
                ((bytes[bit_pos_m6 / 8] >> (bit_pos_m6 % 8)) & 1) as u64
            } else {
                0
            };
            let m4_bit = if bit_pos_m4 / 8 < bytes.len() {
                ((bytes[bit_pos_m4 / 8] >> (bit_pos_m4 % 8)) & 1) as u64
            } else {
                0
            };

            let word_idx = i / 64;
            let bit_idx = i % 64;
            m6_words[word_idx] |= m6_bit << bit_idx;
            m4_words[word_idx] |= m4_bit << bit_idx;
        }

        PairNumber { m4_words, m6_words, pair_count }
    }

    /// BigUint に復元。
    /// ファスナー構造（LSB first）: b[0], a[0], b[1], a[1], ...
    pub fn to_biguint(&self) -> BigUint {
        let k = self.pair_count;
        if k == 0 {
            return BigUint::zero();
        }

        // ビット長 = 2k
        let total_bits = 2 * k;
        let byte_count = total_bits.div_ceil(8);
        let mut bytes = vec![0u8; byte_count];

        for i in 0..k {
            let word_idx = i / 64;
            let bit_idx = i % 64;
            let m6_bit = ((self.m6_words[word_idx] >> bit_idx) & 1) as u8;
            let m4_bit = ((self.m4_words[word_idx] >> bit_idx) & 1) as u8;

            // m6 → 偶数ビット位置 (2i), m4 → 奇数ビット位置 (2i+1)
            let pos_m6 = 2 * i;
            let pos_m4 = 2 * i + 1;
            bytes[pos_m6 / 8] |= m6_bit << (pos_m6 % 8);
            bytes[pos_m4 / 8] |= m4_bit << (pos_m4 % 8);
        }

        BigUint::from_bytes_le(&bytes)
    }

    /// ペア数 k を返す
    pub fn pair_count(&self) -> usize {
        self.pair_count
    }

    /// ワード数を返す
    pub fn word_count(&self) -> usize {
        self.m4_words.len()
    }

    /// m4 ビットへのアクセス（範囲外は 0）
    pub fn get_m4(&self, i: isize) -> u8 {
        if i < 0 || i as usize >= self.pair_count {
            0
        } else {
            let idx = i as usize;
            ((self.m4_words[idx / 64] >> (idx % 64)) & 1) as u8
        }
    }

    /// m6 ビットへのアクセス（範囲外は 0）
    pub fn get_m6(&self, i: isize) -> u8 {
        if i < 0 || i as usize >= self.pair_count {
            0
        } else {
            let idx = i as usize;
            ((self.m6_words[idx / 64] >> (idx % 64)) & 1) as u8
        }
    }

    /// n=1 かどうか判定（BigUint変換なし）
    /// 1 = 01₂ → ペア: (a[0]=0, b[0]=1), k=1
    pub fn is_one(&self) -> bool {
        if self.pair_count != 1 {
            return false;
        }
        self.m4_words[0] == 0 && self.m6_words[0] == 1
    }

    /// m4 ワードスライスへのアクセス
    pub fn m4_words(&self) -> &[u64] {
        &self.m4_words
    }

    /// m6 ワードスライスへのアクセス
    pub fn m6_words(&self) -> &[u64] {
        &self.m6_words
    }

    /// パックドデータから構築
    pub fn from_packed(m4_words: Vec<u64>, m6_words: Vec<u64>, pair_count: usize) -> Self {
        PairNumber { m4_words, m6_words, pair_count }
    }

    /// 互換用: m4 を Vec<u8> で返す（表示・テスト用）
    pub fn m4_as_vec_u8(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.pair_count);
        for i in 0..self.pair_count {
            v.push(((self.m4_words[i / 64] >> (i % 64)) & 1) as u8);
        }
        v
    }

    /// 互換用: m6 を Vec<u8> で返す（表示・テスト用）
    pub fn m6_as_vec_u8(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.pair_count);
        for i in 0..self.pair_count {
            v.push(((self.m6_words[i / 64] >> (i % 64)) & 1) as u8);
        }
        v
    }

    /// m4/m6 ビット列からファスナー展開したビット列を返す（LSB first）
    pub fn to_bits_lsb(&self) -> Vec<u8> {
        let k = self.pair_count;
        let mut bits = Vec::with_capacity(2 * k);
        for i in 0..k {
            bits.push(self.get_m6(i as isize)); // 偶数位置 = m6 (右)
            bits.push(self.get_m4(i as isize)); // 奇数位置 = m4 (左)
        }
        bits
    }

    /// LSB first ビット列からペア数を構成
    pub fn from_bits_lsb(bits: &[u8]) -> Self {
        if bits.is_empty() {
            return PairNumber {
                m4_words: vec![0],
                m6_words: vec![0],
                pair_count: 1,
            };
        }

        let mut bits = bits.to_vec();
        // 偶数長に調整
        if !bits.len().is_multiple_of(2) {
            bits.push(0);
        }

        let mut k = bits.len() / 2;
        let word_count = k.div_ceil(64);
        let mut m4_words = vec![0u64; word_count];
        let mut m6_words = vec![0u64; word_count];

        for i in 0..k {
            let word_idx = i / 64;
            let bit_idx = i % 64;
            m6_words[word_idx] |= (bits[2 * i] as u64) << bit_idx;
            m4_words[word_idx] |= (bits[2 * i + 1] as u64) << bit_idx;
        }

        // MSB側の (0,0) トリミング
        while k > 1 {
            let word_idx = (k - 1) / 64;
            let bit_idx = (k - 1) % 64;
            let m4_top = (m4_words[word_idx] >> bit_idx) & 1;
            let m6_top = (m6_words[word_idx] >> bit_idx) & 1;
            if m4_top == 0 && m6_top == 0 {
                // MSBのビットをクリア（すでに0なのでワードの操作は不要）
                k -= 1;
            } else {
                break;
            }
        }

        // ワード数を再調整
        let new_word_count = k.div_ceil(64);
        m4_words.truncate(new_word_count);
        m6_words.truncate(new_word_count);

        // 最上位ワードの余剰ビットをマスク
        let remainder = k % 64;
        if remainder > 0 && new_word_count > 0 {
            let mask = (1u64 << remainder) - 1;
            m4_words[new_word_count - 1] &= mask;
            m6_words[new_word_count - 1] &= mask;
        }

        PairNumber { m4_words, m6_words, pair_count: k }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::One;

    #[test]
    fn test_roundtrip_small() {
        for n in 0u64..=200 {
            let big = BigUint::from(n);
            let pair = PairNumber::from_biguint(&big);
            let back = pair.to_biguint();
            assert_eq!(big, back, "Roundtrip failed for n={}", n);
        }
    }

    #[test]
    fn test_27_decomposition() {
        // n=27 = 11011₂ → パディング: 011011₂
        // LSB first: 1,1,0,1,1,0
        // ペア: (b[0]=1,a[0]=1), (b[1]=0,a[1]=1), (b[2]=1,a[2]=0)
        let pair = PairNumber::from_biguint(&BigUint::from(27u64));
        assert_eq!(pair.m4_as_vec_u8(), vec![1, 1, 0]); // a = [1, 1, 0]
        assert_eq!(pair.m6_as_vec_u8(), vec![1, 0, 1]); // b = [1, 0, 1]
        assert_eq!(pair.pair_count(), 3);
    }

    #[test]
    fn test_one() {
        let pair = PairNumber::from_biguint(&BigUint::one());
        assert!(pair.is_one());
        assert_eq!(pair.m4_as_vec_u8(), vec![0]);
        assert_eq!(pair.m6_as_vec_u8(), vec![1]);
    }

    #[test]
    fn test_bits_lsb_roundtrip() {
        for n in 1u64..=200 {
            let big = BigUint::from(n);
            let pair = PairNumber::from_biguint(&big);
            let bits = pair.to_bits_lsb();
            let pair2 = PairNumber::from_bits_lsb(&bits);
            assert_eq!(pair, pair2, "Bits roundtrip failed for n={}", n);
        }
    }

    #[test]
    fn test_get_m4_m6() {
        let pair = PairNumber::from_biguint(&BigUint::from(27u64));
        assert_eq!(pair.get_m4(0), 1);
        assert_eq!(pair.get_m4(1), 1);
        assert_eq!(pair.get_m4(2), 0);
        assert_eq!(pair.get_m4(-1), 0);
        assert_eq!(pair.get_m4(3), 0);
        assert_eq!(pair.get_m6(0), 1);
        assert_eq!(pair.get_m6(1), 0);
        assert_eq!(pair.get_m6(2), 1);
    }

    #[test]
    fn test_from_packed() {
        let pair = PairNumber::from_packed(vec![0b110], vec![0b101], 3);
        assert_eq!(pair.get_m4(0), 0);
        assert_eq!(pair.get_m4(1), 1);
        assert_eq!(pair.get_m4(2), 1);
        assert_eq!(pair.get_m6(0), 1);
        assert_eq!(pair.get_m6(1), 0);
        assert_eq!(pair.get_m6(2), 1);
    }

    #[test]
    fn test_large_roundtrip() {
        // 2^100 - 1
        let n = (BigUint::one() << 100u32) - BigUint::one();
        let pair = PairNumber::from_biguint(&n);
        let back = pair.to_biguint();
        assert_eq!(n, back, "Large roundtrip failed");
        assert_eq!(pair.pair_count(), 50);
    }

    #[test]
    fn test_ord_exhaustive_small() {
        // 0..=200 の全ペアで、BigUint比較とPairNumber比較が一致することを確認
        for a in 0u64..=200 {
            let pa = PairNumber::from_biguint(&BigUint::from(a));
            for b in 0u64..=200 {
                let pb = PairNumber::from_biguint(&BigUint::from(b));
                assert_eq!(
                    pa.cmp(&pb), a.cmp(&b),
                    "Ord mismatch: a={}, b={}", a, b
                );
            }
        }
    }

    #[test]
    fn test_ord_different_pair_count() {
        // 3 (pair_count=1) vs 7 (pair_count=2)
        let p3 = PairNumber::from_biguint(&BigUint::from(3u64));
        let p7 = PairNumber::from_biguint(&BigUint::from(7u64));
        assert!(p3 < p7);
    }

    #[test]
    fn test_ord_large() {
        let a = (BigUint::one() << 100u32) - BigUint::one();
        let b = BigUint::one() << 100u32;
        let pa = PairNumber::from_biguint(&a);
        let pb = PairNumber::from_biguint(&b);
        assert!(pa < pb);
        assert!(pb > pa);
        assert_eq!(pa, pa.clone());
    }
}
