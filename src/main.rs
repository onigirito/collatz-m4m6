use collatz_m4m6::*;
use num_bigint::BigUint;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write as IoWrite};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

fn check_avx2() {
    #[cfg(target_arch = "x86_64")]
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("ERROR: This build requires a CPU with AVX2 support (Intel Haswell 2013+ / AMD Excavator 2015+).");
        eprintln!("エラー: このビルドはAVX2対応CPU（Intel第4世代以降 / AMD 2015年以降）が必要です。");
        std::process::exit(1);
    }
}

fn print_usage() {
    eprintln!("コラッツ型写像 m4/m6 走査アルゴリズム (層2: GPK分類付き)");
    eprintln!();
    eprintln!("使い方:");
    eprintln!("  collatz-m4m6 step <n> [x]              1ステップ計算 (デフォルト x=3)");
    eprintln!("  collatz-m4m6 trace <n> [x]             軌道追跡 (1に到達するまで)");
    eprintln!("  collatz-m4m6 verify <start> <end> [x]  範囲検証 (停止時間法)");
    eprintln!();
    eprintln!("結果は自動的に output/ フォルダに保存されます。");
    eprintln!();
    eprintln!("例:");
    eprintln!("  collatz-m4m6 step 27             3*27+1 の1ステップ");
    eprintln!("  collatz-m4m6 step 27 5           5*27+1 の1ステップ");
    eprintln!("  collatz-m4m6 trace 27            27から1までの軌道");
    eprintln!("  collatz-m4m6 verify 3 9999       3〜9999の全奇数を検証");
}

fn output_dir() -> PathBuf {
    let dir = PathBuf::from("output");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let s = now % 60;
    let m = (now / 60) % 60;
    let h = (now / 3600) % 24;
    let days = now / 86400;
    let y = 1970 + days / 365;
    let d = days % 365;
    format!("{:04}{:03}_{:02}{:02}{:02}", y, d, h, m, s)
}

fn short_n(n: &BigUint) -> String {
    let s = n.to_string();
    if s.len() <= 16 {
        s
    } else {
        format!("{}..{}d", &s[..6], s.len())
    }
}

fn gpk_to_str(info: &GpkInfo) -> String {
    info.gpk_string(info.active_pairs)
}

fn enable_ansi() {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::io::IsTerminal;
        let stderr = std::io::stderr();
        if stderr.is_terminal() {
            unsafe {
                let handle = stderr.as_raw_handle();
                let mut mode: u32 = 0;
                extern "system" {
                    fn GetConsoleMode(h: *mut std::ffi::c_void, m: *mut u32) -> i32;
                    fn SetConsoleMode(h: *mut std::ffi::c_void, m: u32) -> i32;
                }
                if GetConsoleMode(handle, &mut mode) != 0 {
                    SetConsoleMode(handle, mode | 0x0004);
                }
            }
        }
    }
}

fn main() {
    check_avx2();
    enable_ansi();
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "step" => cmd_step(&args[2..]),
        "trace" => cmd_trace(&args[2..]),
        "verify" => cmd_verify(&args[2..]),
        _ => {
            eprintln!("不明なコマンド: {}", args[1]);
            print_usage();
        }
    }
}

fn parse_n(s: &str) -> BigUint {
    BigUint::from_str(s).unwrap_or_else(|_| {
        eprintln!("数値を解析できません: {}", s);
        std::process::exit(1);
    })
}

fn parse_x(args: &[String], default: u64) -> u64 {
    if args.is_empty() {
        return default;
    }
    args[0].parse::<u64>().unwrap_or_else(|_| {
        eprintln!("x を解析できません: {}", args[0]);
        std::process::exit(1);
    })
}

