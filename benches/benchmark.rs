use criterion::{black_box, criterion_group, criterion_main, Criterion};
use collatz_m4m6::*;
use num_bigint::BigUint;
use num_traits::One;

fn bench_step_3n1_small(c: &mut Criterion) {
    let n = BigUint::from(27u64);
    let pair = PairNumber::from_biguint(&n);

    c.bench_function("3n+1 step n=27", |b| {
        b.iter(|| collatz_step_3n1(black_box(&pair)))
    });
}

fn bench_step_3n1_medium(c: &mut Criterion) {
    let n = (BigUint::one() << 1000u32) - BigUint::one();
    let pair = PairNumber::from_biguint(&n);

    c.bench_function("3n+1 step 2^1000-1", |b| {
        b.iter(|| collatz_step_3n1(black_box(&pair)))
    });
}

fn bench_step_3n1_large(c: &mut Criterion) {
    let n = (BigUint::one() << 10000u32) - BigUint::one();
    let pair = PairNumber::from_biguint(&n);

    c.bench_function("3n+1 step 2^10000-1", |b| {
        b.iter(|| collatz_step_3n1(black_box(&pair)))
    });
}

fn bench_step_generic_x5(c: &mut Criterion) {
    let n = (BigUint::one() << 1000u32) - BigUint::one();
    let pair = PairNumber::from_biguint(&n);

    c.bench_function("5n+1 step 2^1000-1", |b| {
        b.iter(|| collatz_step_5n1(black_box(&pair)))
    });
}

fn bench_conversion_roundtrip(c: &mut Criterion) {
    let n = (BigUint::one() << 1000u32) - BigUint::one();

    c.bench_function("BigUint->PairNumber->BigUint 2^1000-1", |b| {
        b.iter(|| {
            let pair = PairNumber::from_biguint(black_box(&n));
            pair.to_biguint()
        })
    });
}

fn bench_trajectory_27(c: &mut Criterion) {
    let start = BigUint::from(27u64);

    c.bench_function("trajectory 27->1 (3n+1)", |b| {
        b.iter(|| trace_trajectory(black_box(&start), 3, 200))
    });
}

criterion_group!(
    benches,
    bench_step_3n1_small,
    bench_step_3n1_medium,
    bench_step_3n1_large,
    bench_step_generic_x5,
    bench_conversion_roundtrip,
    bench_trajectory_27,
);
criterion_main!(benches);
