use collatz_m4m6::*;
use num_bigint::BigUint;
use num_traits::One;

/// 大数の算術検証ヘルパー
fn verify_large_step(n: &BigUint, x: u64) {
    let pair = PairNumber::from_biguint(n);
    let result = collatz_step(&pair, x);

    // 算術計算
    let xn1 = n * BigUint::from(x) + BigUint::one();
    let d_arith = xn1.trailing_zeros().unwrap_or(0);
    let n_next_arith = &xn1 >> d_arith;

    let n_prime = result.next.to_biguint();

    assert_eq!(result.d, d_arith, "d mismatch for large n, x={}", x);
    assert_eq!(n_prime, n_next_arith, "n' mismatch for large n, x={}", x);
}

/// 2^100 - 1 (約30桁) のテスト
#[test]
fn test_2pow100_minus1_3n1() {
    let n = (BigUint::one() << 100u32) - BigUint::one();
    verify_large_step(&n, 3);
}

#[test]
fn test_2pow100_minus1_5n1() {
    let n = (BigUint::one() << 100u32) - BigUint::one();
    verify_large_step(&n, 5);
}

#[test]
fn test_2pow100_minus1_9n1() {
    let n = (BigUint::one() << 100u32) - BigUint::one();
    verify_large_step(&n, 9);
}

/// 2^1000 - 1 (約301桁) のテスト
#[test]
fn test_2pow1000_minus1_3n1() {
    let n = (BigUint::one() << 1000u32) - BigUint::one();
    verify_large_step(&n, 3);
}

#[test]
fn test_2pow1000_minus1_5n1() {
    let n = (BigUint::one() << 1000u32) - BigUint::one();
    verify_large_step(&n, 5);
}

/// 2^10000 - 1 (約3010桁) のテスト
#[test]
fn test_2pow10000_minus1_3n1() {
    let n = (BigUint::one() << 10000u32) - BigUint::one();
    verify_large_step(&n, 3);
}

#[test]
fn test_2pow10000_minus1_5n1() {
    let n = (BigUint::one() << 10000u32) - BigUint::one();
    verify_large_step(&n, 5);
}

/// 2^100000 - 1 (約30103桁) のテスト — 仕様の10万桁対応範囲に近い
#[test]
fn test_2pow100000_minus1_3n1() {
    let n = (BigUint::one() << 100000u32) - BigUint::one();
    verify_large_step(&n, 3);
}

/// 3n+1 専用版と汎用版の大数での一致
#[test]
fn test_3n1_special_vs_generic_large() {
    let n = (BigUint::one() << 1000u32) - BigUint::one();
    let pair = PairNumber::from_biguint(&n);

    let r_generic = collatz_step(&pair, 3);
    let r_special = collatz_step_3n1(&pair);

    assert_eq!(r_generic.d, r_special.d);
    assert_eq!(
        r_generic.next.to_biguint(),
        r_special.next.to_biguint()
    );
}

/// 5n+1 専用版と汎用版の大数での一致
#[test]
fn test_5n1_special_vs_generic_large() {
    let n = (BigUint::one() << 1000u32) - BigUint::one();
    let pair = PairNumber::from_biguint(&n);

    let r_generic = collatz_step(&pair, 5);
    let r_special = collatz_step_5n1(&pair);

    assert_eq!(r_generic.d, r_special.d);
    assert_eq!(
        r_generic.next.to_biguint(),
        r_special.next.to_biguint()
    );
}

/// 連続ステップの大数テスト（10ステップ分の算術検証）
#[test]
fn test_large_multi_step_3n1() {
    let mut n = (BigUint::one() << 100u32) - BigUint::one();

    for step in 0..10 {
        let pair = PairNumber::from_biguint(&n);
        let result = collatz_step_3n1(&pair);

        // 算術検証
        let xn1 = &n * BigUint::from(3u64) + BigUint::one();
        let d_arith = xn1.trailing_zeros().unwrap_or(0);
        let n_next_arith = &xn1 >> d_arith;

        let n_prime = result.next.to_biguint();
        assert_eq!(result.d, d_arith, "d mismatch at step {}", step);
        assert_eq!(n_prime, n_next_arith, "n' mismatch at step {}", step);

        n = n_prime;
    }
}