fn cmd_step(args: &[String]) {
    if args.is_empty() {
        eprintln!("使い方: collatz-m4m6 step <n> [x]");
        return;
    }

    let n = parse_n(&args[0]);
    let x = parse_x(&args[1..], 3);

    println!("n = {}", n);
    println!("x = {}", x);

    let pair = PairNumber::from_biguint(&n);
    println!("ペア数 k = {}", pair.pair_count());
    let m4_display = pair.m4_as_vec_u8();
    let m6_display = pair.m6_as_vec_u8();
    println!("m4 (LSB順) = {:?}", &m4_display[..pair.pair_count().min(20)]);
    println!("m6 (LSB順) = {:?}", &m6_display[..pair.pair_count().min(20)]);

    let timer = Instant::now();
    let result = collatz_step(&pair, x);
    let elapsed = timer.elapsed();

    let n_prime = result.next.to_biguint();
    println!();
    println!("--- 結果 ---");
    println!("xn+1 = {}*{}+1 = {}", x, n, &n * x + 1u64);
    println!("d (÷2回数) = {}", result.d);
    println!("n' = {}", n_prime);
    println!("m4⇔m6 交換 = {} (d が{})", result.exchanged, if result.d % 2 == 1 { "奇数" } else { "偶数" });

    // GPK 表示
    let gpk_str = gpk_to_str(&result.gpk);
    println!();
    println!("--- GPK (層2) ---");
    println!("GPK列 (LSB順)    = {}", if gpk_str.len() <= 80 { &gpk_str } else { &gpk_str[..80] });
    println!("G (Generate)     = {}", result.gpk.g_count);
    println!("P (Propagate)    = {}", result.gpk.p_count);
    println!("K (Kill)         = {}", result.gpk.k_count);
    println!("最大キャリー連鎖 = {}", result.gpk.max_carry_chain);
    if x == 3 {
        println!("(x=3: G=m2(AND), P=m7(XOR), K=m9(NOR) / 定理5.1)");
    }
    println!("計算時間 = {:?}", elapsed);

    // ファイル保存
    let filename = format!("step_{}n1_{}_{}.txt", x, short_n(&n), timestamp());
    let path = output_dir().join(&filename);
    if let Ok(mut f) = File::create(&path) {
        writeln!(f, "# collatz-m4m6 step (層2: GPK付き)").ok();
        writeln!(f, "n = {}", n).ok();
        writeln!(f, "x = {}", x).ok();
        writeln!(f, "k = {}", pair.pair_count()).ok();
        writeln!(f, "xn+1 = {}", &n * x + 1u64).ok();
        writeln!(f, "d = {}", result.d).ok();
        writeln!(f, "n' = {}", n_prime).ok();
        writeln!(f, "exchanged = {}", result.exchanged).ok();
        writeln!(f, "gpk_seq = {}", gpk_str).ok();
        writeln!(f, "G = {}", result.gpk.g_count).ok();
        writeln!(f, "P = {}", result.gpk.p_count).ok();
        writeln!(f, "K = {}", result.gpk.k_count).ok();
        writeln!(f, "max_carry_chain = {}", result.gpk.max_carry_chain).ok();
        writeln!(f, "elapsed = {:?}", elapsed).ok();
        println!("\n保存: {}", path.display());
    }
}

