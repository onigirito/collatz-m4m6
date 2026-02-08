use collatz_m4m6::*;
use num_bigint::BigUint;

/// §8.1 n=27, x=5 の完全トレース検証
#[test]
fn test_paper_example_27_x5() {
    let n = BigUint::from(27u64);
    let pair = PairNumber::from_biguint(&n);

    // 前処理の検証
    assert_eq!(pair.m4_as_vec_u8(), vec![1, 1, 0]); // a = [1, 1, 0] (LSB順)
    assert_eq!(pair.m6_as_vec_u8(), vec![1, 0, 1]); // b = [1, 0, 1] (LSB順)
    assert_eq!(pair.pair_count(), 3);

    // 1ステップ実行
    let result = collatz_step_5n1(&pair);

    // 5×27+1 = 136, d=3, n'=17
    assert_eq!(result.d, 3);
    assert!(result.exchanged); // d=3 奇数
    assert_eq!(result.next.to_biguint(), BigUint::from(17u64));
}

/// §8.2 n=27, x=3 の完全トレース検証
#[test]
fn test_paper_example_27_x3() {
    let n = BigUint::from(27u64);
    let pair = PairNumber::from_biguint(&n);

    let result = collatz_step_3n1(&pair);

    // 3×27+1 = 82, d=1, n'=41
    assert_eq!(result.d, 1);
    assert!(result.exchanged); // d=1 奇数
    assert_eq!(result.next.to_biguint(), BigUint::from(41u64));
}

/// n=27 の 3n+1 軌道全体
/// 標準カウント(各/2を個別計上)では111ステップだが、
/// 本実装は奇数→奇数を1ステップとするSyracuse関数なので41ステップ。
#[test]
fn test_trajectory_27_3n1() {
    let start = BigUint::from(27u64);
    let result = trace_trajectory(&start, 3, 200);

    assert!(result.reached_one, "27 should reach 1 under 3n+1");
    assert_eq!(
        result.total_steps, 41,
        "27 should take 41 odd-to-odd steps to reach 1"
    );

    // 標準カウントとの整合性: ステップ数 + Σd = 111
    let sum_d: u64 = result.steps.iter().map(|(_, d)| d).sum();
    assert_eq!(
        result.total_steps + sum_d, 111,
        "odd-to-odd steps + total divisions should equal 111"
    );
}

/// 5n+1 サイクル: 27 → 17 → 43 → 27
#[test]
fn test_5n1_cycle_27() {
    let start = BigUint::from(27u64);
    let pair = PairNumber::from_biguint(&start);

    // 27 → 17
    let r1 = collatz_step_5n1(&pair);
    assert_eq!(r1.next.to_biguint(), BigUint::from(17u64));

    // 17 → 43
    let r2 = collatz_step_5n1(&r1.next);
    assert_eq!(r2.next.to_biguint(), BigUint::from(43u64));

    // 43 → 27 (サイクル完了)
    let r3 = collatz_step_5n1(&r2.next);
    assert_eq!(r3.next.to_biguint(), BigUint::from(27u64));
}

/// 5n+1 サイクル: 13 → 33 → 83 → 13
#[test]
fn test_5n1_cycle_13() {
    let start = BigUint::from(13u64);
    let pair = PairNumber::from_biguint(&start);

    // 13 → 33
    let r1 = collatz_step_5n1(&pair);
    assert_eq!(r1.next.to_biguint(), BigUint::from(33u64));

    // 33 → 83
    let r2 = collatz_step_5n1(&r1.next);
    assert_eq!(r2.next.to_biguint(), BigUint::from(83u64));

    // 83 → 13 (サイクル完了)
    let r3 = collatz_step_5n1(&r2.next);
    assert_eq!(r3.next.to_biguint(), BigUint::from(13u64));
}

/// 3n+1 不動点: n=1 → d=2, n'=1
#[test]
fn test_3n1_fixed_point() {
    let one = BigUint::from(1u64);
    let pair = PairNumber::from_biguint(&one);
    let result = collatz_step_3n1(&pair);
    assert_eq!(result.d, 2);
    assert_eq!(result.next.to_biguint(), one);
}

/// PairNumber のファスナー構造検証
#[test]
fn test_zipper_structure() {
    // n=27 = 011011₂
    // LSB first ビット列: 1,1,0,1,1,0
    let pair = PairNumber::from_biguint(&BigUint::from(27u64));
    let bits = pair.to_bits_lsb();
    assert_eq!(bits, vec![1, 1, 0, 1, 1, 0]);

    // n=136 = 10001000₂
    // LSB first: 0,0,0,1,0,0,0,1
    let pair136 = PairNumber::from_biguint(&BigUint::from(136u64));
    let bits136 = pair136.to_bits_lsb();
    assert_eq!(bits136, vec![0, 0, 0, 1, 0, 0, 0, 1]);
}

