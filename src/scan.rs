use crate::pair_number::PairNumber;
use crate::postprocess;
use crate::reference::RefPattern;

/// GPK 分類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Gpk {
    Kill = 0,
    Propagate = 1,
    Generate = 2,
}

/// 1ステップの GPK 情報
#[derive(Debug, Clone)]
pub struct GpkInfo {
    /// 各ペアの GPK マスク（パックド）: ビット i = 1 ならそのペアが G
    pub g_masks: Vec<u64>,
    /// 各ペアの GPK マスク（パックド）: ビット i = 1 ならそのペアが P
    pub p_masks: Vec<u64>,
    /// 有効ペア数
    pub active_pairs: usize,
    /// G の数
    pub g_count: u32,
    /// P の数
    pub p_count: u32,
    /// K の数
    pub k_count: u32,
    /// 最大キャリー伝播距離（キャリーが生存する連続ペア数の最大値）
    pub max_carry_chain: u32,
}

impl GpkInfo {
    fn new(pair_count: usize) -> Self {
        let word_count = (pair_count + 63) / 64;
        GpkInfo {
            g_masks: vec![0u64; word_count],
            p_masks: vec![0u64; word_count],
            active_pairs: pair_count,
            g_count: 0,
            p_count: 0,
            k_count: 0,
            max_carry_chain: 0,
        }
    }

    #[inline]
    fn set_gpk(&mut self, i: usize, gpk: Gpk) {
        let word_idx = i / 64;
        let bit_idx = i % 64;
        match gpk {
            Gpk::Generate => {
                self.g_count += 1;
                self.g_masks[word_idx] |= 1u64 << bit_idx;
            }
            Gpk::Propagate => {
                self.p_count += 1;
                self.p_masks[word_idx] |= 1u64 << bit_idx;
            }
            Gpk::Kill => {
                self.k_count += 1;
            }
        }
    }

    /// キャリー伝播距離を計算。初期キャリー c=1 からの連鎖と、
    /// G で再生成された後の連鎖の最大長を求める。
    fn finalize(&mut self) {
        let mut chain = 0u32;
        let mut max_chain = 0u32;
        let mut carry = true; // 初期キャリー = 1

        for i in 0..self.active_pairs {
            let word_idx = i / 64;
            let bit_idx = i % 64;
            let is_g = (self.g_masks[word_idx] >> bit_idx) & 1 != 0;
            let is_p = (self.p_masks[word_idx] >> bit_idx) & 1 != 0;

            if is_g {
                chain += 1;
                carry = true;
            } else if is_p {
                if carry {
                    chain += 1;
                }
            } else {
                // Kill
                if chain > max_chain {
                    max_chain = chain;
                }
                chain = 0;
                carry = false;
            }
        }
        if chain > max_chain {
            max_chain = chain;
        }
        self.max_carry_chain = max_chain;
    }

    /// GPK列を文字列で取得（表示用、limit文字まで）
    pub fn gpk_string(&self, limit: usize) -> String {
        let len = self.active_pairs.min(limit);
        let mut s = String::with_capacity(len);
        for i in 0..len {
            let word_idx = i / 64;
            let bit_idx = i % 64;
            let is_g = (self.g_masks[word_idx] >> bit_idx) & 1 != 0;
            let is_p = (self.p_masks[word_idx] >> bit_idx) & 1 != 0;
            s.push(if is_g { 'G' } else if is_p { 'P' } else { 'K' });
        }
        if self.active_pairs > limit {
            s.push_str("...");
        }
        s
    }

    /// Vec<Gpk> をオンデマンド生成（テスト互換）
    pub fn to_seq(&self) -> Vec<Gpk> {
        let mut seq = Vec::with_capacity(self.active_pairs);
        for i in 0..self.active_pairs {
            let word_idx = i / 64;
            let bit_idx = i % 64;
            let is_g = (self.g_masks[word_idx] >> bit_idx) & 1 != 0;
            let is_p = (self.p_masks[word_idx] >> bit_idx) & 1 != 0;
            if is_g {
                seq.push(Gpk::Generate);
            } else if is_p {
                seq.push(Gpk::Propagate);
            } else {
                seq.push(Gpk::Kill);
            }
        }
        seq
    }
}

/// 1ステップの計算結果
#[derive(Debug, Clone)]
pub struct StepResult {
    /// 次の奇数 n'
    pub next: PairNumber,
    /// 末尾ゼロ数 d（÷2 の回数）
    pub d: u64,
    /// 交換が発生したか (d が奇数)
    pub exchanged: bool,
    /// GPK 分類情報
    pub gpk: GpkInfo,
    /// postprocess前の偶数状態 xn+1 の m4/m6（トレース用）
    pub raw_m4: Vec<u64>,
    pub raw_m6: Vec<u64>,
    pub raw_pair_count: usize,
}