/// PairNumber のまま回す連続ステップ（変換なしの内部表現テスト）
#[test]
fn test_internal_multi_step_3n1() {
    let start = (BigUint::one() << 100u32) - BigUint::one();
    let mut pair = PairNumber::from_biguint(&start);
    let mut arith_n = start;

    for step in 0..20 {
        let result = collatz_step_3n1(&pair);

        // 算術検証
        let xn1 = &arith_n * BigUint::from(3u64) + BigUint::one();
        let d_arith = xn1.trailing_zeros().unwrap_or(0);
        let n_next_arith = &xn1 >> d_arith;

        assert_eq!(result.d, d_arith, "d mismatch at step {}", step);
        assert_eq!(
            result.next.to_biguint(),
            n_next_arith,
            "n' mismatch at step {}",
            step
        );

        pair = result.next;
        arith_n = n_next_arith;
    }
}

/// 範囲検証のテスト（小範囲）
#[test]
fn test_verify_range_small() {
    let start = BigUint::from(3u64);
    let end = BigUint::from(99u64);

    let result = verify_range(&start, &end, 3, 10000, |_, _| {});

    assert!(result.all_converged);
    assert!(result.failures.is_empty());
    assert!(result.total_checked > 0);
}

/// ベンチマーク: パックドscan vs BigUint古典演算（5n+1, 3-999, 1000ステップ）
/// `cargo test --release bench_packed_vs_biguint -- --nocapture` で実行
#[test]
fn bench_packed_vs_biguint() {
    use std::time::Instant;

    let x = 5u64;
    let max_steps = 5000u64;
    let range_end = 999u64;
    let max_bits = 20000usize; // パックドscanの MAX_PAIR_COUNT=10000 に合わせる

    // まず BigUint古典演算で各数のステップ数を記録（正解データ）
    let mut expected: Vec<(u64, u64)> = Vec::new(); // (n, steps)
    for n in (3..=range_end).step_by(2) {
        let initial = BigUint::from(n);
        let mut current = initial.clone();
        let mut steps = 0u64;
        let mut converged = false;
        while steps < max_steps {
            let xn1 = &current * BigUint::from(x) + BigUint::one();
            let d = xn1.trailing_zeros().unwrap_or(0);
            current = &xn1 >> d;
            steps += 1;
            if current == BigUint::one() || current < initial {
                converged = true;
                break;
            }
            if current.bits() as usize > max_bits {
                break;
            }
        }
        expected.push((n, if converged { steps } else { 0 }));
    }

    // パックドscan で同じ数を処理
    let mut packed_results: Vec<(u64, u64)> = Vec::new();
    for n in (3..=range_end).step_by(2) {
        let st = stopping_time_u64_fast(n, x, max_steps, None, false, true);
        packed_results.push((n, st.unwrap_or(0)));
    }

    // 結果一致を確認
    for (e, p) in expected.iter().zip(packed_results.iter()) {
        assert_eq!(e, p, "Mismatch for n={}: expected steps={}, got={}", e.0, e.1, p.1);
    }

    // --- タイミング計測 ---
    // BigUint古典演算
    let t0 = Instant::now();
    for n in (3..=range_end).step_by(2) {
        let initial = BigUint::from(n);
        let mut current = initial.clone();
        let mut steps = 0u64;
        while steps < max_steps {
            let xn1 = &current * BigUint::from(x) + BigUint::one();
            let d = xn1.trailing_zeros().unwrap_or(0);
            current = &xn1 >> d;
            steps += 1;
            if current == BigUint::one() || current < initial {
                break;
            }
            if current.bits() as usize > max_bits {
                break;
            }
        }
    }
    let biguint_elapsed = t0.elapsed();

    // パックドscan（Phase1 OFF）
    let t1 = Instant::now();
    for n in (3..=range_end).step_by(2) {
        stopping_time_u64_fast(n, x, max_steps, None, false, true);
    }
    let packed_elapsed = t1.elapsed();

    let total_steps: u64 = expected.iter().map(|(_, s)| *s).sum();
    eprintln!("\n=== 5n+1 bench (3-{}, {}steps, GPK OFF) ===", range_end, max_steps);
    eprintln!("Total converged steps: {}", total_steps);
    eprintln!("BigUint classical: {:.3}s", biguint_elapsed.as_secs_f64());
    eprintln!("Packed scan      : {:.3}s", packed_elapsed.as_secs_f64());
    if packed_elapsed.as_secs_f64() > 0.0 {
        eprintln!("BigUint/Packed = {:.2}x", biguint_elapsed.as_secs_f64() / packed_elapsed.as_secs_f64());
    }
}
