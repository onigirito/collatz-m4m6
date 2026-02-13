# collatz-m4m6

Pair Predicate Decomposition Tool for Collatz-type Maps $(xn+1)/2^d$

**[Website / Documentation](https://onigirito.github.io/collatz-m4m6/)** | **[Zenodo](https://doi.org/10.5281/zenodo.18627192)**

---

## Overview

This tool analyzes Collatz-type maps not as multiplication, but as **carry propagation in an adder circuit** using pair predicate decomposition (m4/m6 basis). It provides:

- **Single Analysis** — Trace a single odd number with full GPK decomposition, d-values, and carry chain lengths
- **Range Analysis (Sweep)** — Verify all odd numbers in a range using parallel threads, with GPK statistics accumulation
- **Log Analysis** — Browse and visualize past verification results

![Single Analysis](screenshot_single.png)
![Range Analysis](screenshot_sweep.png)
![Log Analysis](screenshot_log.png)

## Key Features

- **GPK Classification**: Each pair position is classified as Generate / Propagate / Kill, revealing the carry structure of the map
- **Carry chain length distribution**: Histogram of how far carries propagate — structural data not available from conventional tools
- **Three-tier computation**: u128 native (Phase 1) → U256 stack (Phase 1.5) → Kogge-Stone packed scan (Phase 2)
- **Sweep capability**: Exhaustive verification of all odd numbers in a range — the same method used in world-record verification

## Verified Results

| Map | Range | Odd numbers | Time | Result |
|-----|-------|-------------|------|--------|
| 3n+1 | 3 ~ 99,999,999,999 (~2^37) | 50 billion | ~2m 17s | All converge (max stopping time 345) |

Measured on Intel Core i7-12650H.

## Paper

**Pair Predicate Decomposition of Collatz-type Maps $(xn+1)/2^d$ and the Structural Closure of $3n+1$**

- [PDF](https://onigirito.github.io/collatz-m4m6/m4m6_xn.pdf) | [LaTeX source](https://onigirito.github.io/collatz-m4m6/m4m6_xn.tex) | [Zenodo](https://doi.org/10.5281/zenodo.18627192)
- DOI: [10.5281/zenodo.18521822](https://doi.org/10.5281/zenodo.18521822) (concept DOI, always resolves to latest version)
- Japanese version / 日本語版 — in preparation

## Download

**[collatz-gui.exe](https://github.com/onigirito/collatz-m4m6/releases/download/v0.4.0/collatz-gui.exe)** v0.4.0 (Windows x86_64, 5.4 MB, standalone / crt-static)

```
SHA-256: 78c050d11120c2a23606879a609f38f37f8267db5b8c04727933e63bdb0e9900
```
Verify: `certutil -hashfile collatz-gui.exe SHA256`

Requirements: Windows 10/11, AVX2-capable CPU (Intel Haswell 2013+ / AMD Excavator 2015+), OpenGL 2.0

## License

[MIT License](LICENSE) — Free to use, modify, and redistribute.

If you use this software or the accompanying paper in academic publications or commercial products, please [contact me](https://github.com/onigirito).

---

## 概要（日本語）

コラッツ型写像 $(xn+1)/2^d$ を乗算ではなく**加算器のキャリー伝播**として構造的に分解・解析するツールです。

- **単発解析** — 1つの奇数のペア述語分解（GPK列・d値・キャリー連鎖長）
- **区間解析（スイープ）** — 指定範囲の全奇数を並列検証、GPK統計を蓄積
- **ログ解析** — 過去の検証ログの閲覧・可視化

詳細は **[サイト](https://onigirito.github.io/collatz-m4m6/)** をご覧ください。