/// GPK 統計情報（メモリ上集約用、verify で使用）
#[derive(Debug, Clone)]
pub struct GpkStats {
    /// G の総数
    pub total_g: u64,
    /// P の総数
    pub total_p: u64,
    /// K の総数
    pub total_k: u64,
    /// 処理したペアの総数
    pub total_pairs: u64,
    /// 処理したステップの総数
    pub total_steps: u64,
    /// 最大キャリー伝播距離のヒストグラム (index=距離, value=出現回数)
    pub carry_chain_hist: [u64; 128],
}

impl GpkStats {
    pub fn new() -> Self {
        GpkStats {
            total_g: 0,
            total_p: 0,
            total_k: 0,
            total_pairs: 0,
            total_steps: 0,
            carry_chain_hist: [0u64; 128],
        }
    }

    /// 1ステップの GPK 情報を集約
    #[inline]
    pub fn accumulate(&mut self, info: &GpkInfo) {
        self.total_g += info.g_count as u64;
        self.total_p += info.p_count as u64;
        self.total_k += info.k_count as u64;
        self.total_pairs += info.active_pairs as u64;
        self.total_steps += 1;
        let idx = (info.max_carry_chain as usize).min(127);
        self.carry_chain_hist[idx] += 1;
    }

    /// 並列処理用: 他の GpkStats をマージ
    pub fn merge(&mut self, other: &GpkStats) {
        self.total_g += other.total_g;
        self.total_p += other.total_p;
        self.total_k += other.total_k;
        self.total_pairs += other.total_pairs;
        self.total_steps += other.total_steps;
        for i in 0..128 {
            self.carry_chain_hist[i] += other.carry_chain_hist[i];
        }
    }
}

/// 参照ビットペアからペア GPK を計算
#[inline]
fn pair_gpk(p_r: u8, q_r: u8, p_l: u8, q_l: u8) -> Gpk {
    // m6段 GPK
    let g_mid = p_r & q_r;
    let p_mid = p_r ^ q_r;
    // m4段 GPK
    let g_out = p_l & q_l;
    let p_out = p_l ^ q_l;
    // 直列合成
    let g_i = g_out | (p_out & g_mid);
    let p_i = p_out & p_mid;

    if g_i != 0 {
        Gpk::Generate
    } else if p_i != 0 {
        Gpk::Propagate
    } else {
        Gpk::Kill
    }
}

/// 汎用 collatz_step: T(n) = (xn+1) / 2^d
/// x は x-1 が2の冪であること。x ∈ {3, 5, 9, 17, ...}
/// n は奇数であること。
pub fn collatz_step(n: &PairNumber, x: u64) -> StepResult {
    let rp = RefPattern::new(x);
    let k = n.pair_count();

    // オーバーフロー分を含む最大インデックス
    let max_i = k + ((rp.s as usize + 1) / 2);

    let out_pair_count = max_i + 1;
    let out_word_count = (out_pair_count + 63) / 64;
    let mut new_m4 = vec![0u64; out_word_count];
    let mut new_m6 = vec![0u64; out_word_count];
    let mut gpk_info = GpkInfo::new(k);
    let mut c: u8 = 1; // 初期キャリー = 1 (+1 の効果)

    let mut actual_pairs = 0usize;

    for i in 0..=max_i {
        let ii = i as isize;
        let ai = n.get_m4(ii);
        let bi = n.get_m6(ii);

        // 参照パターンに基づくビット取得
        let (p_r, q_r) = rp.ref_r(n, ii, bi);
        let (p_l, q_l) = rp.ref_l(n, ii, ai);

        // GPK 分類（有効ペア範囲のみ記録）
        if i < k {
            gpk_info.set_gpk(i, pair_gpk(p_r, q_r, p_l, q_l));
        }

        // m6段
        let sum_r = p_r + q_r + c;
        let m6_bit = (sum_r & 1) as u64;
        let c_mid = sum_r >> 1;

        // m4段
        let sum_l = p_l + q_l + c_mid;
        let m4_bit = (sum_l & 1) as u64;
        c = sum_l >> 1;

        let word_idx = i / 64;
        let bit_idx = i % 64;
        new_m6[word_idx] |= m6_bit << bit_idx;
        new_m4[word_idx] |= m4_bit << bit_idx;
        actual_pairs = i + 1;

        // 早期終了: キャリー消滅 かつ 参照パターンの後方参照も範囲外
        let safe_end = k + (rp.s as usize).saturating_sub(1) / 2;
        if c == 0 && i >= safe_end {
            break;
        }
    }

    gpk_info.finalize();

    // 偶数状態を保存（postprocess前）
    let raw_m4 = new_m4.clone();
    let raw_m6 = new_m6.clone();
    let raw_pair_count = actual_pairs;

    // 後処理: MSBトリミング → 末尾ゼロ計数 → 右シフト → 再ペア化
    let pp = postprocess::postprocess(new_m4, new_m6, actual_pairs);
    StepResult {
        next: pp.next,
        d: pp.d,
        exchanged: pp.exchanged,
        gpk: gpk_info,
        raw_m4,
        raw_m6,
        raw_pair_count,
    }
}