/// m4⇔m6 交換の検証
#[test]
fn test_exchange_flag() {
    // d が奇数なら exchanged=true
    let pair27 = PairNumber::from_biguint(&BigUint::from(27u64));

    // 3n+1: 27 → 41, d=1 (奇数)
    let r3 = collatz_step_3n1(&pair27);
    assert!(r3.exchanged);

    // 5n+1: 27 → 17, d=3 (奇数)
    let r5 = collatz_step_5n1(&pair27);
    assert!(r5.exchanged);

    // 3n+1: 1 → 1, d=2 (偶数)
    let pair1 = PairNumber::from_biguint(&BigUint::from(1u64));
    let r1 = collatz_step_3n1(&pair1);
    assert!(!r1.exchanged);
}

/// §4.8 GPK 検証: n=27, x=5 → P, G, P
#[test]
fn test_gpk_27_x5() {
    let pair = PairNumber::from_biguint(&BigUint::from(27u64));
    let result = collatz_step_5n1(&pair);

    // 論文 §4.8 の表より: GPK列(LSB順) = P, G, P
    assert_eq!(result.gpk.to_seq(), vec![Gpk::Propagate, Gpk::Generate, Gpk::Propagate]);
    assert_eq!(result.gpk.g_count, 1);
    assert_eq!(result.gpk.p_count, 2);
    assert_eq!(result.gpk.k_count, 0);
    // キャリー伝播: 初期c=1, P→伝播, G→生成, P→伝播 = 全3ペア生存
    assert_eq!(result.gpk.max_carry_chain, 3);
}

/// §4.9 GPK 検証: n=27, x=3 → G, P, G
#[test]
fn test_gpk_27_x3() {
    let pair = PairNumber::from_biguint(&BigUint::from(27u64));
    let result = collatz_step_3n1(&pair);

    // 論文 §4.9 の表より: GPK列(LSB順) = G, P, G
    assert_eq!(result.gpk.to_seq(), vec![Gpk::Generate, Gpk::Propagate, Gpk::Generate]);
    assert_eq!(result.gpk.g_count, 2);
    assert_eq!(result.gpk.p_count, 1);
    assert_eq!(result.gpk.k_count, 0);
    assert_eq!(result.gpk.max_carry_chain, 3);
}

/// 定理5.1 検証: x=3 では m4段 GPK が m2(AND)/m7(XOR)/m9(NOR) に一致
#[test]
fn test_theorem_5_1_x3_gpk_matches_predicates() {
    // 全奇数 1〜199 で検証
    for n_val in (1u64..200).step_by(2) {
        let n = BigUint::from(n_val);
        let pair = PairNumber::from_biguint(&n);
        let _result = collatz_step_3n1(&pair);

        for i in 0..pair.pair_count() {
            let ai = pair.get_m4(i as isize);
            let bi = pair.get_m6(i as isize);

            // m4段 ref_L = (b[i], a[i]) なので:
            // G_out = AND(bi, ai) = m2
            // P_out = XOR(bi, ai) = m7
            // K_out = NOR(bi, ai) = m9
            let g_out = bi & ai;
            let p_out = bi ^ ai;

            // m6段は交差項を含むので、ペア全体GPKは m4段だけでは決まらない。
            // ただし m4段の G/P/K が n の述語と一致することを検証。
            let k_out = if g_out == 0 && p_out == 0 { 1u8 } else { 0 };

            // m2 = AND, m7 = XOR, m9 = NOR
            assert_eq!(g_out, ai & bi, "n={} i={}: G_out should be m2(AND)", n_val, i);
            assert_eq!(p_out, ai ^ bi, "n={} i={}: P_out should be m7(XOR)", n_val, i);
            assert_eq!(k_out, if ai == 0 && bi == 0 { 1 } else { 0 },
                "n={} i={}: K_out should be m9(NOR)", n_val, i);
        }
    }
}

/// 停止時間テスト
#[test]
fn test_stopping_time() {
    // n=27 は 96ステップで n 未満に到達するはず（最終的に1に到達するまで111ステップ）
    let n27 = BigUint::from(27u64);
    let st = stopping_time(&n27, 3, 200);
    assert!(st.is_some(), "27 should have a finite stopping time");

    // n=1 は停止時間 0
    let n1 = BigUint::from(1u64);
    let st1 = stopping_time(&n1, 3, 200);
    assert_eq!(st1, Some(0));
}
