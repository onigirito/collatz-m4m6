use crate::pair_number::PairNumber;

/// 参照パターン（表3.1）の実装。
/// xn+1 のペア加算における参照ビットを計算する。
///
/// s = log₂(x-1), t = ⌊s/2⌋
///
/// s偶数: ref_R(i) = (b[i-t], b[i]),  ref_L(i) = (a[i-t], a[i])
/// s奇数: ref_R(i) = (a[i-t-1], b[i]), ref_L(i) = (b[i-t], a[i])
/// 参照パターンのパラメータ
pub struct RefPattern {
    pub s: u32,
    pub t: isize,
    pub s_is_even: bool,
}

impl RefPattern {
    /// x から参照パターンのパラメータを計算。
    /// x-1 は2の冪であること。
    pub fn new(x: u64) -> Self {
        assert!(x >= 3, "x must be >= 3");
        let xm1 = x - 1;
        assert!(xm1.is_power_of_two(), "x-1 must be a power of 2");
        let s = xm1.trailing_zeros();
        let t = (s / 2) as isize;
        RefPattern {
            s,
            t,
            s_is_even: s.is_multiple_of(2),
        }
    }

    /// ペア位置 i での m6段の参照ビットペア (ref_bit, current_b) を返す
    #[inline]
    pub fn ref_r(&self, n: &PairNumber, i: isize, bi: u8) -> (u8, u8) {
        if self.s_is_even {
            // ref_R(i) = (b[i-t], b[i])
            (n.get_m6(i - self.t), bi)
        } else {
            // ref_R(i) = (a[i-t-1], b[i])
            (n.get_m4(i - self.t - 1), bi)
        }
    }

    /// ペア位置 i での m4段の参照ビットペア (ref_bit, current_a) を返す
    #[inline]
    pub fn ref_l(&self, n: &PairNumber, i: isize, ai: u8) -> (u8, u8) {
        if self.s_is_even {
            // ref_L(i) = (a[i-t], a[i])
            (n.get_m4(i - self.t), ai)
        } else {
            // ref_L(i) = (b[i-t], a[i])
            (n.get_m6(i - self.t), ai)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_pattern_x3() {
        let rp = RefPattern::new(3);
        assert_eq!(rp.s, 1);
        assert_eq!(rp.t, 0);
        assert!(!rp.s_is_even);
    }

    #[test]
    fn test_ref_pattern_x5() {
        let rp = RefPattern::new(5);
        assert_eq!(rp.s, 2);
        assert_eq!(rp.t, 1);
        assert!(rp.s_is_even);
    }

    #[test]
    fn test_ref_pattern_x9() {
        let rp = RefPattern::new(9);
        assert_eq!(rp.s, 3);
        assert_eq!(rp.t, 1);
        assert!(!rp.s_is_even);
    }

    #[test]
    fn test_ref_pattern_x17() {
        let rp = RefPattern::new(17);
        assert_eq!(rp.s, 4);
        assert_eq!(rp.t, 2);
        assert!(rp.s_is_even);
    }
}
