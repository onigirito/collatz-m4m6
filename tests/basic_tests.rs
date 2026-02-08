use collatz_m4m6::*;
use num_bigint::BigUint;
use num_traits::One;

/// 算術計算との一致を検証するヘルパー
fn verify_step_arithmetic(n: u64, x: u64) {
    let big_n = BigUint::from(n);
    assert!(n % 2 == 1, "n must be odd");

    let pair = PairNumber::from_biguint(&big_n);
    let result = collatz_step(&pair, x);

    // 算術計算
    let xn1 = &big_n * BigUint::from(x) + BigUint::one();
    let d_arith = xn1.trailing_zeros().unwrap_or(0);
    let n_next_arith = &xn1 >> d_arith;

    let n_prime = result.next.to_biguint();

    assert_eq!(
        result.d, d_arith,
        "d mismatch for n={}, x={}: got {}, expected {}",
        n, x, result.d, d_arith
    );
    assert_eq!(
        n_prime, n_next_arith,
        "n' mismatch for n={}, x={}: got {}, expected {}",
        n, x, n_prime, n_next_arith
    );
}

/// 3n+1 専用版の算術検証
fn verify_step_3n1(n: u64) {
    let big_n = BigUint::from(n);
    let pair = PairNumber::from_biguint(&big_n);
    let result = collatz_step_3n1(&pair);

    let xn1 = &big_n * BigUint::from(3u64) + BigUint::one();
    let d_arith = xn1.trailing_zeros().unwrap_or(0);
    let n_next_arith = &xn1 >> d_arith;

    let n_prime = result.next.to_biguint();

    assert_eq!(result.d, d_arith, "d mismatch for 3n+1, n={}", n);
    assert_eq!(n_prime, n_next_arith, "n' mismatch for 3n+1, n={}", n);
}

/// 5n+1 専用版の算術検証
fn verify_step_5n1(n: u64) {
    let big_n = BigUint::from(n);
    let pair = PairNumber::from_biguint(&big_n);
    let result = collatz_step_5n1(&pair);

    let xn1 = &big_n * BigUint::from(5u64) + BigUint::one();
    let d_arith = xn1.trailing_zeros().unwrap_or(0);
    let n_next_arith = &xn1 >> d_arith;

    let n_prime = result.next.to_biguint();

    assert_eq!(result.d, d_arith, "d mismatch for 5n+1, n={}", n);
    assert_eq!(n_prime, n_next_arith, "n' mismatch for 5n+1, n={}", n);
}

// ===== 3n+1 テスト =====

#[test]
fn test_3n1_specific_cases() {
    // 仕様書 §5.5 のテストケース
    verify_step_3n1(1);  // d=2, n'=1 (不動点)
    verify_step_3n1(3);  // d=1, n'=5
    verify_step_3n1(5);  // d=4, n'=1
    verify_step_3n1(7);  // d=1, n'=11
    verify_step_3n1(27); // d=1, n'=41
}

#[test]
fn test_3n1_d_values() {
    let check = |n: u64, expected_d: u64, expected_next: u64| {
        let pair = PairNumber::from_biguint(&BigUint::from(n));
        let result = collatz_step_3n1(&pair);
        assert_eq!(result.d, expected_d, "d for n={}", n);
        assert_eq!(
            result.next.to_biguint(),
            BigUint::from(expected_next),
            "n' for n={}",
            n
        );
    };

    check(1, 2, 1);
    check(3, 1, 5);
    check(5, 4, 1);
    check(7, 1, 11);
    check(27, 1, 41);
}

#[test]
fn test_3n1_all_odd_1_to_99() {
    // 1〜99 の全奇数で算術検証
    for n in (1u64..=99).step_by(2) {
        verify_step_3n1(n);
    }
}

#[test]
fn test_3n1_generic_match() {
    // 汎用版と専用版の結果が一致することを確認
    for n in (1u64..=99).step_by(2) {
        let big_n = BigUint::from(n);
        let pair = PairNumber::from_biguint(&big_n);

        let r_generic = collatz_step(&pair, 3);
        let r_special = collatz_step_3n1(&pair);

        assert_eq!(r_generic.d, r_special.d, "d mismatch for n={}", n);
        assert_eq!(
            r_generic.next.to_biguint(),
            r_special.next.to_biguint(),
            "n' mismatch for n={}",
            n
        );
    }
}

// ===== 5n+1 テスト =====

#[test]
fn test_5n1_specific_cases() {
    verify_step_5n1(1);  // d=1, n'=3
    verify_step_5n1(3);  // d=4, n'=1
    verify_step_5n1(27); // d=3, n'=17
    verify_step_5n1(13); // d=1, n'=33
}

#[test]
fn test_5n1_d_values() {
    let check = |n: u64, expected_d: u64, expected_next: u64| {
        let pair = PairNumber::from_biguint(&BigUint::from(n));
        let result = collatz_step_5n1(&pair);
        assert_eq!(result.d, expected_d, "d for n={}", n);
        assert_eq!(
            result.next.to_biguint(),
            BigUint::from(expected_next),
            "n' for n={}",
            n
        );
    };

    check(1, 1, 3);
    check(3, 4, 1);
    check(27, 3, 17);
    check(13, 1, 33);
}

#[test]
fn test_5n1_all_odd_1_to_99() {
    for n in (1u64..=99).step_by(2) {
        verify_step_5n1(n);
    }
}

#[test]
fn test_5n1_generic_match() {
    for n in (1u64..=99).step_by(2) {
        let big_n = BigUint::from(n);
        let pair = PairNumber::from_biguint(&big_n);

        let r_generic = collatz_step(&pair, 5);
        let r_special = collatz_step_5n1(&pair);

        assert_eq!(r_generic.d, r_special.d, "d mismatch for n={}", n);
        assert_eq!(
            r_generic.next.to_biguint(),
            r_special.next.to_biguint(),
            "n' mismatch for n={}",
            n
        );
    }
}

// ===== 汎用 x テスト =====

#[test]
fn test_generic_x9() {
    for n in (1u64..=99).step_by(2) {
        verify_step_arithmetic(n, 9);
    }
}

#[test]
fn test_generic_x17() {
    for n in (1u64..=99).step_by(2) {
        verify_step_arithmetic(n, 17);
    }
}

#[test]
fn test_generic_x33() {
    for n in (1u64..=99).step_by(2) {
        verify_step_arithmetic(n, 33);
    }
}

#[test]
fn test_generic_x65() {
    for n in (1u64..=99).step_by(2) {
        verify_step_arithmetic(n, 65);
    }
}

// ===== 広範囲テスト =====

#[test]
fn test_3n1_1_to_999() {
    for n in (1u64..=999).step_by(2) {
        verify_step_3n1(n);
    }
}

#[test]
fn test_5n1_1_to_999() {
    for n in (1u64..=999).step_by(2) {
        verify_step_5n1(n);
    }
}