fn cmd_trace(args: &[String]) {
    if args.is_empty() {
        eprintln!("使い方: collatz-m4m6 trace <n> [x]");
        return;
    }

    let n = parse_n(&args[0]);
    let x = parse_x(&args[1..], 3);
    let max_steps = 100_000;

    println!("軌道追跡 (層2: GPK付き): n={}, x={}", n, x);
    println!("(最大 {} ステップ)", max_steps);
    println!();

    let timer = Instant::now();
    let last_print = std::cell::Cell::new(Instant::now());
    let result = trace_trajectory_with_callback(&n, x, max_steps, |step, bits, _d| {
        let now = Instant::now();
        if now.duration_since(last_print.get()).as_millis() >= 1000 {
            let elapsed = timer.elapsed();
            let sps = step as f64 / elapsed.as_secs_f64();
            eprint!(
                "\x1b[2K\r  [{:.1}s] step {} | ~{}bits | {:.0} steps/s",
                elapsed.as_secs_f64(), step, bits, sps
            );
            last_print.set(now);
        }
    });
    let elapsed = timer.elapsed();
    eprintln!();

    // 画面表示（長すぎる場合は省略）
    let show_limit = 50;
    println!("  {:>6}  {:>50}  {:>4}  GPK", "step", "n", "d");
    println!("  {:>6}  {:>50}", 0, format_big(&n));

    for (i, ((next_n, d), gpk)) in result.steps.iter().zip(result.gpk_per_step.iter()).enumerate() {
        if i < show_limit || i >= result.steps.len().saturating_sub(5) {
            let gpk_str = gpk_to_str(gpk);
            let gpk_display = if gpk_str.len() <= 20 { gpk_str } else { format!("{}...", &gpk_str[..17]) };
            println!("  {:>6}  {:>50}  d={:<3} {}", i + 1, format_big(next_n), d, gpk_display);
        } else if i == show_limit {
            println!("  ... ({} ステップ省略) ...", result.steps.len().saturating_sub(show_limit + 5));
        }
    }

    let sum_d: u64 = result.steps.iter().map(|(_, d)| d).sum();
    let gs = &result.gpk_stats;
    let total_gpk = gs.total_g + gs.total_p + gs.total_k;

    println!();
    println!("--- 統計 ---");
    println!("総ステップ数 (奇数→奇数) = {}", result.total_steps);
    println!("総÷2回数 (Σd)            = {}", sum_d);
    println!("標準ステップ数            = {} (= ステップ + Σd)", result.total_steps + sum_d);
    println!("最大値                    = {}", format_big(&result.max_value));
    println!("最大値の桁数              = {}", result.max_value.to_string().len());
    println!("1に到達                   = {}", if result.reached_one { "はい" } else { "いいえ" });

    println!();
    println!("--- GPK 統計 ---");
    if total_gpk > 0 {
        println!("G (Generate)  = {} ({:.1}%)", gs.total_g, gs.total_g as f64 / total_gpk as f64 * 100.0);
        println!("P (Propagate) = {} ({:.1}%)", gs.total_p, gs.total_p as f64 / total_gpk as f64 * 100.0);
        println!("K (Kill)      = {} ({:.1}%)", gs.total_k, gs.total_k as f64 / total_gpk as f64 * 100.0);
        println!("総ペア数      = {}", total_gpk);
    }
    // キャリー伝播距離ヒストグラム（上位のみ表示）
    println!("キャリー連鎖長分布:");
    for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
        if count > 0 {
            println!("  距離{:<3}: {} 回", dist, count);
        }
    }
    println!("計算時間                  = {:?}", elapsed);

    // CSV保存: 全軌道 + GPK
    let filename = format!("trace_{}n1_{}_s{}_{}.csv", x, short_n(&n), max_steps, timestamp());
    let path = output_dir().join(&filename);
    if let Ok(file) = File::create(&path) {
        let mut w = BufWriter::new(file);
        writeln!(w, "step,n,d,digits,gpk,G,P,K,max_carry_chain").ok();
        writeln!(w, "0,{},0,{},,0,0,0,0", n, n.to_string().len()).ok();
        for (i, ((next_n, d), gpk)) in result.steps.iter().zip(result.gpk_per_step.iter()).enumerate() {
            writeln!(w, "{},{},{},{},{},{},{},{},{}",
                i + 1, next_n, d, next_n.to_string().len(),
                gpk_to_str(gpk), gpk.g_count, gpk.p_count, gpk.k_count, gpk.max_carry_chain
            ).ok();
        }
        w.flush().ok();
        println!("\n軌道CSV保存: {}", path.display());
    }

    // サマリー保存
    let summary_name = format!("trace_{}n1_{}_{}_summary.txt", x, short_n(&n), timestamp());
    let summary_path = output_dir().join(&summary_name);
    if let Ok(mut f) = File::create(&summary_path) {
        writeln!(f, "# collatz-m4m6 trace (層2: GPK付き)").ok();
        writeln!(f, "start = {}", n).ok();
        writeln!(f, "x = {}", x).ok();
        writeln!(f, "total_steps (odd-to-odd) = {}", result.total_steps).ok();
        writeln!(f, "sum_d = {}", sum_d).ok();
        writeln!(f, "standard_steps = {}", result.total_steps + sum_d).ok();
        writeln!(f, "max_value = {}", result.max_value).ok();
        writeln!(f, "max_value_digits = {}", result.max_value.to_string().len()).ok();
        writeln!(f, "reached_one = {}", result.reached_one).ok();
        writeln!(f, "").ok();
        writeln!(f, "# GPK Statistics").ok();
        writeln!(f, "total_G = {}", gs.total_g).ok();
        writeln!(f, "total_P = {}", gs.total_p).ok();
        writeln!(f, "total_K = {}", gs.total_k).ok();
        writeln!(f, "total_pairs = {}", total_gpk).ok();
        if total_gpk > 0 {
            writeln!(f, "G% = {:.2}", gs.total_g as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "P% = {:.2}", gs.total_p as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "K% = {:.2}", gs.total_k as f64 / total_gpk as f64 * 100.0).ok();
        }
        writeln!(f, "").ok();
        writeln!(f, "# Carry chain histogram (distance: count)").ok();
        for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
            if count > 0 {
                writeln!(f, "{}: {}", dist, count).ok();
            }
        }
        writeln!(f, "").ok();
        writeln!(f, "elapsed = {:?}", elapsed).ok();
        println!("サマリー保存: {}", summary_path.display());
    }
}