/// x=3 専用の最適化版。
/// s=1, t=0, s奇数。
/// ref_R(i) = (a[i-1], b[i])
/// ref_L(i) = (b[i], a[i])  ← 現ペアそのもの
pub fn collatz_step_3n1(n: &PairNumber) -> StepResult {
    let k = n.pair_count();
    let max_i = k + 1;

    let out_pair_count = max_i + 1;
    let out_word_count = (out_pair_count + 63) / 64;
    let mut new_m4 = vec![0u64; out_word_count];
    let mut new_m6 = vec![0u64; out_word_count];
    let mut gpk_info = GpkInfo::new(k);
    let mut c: u8 = 1;

    let mut actual_pairs = 0usize;

    for i in 0..=max_i {
        let ai = n.get_m4(i as isize);
        let bi = n.get_m6(i as isize);
        let a_prev = n.get_m4(i as isize - 1);

        // GPK: ref_R=(a_prev, bi), ref_L=(bi, ai)
        if i < k {
            gpk_info.set_gpk(i, pair_gpk(a_prev, bi, bi, ai));
        }

        // m6段: a[i-1] + b[i] + c
        let sum_r = a_prev + bi + c;
        let m6_bit = (sum_r & 1) as u64;
        let c_mid = sum_r >> 1;

        // m4段: b[i] + a[i] + c_mid
        let sum_l = bi + ai + c_mid;
        let m4_bit = (sum_l & 1) as u64;
        c = sum_l >> 1;

        let word_idx = i / 64;
        let bit_idx = i % 64;
        new_m6[word_idx] |= m6_bit << bit_idx;
        new_m4[word_idx] |= m4_bit << bit_idx;
        actual_pairs = i + 1;

        if c == 0 && i >= k {
            break;
        }
    }

    gpk_info.finalize();

    let raw_m4 = new_m4.clone();
    let raw_m6 = new_m6.clone();
    let raw_pair_count = actual_pairs;

    let pp = postprocess::postprocess(new_m4, new_m6, actual_pairs);
    StepResult {
        next: pp.next,
        d: pp.d,
        exchanged: pp.exchanged,
        gpk: gpk_info,
        raw_m4,
        raw_m6,
        raw_pair_count,
    }
}

/// x=5 専用の最適化版。
/// s=2, t=1, s偶数。
/// ref_R(i) = (b[i-1], b[i])
/// ref_L(i) = (a[i-1], a[i])
pub fn collatz_step_5n1(n: &PairNumber) -> StepResult {
    let k = n.pair_count();
    let max_i = k + 1;

    let out_pair_count = max_i + 1;
    let out_word_count = (out_pair_count + 63) / 64;
    let mut new_m4 = vec![0u64; out_word_count];
    let mut new_m6 = vec![0u64; out_word_count];
    let mut gpk_info = GpkInfo::new(k);
    let mut c: u8 = 1;

    let mut actual_pairs = 0usize;

    for i in 0..=max_i {
        let ai = n.get_m4(i as isize);
        let bi = n.get_m6(i as isize);
        let b_prev = n.get_m6(i as isize - 1);
        let a_prev = n.get_m4(i as isize - 1);

        // GPK: ref_R=(b_prev, bi), ref_L=(a_prev, ai)
        if i < k {
            gpk_info.set_gpk(i, pair_gpk(b_prev, bi, a_prev, ai));
        }

        // m6段: b[i-1] + b[i] + c
        let sum_r = b_prev + bi + c;
        let m6_bit = (sum_r & 1) as u64;
        let c_mid = sum_r >> 1;

        // m4段: a[i-1] + a[i] + c_mid
        let sum_l = a_prev + ai + c_mid;
        let m4_bit = (sum_l & 1) as u64;
        c = sum_l >> 1;

        let word_idx = i / 64;
        let bit_idx = i % 64;
        new_m6[word_idx] |= m6_bit << bit_idx;
        new_m4[word_idx] |= m4_bit << bit_idx;
        actual_pairs = i + 1;

        if c == 0 && i >= k {
            break;
        }
    }

    gpk_info.finalize();

    let raw_m4 = new_m4.clone();
    let raw_m6 = new_m6.clone();
    let raw_pair_count = actual_pairs;

    let pp = postprocess::postprocess(new_m4, new_m6, actual_pairs);
    StepResult {
        next: pp.next,
        d: pp.d,
        exchanged: pp.exchanged,
        gpk: gpk_info,
        raw_m4,
        raw_m6,
        raw_pair_count,
    }
}