fn cmd_verify(args: &[String]) {
    if args.len() < 2 {
        eprintln!("使い方: collatz-m4m6 verify <start> <end> [x]");
        return;
    }

    let start = parse_n(&args[0]);
    let end = parse_n(&args[1]);
    let x = parse_x(&args[2..], 3);
    let max_steps = 100_000;

    let num_threads = rayon::current_num_threads();
    println!("範囲検証 (層2: GPK統計付き): [{}, {}], x={}", start, end, x);
    println!("(停止時間法、最大 {} ステップ/数、{}スレッド並列)", max_steps, num_threads);
    println!();

    let timer = Instant::now();
    let last_print = std::sync::Mutex::new(Instant::now());
    let result = verify_range_parallel(&start, &end, x, max_steps, |done, total| {
        if total > 0 {
            let now = Instant::now();
            if let Ok(mut lp) = last_print.try_lock() {
                if now.duration_since(*lp).as_millis() >= 500 {
                    let elapsed = timer.elapsed();
                    let pct = done as f64 / total as f64 * 100.0;
                    let nps = done as f64 / elapsed.as_secs_f64();
                    let remaining = if done > 0 {
                        let eta_s = (total - done) as f64 / nps;
                        if eta_s > 3600.0 {
                            format!("{:.1}h", eta_s / 3600.0)
                        } else if eta_s > 60.0 {
                            format!("{:.0}m{:.0}s", eta_s / 60.0, eta_s % 60.0)
                        } else {
                            format!("{:.0}s", eta_s)
                        }
                    } else {
                        "---".to_string()
                    };
                    eprint!(
                        "\x1b[2K\r  [{:.1}s] {}/{} ({:.1}%) | {:.0} nums/s | 残り約{}",
                        elapsed.as_secs_f64(), done, total, pct, nps, remaining
                    );
                    *lp = now;
                }
            }
        }
    });
    let elapsed = timer.elapsed();

    eprintln!();
    println!();
    println!("--- 結果 ---");
    println!("検証した奇数の数    = {}", result.total_checked);
    println!("全て収束            = {}", if result.all_converged { "はい" } else { "いいえ" });
    println!("最大停止時間        = {} (n={})", result.max_stopping_time, result.max_stopping_time_number);

    // GPK 統計
    let gs = &result.gpk_stats;
    let total_gpk = gs.total_g + gs.total_p + gs.total_k;
    println!();
    println!("--- GPK 統計 ---");
    if total_gpk > 0 {
        println!("G (Generate)  = {} ({:.1}%)", gs.total_g, gs.total_g as f64 / total_gpk as f64 * 100.0);
        println!("P (Propagate) = {} ({:.1}%)", gs.total_p, gs.total_p as f64 / total_gpk as f64 * 100.0);
        println!("K (Kill)      = {} ({:.1}%)", gs.total_k, gs.total_k as f64 / total_gpk as f64 * 100.0);
        println!("総ペア数      = {}", total_gpk);
        println!("総ステップ数  = {}", gs.total_steps);
    }
    println!("キャリー連鎖長分布:");
    for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
        if count > 0 {
            println!("  距離{:<3}: {} 回", dist, count);
        }
    }
    println!("計算時間            = {:?}", elapsed);

    if !result.failures.is_empty() {
        println!("収束しなかった数    = {} 個", result.failures.len());
        for f in &result.failures[..result.failures.len().min(10)] {
            println!("  {}", f);
        }
    }

    // 結果保存
    let filename = format!("verify_{}n1_{}-{}_s{}_{}.txt", x, short_n(&start), short_n(&end), max_steps, timestamp());
    let path = output_dir().join(&filename);
    if let Ok(mut f) = File::create(&path) {
        writeln!(f, "# collatz-m4m6 verify (層2: GPK統計付き)").ok();
        writeln!(f, "range = [{}, {}]", start, end).ok();
        writeln!(f, "x = {}", x).ok();
        writeln!(f, "max_steps_per_number = {}", max_steps).ok();
        writeln!(f, "threads = {}", num_threads).ok();
        writeln!(f, "total_checked = {}", result.total_checked).ok();
        writeln!(f, "all_converged = {}", result.all_converged).ok();
        writeln!(f, "max_stopping_time = {}", result.max_stopping_time).ok();
        writeln!(f, "max_stopping_time_number = {}", result.max_stopping_time_number).ok();
        writeln!(f, "failures = {}", result.failures.len()).ok();
        writeln!(f, "").ok();
        writeln!(f, "# GPK Statistics").ok();
        writeln!(f, "total_G = {}", gs.total_g).ok();
        writeln!(f, "total_P = {}", gs.total_p).ok();
        writeln!(f, "total_K = {}", gs.total_k).ok();
        writeln!(f, "total_pairs = {}", total_gpk).ok();
        writeln!(f, "total_gpk_steps = {}", gs.total_steps).ok();
        if total_gpk > 0 {
            writeln!(f, "G% = {:.4}", gs.total_g as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "P% = {:.4}", gs.total_p as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "K% = {:.4}", gs.total_k as f64 / total_gpk as f64 * 100.0).ok();
        }
        writeln!(f, "").ok();
        writeln!(f, "# Carry chain histogram (distance: count)").ok();
        for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
            if count > 0 {
                writeln!(f, "{}: {}", dist, count).ok();
            }
        }
        writeln!(f, "").ok();
        writeln!(f, "elapsed = {:?}", elapsed).ok();
        if !result.failures.is_empty() {
            writeln!(f, "\n# 収束しなかった数:").ok();
            for fail in &result.failures {
                writeln!(f, "{}", fail).ok();
            }
        }
        println!("\n保存: {}", path.display());
    }
}

fn format_big(n: &BigUint) -> String {
    let s = n.to_string();
    if s.len() <= 50 {
        s
    } else {
        format!("{}...{} ({}桁)", &s[..20], &s[s.len()-20..], s.len())
    }
}
