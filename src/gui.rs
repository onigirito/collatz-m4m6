#![windows_subsystem = "windows"]

use collatz_m4m6::*;
use eframe::egui;
use egui_plot::{Bar, BarChart, Plot};
use num_bigint::BigUint;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write as IoWrite};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

fn main() -> eframe::Result<()> {
    #[cfg(target_arch = "x86_64")]
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("ERROR: This build requires a CPU with AVX2 support (Intel Haswell 2013+ / AMD Excavator 2015+).");
        eprintln!("エラー: このビルドはAVX2対応CPU（Intel第4世代以降 / AMD 2015年以降）が必要です。");
        std::process::exit(1);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("Collatz m4/m6 Scanner (Layer2: GPK)"),
        ..Default::default()
    };
    eframe::run_native(
        "collatz-m4m6",
        options,
        Box::new(|cc| {
            setup_japanese_font(&cc.egui_ctx);
            Ok(Box::new(CollatzApp::default()))
        }),
    )
}

fn setup_japanese_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_paths = [
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
    ];
    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                "japanese".to_owned(),
                egui::FontData::from_owned(data),
            );
            fonts.families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "japanese".to_owned());
            fonts.families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("japanese".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn output_dir() -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("output")))
        .unwrap_or_else(|| PathBuf::from("output"));
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

fn short_n(n: &str) -> String {
    if n.len() <= 16 {
        n.to_string()
    } else {
        let bits = (n.len() as f64 * 3.3219).round() as u64;
        format!("2p{}", bits)
    }
}

fn gpk_to_str(info: &GpkInfo) -> String {
    info.gpk_string(info.active_pairs)
}

// ─── データ構造 ─────────────────────────────────────

#[derive(PartialEq)]
enum Tab { Single, Range, Analysis }

struct StepResultDisplay {
    n_prime: String,
    d: u64,
    exchanged: bool,
    gpk_str: String,
    g: u32, p: u32, k: u32,
    max_carry: u32,
    elapsed_us: u128,
}

struct TraceResultDisplay {
    total_steps: u64,
    sum_d: u64,
    max_value_digits: usize,
    reached_one: bool,
    cancelled: bool,
    gpk_stats: GpkStats,
    steps_preview: Vec<(u64, String, u64, String)>,
    elapsed_ms: u128,
    save_path: Option<String>,
}

struct SingleTraceState {
    running: bool,
    step: u64,
    digits: usize,
    result: Option<TraceResultDisplay>,
}

struct RangeState {
    running: bool,
    done: u64,
    total: u64,
    nps: f64,
    elapsed_s: f64,
    result: Option<VerifyResultDisplay>,
}

struct VerifyResultDisplay {
    total_checked: u64,
    all_converged: bool,
    max_stopping_time: u64,
    max_stopping_time_number: String,
    cancelled: bool,
    gpk_stats: GpkStats,
    elapsed_s: f64,
    save_path: Option<String>,
}

/// 解析タブ用: ログファイルから読み取ったデータ
struct LoadedLog {
    filename: String,
    header: String,
    params: Vec<(String, String)>,
    gpk_stats: GpkStats,
}

struct CollatzApp {
    tab: Tab,
    x_input: String,
    x_val: u64,
    max_steps_input: String,
    collect_gpk: bool,
    use_phase1: bool,
    use_stopping_time: bool,
    // 単発解析
    single_n_input: String,
    single_step_result: Option<StepResultDisplay>,
    single_trace_state: Arc<Mutex<SingleTraceState>>,
    single_cancel: Arc<AtomicBool>,
    // 区間解析
    range_start_input: String,
    range_end_input: String,
    range_state: Arc<Mutex<RangeState>>,
    range_cancel: Arc<AtomicBool>,
    // 解析タブ: ログビューア
    log_files: Vec<String>,
    selected_log: Option<usize>,
    loaded_log: Option<LoadedLog>,
}

impl Default for CollatzApp {
    fn default() -> Self {
        Self {
            tab: Tab::Range,
            x_input: "3".to_string(),
            x_val: 3,
            max_steps_input: "1000".to_string(),
            collect_gpk: true,
            use_phase1: true,
            use_stopping_time: true,
            single_n_input: "27".to_string(),
            single_step_result: None,
            single_trace_state: Arc::new(Mutex::new(SingleTraceState {
                running: false, step: 0, digits: 0, result: None,
            })),
            single_cancel: Arc::new(AtomicBool::new(false)),
            range_start_input: "3".to_string(),
            range_end_input: "9999999".to_string(),
            range_state: Arc::new(Mutex::new(RangeState {
                running: false, done: 0, total: 0, nps: 0.0, elapsed_s: 0.0, result: None,
            })),
            range_cancel: Arc::new(AtomicBool::new(false)),
            log_files: Vec::new(),
            selected_log: None,
            loaded_log: None,
        }
    }
}

impl eframe::App for CollatzApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        {
            let trace_running = self.single_trace_state.lock().unwrap().running;
            let range_running = self.range_state.lock().unwrap().running;
            if trace_running || range_running {
                ctx.request_repaint();
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Collatz m4/m6");
                ui.separator();
                ui.label("x =");
                let resp = ui.add(egui::TextEdit::singleline(&mut self.x_input).desired_width(40.0));
                if resp.changed() {
                    if let Ok(v) = self.x_input.parse::<u64>() {
                        if v >= 3 && (v - 1).is_power_of_two() {
                            self.x_val = v;
                        }
                    }
                }
                // x の有効性フィードバック
                let x_input_valid = self.x_input.parse::<u64>()
                    .map(|v| v >= 3 && (v - 1).is_power_of_two())
                    .unwrap_or(false);
                if x_input_valid {
                    ui.label(format!("({})", self.x_val));
                } else {
                    ui.colored_label(egui::Color32::from_rgb(220, 50, 50),
                        format!("x-1が2の冪でない → x={}で実行", self.x_val));
                }
                ui.separator();
                ui.label("max_steps:");
                ui.add(egui::TextEdit::singleline(&mut self.max_steps_input).desired_width(60.0));
                ui.checkbox(&mut self.collect_gpk, "GPK統計");
                ui.checkbox(&mut self.use_phase1, "u128 Phase1");
                ui.checkbox(&mut self.use_stopping_time, "停止時間判定");
                ui.separator();
                if ui.selectable_label(self.tab == Tab::Single, "単発解析").clicked() {
                    self.tab = Tab::Single;
                }
                if ui.selectable_label(self.tab == Tab::Range, "区間解析").clicked() {
                    self.tab = Tab::Range;
                }
                if ui.selectable_label(self.tab == Tab::Analysis, "解析").clicked() {
                    self.tab = Tab::Analysis;
                    self.refresh_log_files();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.tab {
                Tab::Single => self.ui_single(ui),
                Tab::Range => self.ui_range(ui),
                Tab::Analysis => self.ui_analysis(ui),
            }
        });
    }
}

impl CollatzApp {
    // ─── 単発解析 ──────────────────────────────
    fn ui_single(&mut self, ui: &mut egui::Ui) {
        let trace_running = self.single_trace_state.lock().unwrap().running;

        ui.horizontal(|ui| {
            ui.label("n =");
            ui.add(egui::TextEdit::singleline(&mut self.single_n_input).desired_width(200.0));
            // 2^XX 近似表示
            let n_len = self.single_n_input.trim().len();
            if n_len > 0 {
                let approx_bits = (n_len as f64 * 3.3219).round() as u64;
                ui.colored_label(egui::Color32::GRAY,
                    format!("≈2^{} ({}桁)", approx_bits, n_len));
            }
            ui.add_enabled_ui(!trace_running, |ui| {
                if ui.button("1ステップ").clicked() {
                    self.run_step();
                }
            });
            if !trace_running {
                if ui.button("軌道追跡").clicked() {
                    self.start_trace();
                }
            } else {
                if ui.button("停止").clicked() {
                    self.single_cancel.store(true, Ordering::Relaxed);
                }
            }
        });

        {
            let state = self.single_trace_state.lock().unwrap();
            if state.running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("step {} | ~{}桁", state.step, state.digits));
                });
            }
        }

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(ref step) = self.single_step_result {
                ui.heading("1ステップ結果");
                egui::Grid::new("step_grid").striped(true).show(ui, |ui| {
                    ui.label("n'"); ui.label(&step.n_prime); ui.end_row();
                    ui.label("d (÷2)"); ui.label(format!("{}", step.d)); ui.end_row();
                    ui.label("交換"); ui.label(if step.exchanged { "あり" } else { "なし" }); ui.end_row();
                    ui.label("GPK列"); ui.label(&step.gpk_str); ui.end_row();
                    ui.label("G/P/K"); ui.label(format!("{}/{}/{}", step.g, step.p, step.k)); ui.end_row();
                    ui.label("最大carry連鎖"); ui.label(format!("{}", step.max_carry)); ui.end_row();
                    ui.label("時間"); ui.label(format!("{}us", step.elapsed_us)); ui.end_row();
                });
            }

            let trace_result = &self.single_trace_state.lock().unwrap().result;
            if let Some(ref trace) = *trace_result {
                ui.heading(if trace.cancelled { "軌道追跡結果 (中断)" } else { "軌道追跡結果" });
                egui::Grid::new("trace_grid").striped(true).show(ui, |ui| {
                    ui.label("ステップ"); ui.label(format!("{}", trace.total_steps)); ui.end_row();
                    ui.label("Σd"); ui.label(format!("{}", trace.sum_d)); ui.end_row();
                    ui.label("最大値桁数"); ui.label(format!("{}", trace.max_value_digits)); ui.end_row();
                    ui.label("1到達"); ui.label(if trace.reached_one { "はい" } else { "いいえ" }); ui.end_row();
                    ui.label("時間"); ui.label(format!("{}ms", trace.elapsed_ms)); ui.end_row();
                });

                let gs = &trace.gpk_stats;
                let total = gs.total_g + gs.total_p + gs.total_k;
                if total > 0 {
                    ui.label(format!("GPK: G={} ({:.1}%) P={} ({:.1}%) K={} ({:.1}%)",
                        gs.total_g, gs.total_g as f64 / total as f64 * 100.0,
                        gs.total_p, gs.total_p as f64 / total as f64 * 100.0,
                        gs.total_k, gs.total_k as f64 / total as f64 * 100.0,
                    ));
                }

                if let Some(ref path) = trace.save_path {
                    ui.colored_label(egui::Color32::GREEN, format!("保存: {}", path));
                }

                ui.separator();
                ui.label("軌道 (先頭)");
                egui::Grid::new("trace_steps").striped(true).show(ui, |ui| {
                    ui.label("step"); ui.label("n"); ui.label("d"); ui.label("GPK"); ui.end_row();
                    for (step, n_str, d, gpk) in &trace.steps_preview {
                        ui.label(format!("{}", step));
                        ui.label(n_str);
                        ui.label(format!("{}", d));
                        ui.label(gpk);
                        ui.end_row();
                    }
                });
            }
        });
    }

    // ─── 区間解析 ──────────────────────────────
    fn ui_range(&mut self, ui: &mut egui::Ui) {
        let running = self.range_state.lock().unwrap().running;

        ui.horizontal(|ui| {
            ui.label("開始:");
            ui.add(egui::TextEdit::singleline(&mut self.range_start_input).desired_width(120.0));
            ui.label("終了:");
            ui.add(egui::TextEdit::singleline(&mut self.range_end_input).desired_width(120.0));
            // 終了値の 2^XX 近似表示
            let end_len = self.range_end_input.trim().len();
            if end_len > 6 {
                let approx_bits = (end_len as f64 * 3.3219).round() as u64;
                ui.colored_label(egui::Color32::GRAY, format!("≈2^{}", approx_bits));
            }
            if !running {
                if ui.button("検証開始").clicked() {
                    self.start_verify();
                }
            } else {
                if ui.button("停止").clicked() {
                    self.range_cancel.store(true, Ordering::Relaxed);
                }
            }
        });

        ui.separator();

        let state = self.range_state.lock().unwrap();

        if state.running && state.total > 0 {
            let pct = state.done as f32 / state.total as f32;
            ui.add(egui::ProgressBar::new(pct).text(format!(
                "{}/{} ({:.1}%) | {:.0} nums/s | {:.1}s",
                state.done, state.total, pct * 100.0, state.nps, state.elapsed_s
            )));
        }

        if let Some(ref result) = state.result {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(if result.cancelled { "検証結果 (中断)" } else { "検証結果" });

                ui.columns(2, |cols| {
                    egui::Grid::new("verify_grid").striped(true).show(&mut cols[0], |ui| {
                        ui.label("検証数"); ui.label(format!("{}", result.total_checked)); ui.end_row();
                        ui.label("全て収束"); ui.label(if result.all_converged { "はい" } else { "いいえ" }); ui.end_row();
                        ui.label("最大停止時間"); ui.label(format!("{} (n={})", result.max_stopping_time, result.max_stopping_time_number)); ui.end_row();
                        ui.label("時間"); ui.label(format!("{:.2}s", result.elapsed_s)); ui.end_row();
                    });

                    let gs = &result.gpk_stats;
                    let total = gs.total_g + gs.total_p + gs.total_k;
                    if total > 0 {
                        egui::Grid::new("verify_gpk_grid").striped(true).show(&mut cols[1], |ui| {
                            ui.label("G (Generate)"); ui.label(format!("{} ({:.2}%)", gs.total_g, gs.total_g as f64 / total as f64 * 100.0)); ui.end_row();
                            ui.label("P (Propagate)"); ui.label(format!("{} ({:.2}%)", gs.total_p, gs.total_p as f64 / total as f64 * 100.0)); ui.end_row();
                            ui.label("K (Kill)"); ui.label(format!("{} ({:.2}%)", gs.total_k, gs.total_k as f64 / total as f64 * 100.0)); ui.end_row();
                            ui.label("総ペア"); ui.label(format!("{}", total)); ui.end_row();
                            ui.label("総ステップ"); ui.label(format!("{}", gs.total_steps)); ui.end_row();
                        });
                    }
                });

                let gs = &result.gpk_stats;
                let total = gs.total_g + gs.total_p + gs.total_k;
                if total > 0 {
                    ui.separator();
                    Self::draw_gpk_graphs(ui, gs, "range");
                }

                if let Some(ref path) = result.save_path {
                    ui.colored_label(egui::Color32::GREEN, format!("保存: {}", path));
                }
            });
        }
    }

    // ─── 解析タブ（ログビューア）──────────────────
    fn ui_analysis(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("output/ ログファイル:");
            if ui.button("更新").clicked() {
                self.refresh_log_files();
            }
        });

        if self.log_files.is_empty() {
            ui.label("ログファイルが見つかりません。先に解析を実行してください。");
            return;
        }

        // ファイル選択リスト（左） + 解析結果（右）
        ui.columns(2, |cols| {
            // 左: ファイルリスト
            cols[0].heading("ファイル");
            egui::ScrollArea::vertical().id_salt("log_list").show(&mut cols[0], |ui| {
                for (i, name) in self.log_files.iter().enumerate() {
                    let selected = self.selected_log == Some(i);
                    if ui.selectable_label(selected, name).clicked() {
                        self.selected_log = Some(i);
                        self.loaded_log = parse_log_file(&output_dir().join(name));
                    }
                }
            });

            // 右: 解析結果
            if let Some(ref log) = self.loaded_log {
                egui::ScrollArea::vertical().id_salt("log_view").show(&mut cols[1], |ui| {
                    ui.heading(&log.filename);
                    if !log.header.is_empty() {
                        ui.label(&log.header);
                    }

                    // パラメータ表示
                    if !log.params.is_empty() {
                        egui::Grid::new("log_params").striped(true).show(ui, |ui| {
                            for (k, v) in &log.params {
                                ui.label(k);
                                ui.label(v);
                                ui.end_row();
                            }
                        });
                    }

                    let gs = &log.gpk_stats;
                    let total = gs.total_g + gs.total_p + gs.total_k;
                    if total > 0 {
                        ui.separator();
                        ui.label(format!("GPK: G={} ({:.2}%) P={} ({:.2}%) K={} ({:.2}%)",
                            gs.total_g, gs.total_g as f64 / total as f64 * 100.0,
                            gs.total_p, gs.total_p as f64 / total as f64 * 100.0,
                            gs.total_k, gs.total_k as f64 / total as f64 * 100.0,
                        ));

                        ui.separator();
                        Self::draw_gpk_graphs(ui, gs, "log");
                    }
                });
            } else {
                cols[1].label("ファイルを選択してください。");
            }
        });
    }

    // ─── 共通: GPKグラフ描画 ──────────────────────
    fn draw_gpk_graphs(ui: &mut egui::Ui, gs: &GpkStats, id_prefix: &str) {
        let total = gs.total_g + gs.total_p + gs.total_k;
        if total == 0 { return; }

        let g_pct = gs.total_g as f64 / total as f64 * 100.0;
        let p_pct = gs.total_p as f64 / total as f64 * 100.0;
        let _k_pct = gs.total_k as f64 / total as f64 * 100.0;

        // ── GPK Heat ──
        let heat = g_pct + p_pct;  // carry活性度: G+P = 生成+伝播
        ui.horizontal(|ui| {
            ui.label("GPK Heat:");
            let (label, color) = if heat > 66.0 {
                (format!("{:.1}% (hot)", heat), egui::Color32::from_rgb(220, 80, 50))
            } else if heat > 60.0 {
                (format!("{:.1}% (warm)", heat), egui::Color32::from_rgb(200, 160, 50))
            } else {
                (format!("{:.1}% (cool)", heat), egui::Color32::from_rgb(80, 160, 200))
            };
            ui.colored_label(color, egui::RichText::new(label).strong());
        });

        ui.add_space(4.0);

        // ── GPK スタックドバー ──
        let bar_height = 24.0;
        let available_width = ui.available_width().min(600.0);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(available_width, bar_height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);

        let g_w = (g_pct / 100.0) as f32 * rect.width();
        let p_w = (p_pct / 100.0) as f32 * rect.width();
        let k_w = rect.width() - g_w - p_w;

        let g_color = egui::Color32::from_rgb(220, 80, 80);   // 赤系
        let p_color = egui::Color32::from_rgb(100, 160, 220);  // 青系
        let k_color = egui::Color32::from_rgb(80, 190, 80);    // 緑系

        // G セグメント
        let g_rect = egui::Rect::from_min_size(rect.min, egui::vec2(g_w, bar_height));
        painter.rect_filled(g_rect, 0.0, g_color);

        // P セグメント
        let p_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(g_w, 0.0), egui::vec2(p_w, bar_height));
        painter.rect_filled(p_rect, 0.0, p_color);

        // K セグメント
        let k_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(g_w + p_w, 0.0), egui::vec2(k_w, bar_height));
        painter.rect_filled(k_rect, 0.0, k_color);

        // ラベル描画
        let text_color = egui::Color32::WHITE;
        let font = egui::FontId::proportional(12.0);
        if g_w > 50.0 {
            painter.text(g_rect.center(), egui::Align2::CENTER_CENTER,
                format!("G {:.1}%", g_pct), font.clone(), text_color);
        }
        if p_w > 50.0 {
            painter.text(p_rect.center(), egui::Align2::CENTER_CENTER,
                format!("P {:.1}%", p_pct), font.clone(), text_color);
        }
        if k_w > 50.0 {
            painter.text(k_rect.center(), egui::Align2::CENTER_CENTER,
                format!("K {:.1}%", k_pct), font, text_color);
        }

        ui.add_space(4.0);

        // ── キャリー連鎖長ヒストグラム ──
        let bars: Vec<Bar> = gs.carry_chain_hist.iter().enumerate()
            .filter(|(_, &c)| c > 0)
            .map(|(d, &c)| Bar::new(d as f64, c as f64))
            .collect();
        if !bars.is_empty() {
            ui.label("キャリー連鎖長分布");
            Plot::new(format!("{}_carry", id_prefix))
                .height(110.0)
                .allow_drag(false)
                .allow_zoom(false)
                .x_axis_label("連鎖長")
                .y_axis_label("回数")
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(BarChart::new(bars).width(0.8));
                });
        }

        ui.collapsing("キャリー連鎖長 詳細", |ui| {
            egui::Grid::new(format!("{}_carry_detail", id_prefix)).striped(true).show(ui, |ui| {
                ui.label("距離"); ui.label("回数"); ui.end_row();
                for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
                    if count > 0 {
                        ui.label(format!("{}", dist));
                        ui.label(format!("{}", count));
                        ui.end_row();
                    }
                }
            });
        });
    }

    // ─── ログファイル一覧取得 ──────────────────────
    fn refresh_log_files(&mut self) {
        let dir = output_dir();
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".txt") { Some(name) } else { None }
            })
            .collect();
        files.sort();
        files.reverse(); // 新しいファイルが上
        self.log_files = files;
    }

    // ─── 計算実行 ──────────────────────────────

    fn run_step(&mut self) {
        let n = match BigUint::from_str(&self.single_n_input) {
            Ok(n) => n,
            Err(_) => return,
        };
        let x = self.x_val;
        let pair = PairNumber::from_biguint(&n);
        let timer = Instant::now();
        let result = collatz_step(&pair, x);
        let elapsed = timer.elapsed();
        let gpk_str: String = gpk_to_str(&result.gpk);
        self.single_step_result = Some(StepResultDisplay {
            n_prime: result.next.to_biguint().to_string(),
            d: result.d,
            exchanged: result.exchanged,
            gpk_str: if gpk_str.len() <= 100 { gpk_str } else { format!("{}...", &gpk_str[..97]) },
            g: result.gpk.g_count,
            p: result.gpk.p_count,
            k: result.gpk.k_count,
            max_carry: result.gpk.max_carry_chain,
            elapsed_us: elapsed.as_micros(),
        });
    }

    fn start_trace(&mut self) {
        let n = match BigUint::from_str(&self.single_n_input) {
            Ok(n) => n,
            Err(_) => return,
        };
        let n_str = self.single_n_input.clone();
        let x = self.x_val;
        let collect_gpk = self.collect_gpk;
        self.single_cancel.store(false, Ordering::Relaxed);
        {
            let mut state = self.single_trace_state.lock().unwrap();
            state.running = true;
            state.step = 0;
            state.digits = 0;
            state.result = None;
        }
        let state = self.single_trace_state.clone();
        let cancel = self.single_cancel.clone();
        let max_steps = self.max_steps_input.parse::<u64>().unwrap_or(10_000);

        thread::spawn(move || {
            // パニック時も running = false を保証するガード
            let state_guard = state.clone();
            struct TraceGuard(Arc<Mutex<SingleTraceState>>);
            impl Drop for TraceGuard {
                fn drop(&mut self) {
                    if let Ok(mut s) = self.0.lock() {
                        s.running = false;
                    }
                }
            }
            let _guard = TraceGuard(state_guard);

            let timer = Instant::now();
            let state_cb = state.clone();
            let last_update = Mutex::new(Instant::now());
            let result = trace_trajectory_cancellable(&n, x, max_steps, &cancel, |step, digits, _d| {
                let now = Instant::now();
                if let Ok(mut lu) = last_update.try_lock() {
                    if now.duration_since(*lu).as_millis() >= 200 {
                        let mut s = state_cb.lock().unwrap();
                        s.step = step;
                        s.digits = digits;
                        *lu = now;
                    }
                }
            });
            let elapsed = timer.elapsed();
            let cancelled = cancel.load(Ordering::Relaxed);
            let sum_d: u64 = result.steps.iter().map(|(_, d)| d).sum();
            let save_path = save_trace_log(&n_str, x, max_steps, collect_gpk, &result, cancelled, elapsed);
            let steps_preview: Vec<_> = result.steps.iter()
                .zip(result.gpk_per_step.iter())
                .take(100)
                .enumerate()
                .map(|(i, ((next_n, d), gpk))| {
                    let ns = { let s = next_n.to_string(); if s.len() <= 30 { s } else { format!("{}...({}桁)", &s[..10], s.len()) } };
                    let gs: String = gpk_to_str(gpk);
                    let gd = if gs.len() <= 20 { gs } else { format!("{}...", &gs[..17]) };
                    ((i + 1) as u64, ns, *d, gd)
                })
                .collect();
            let mut s = state.lock().unwrap();
            s.running = false;
            s.result = Some(TraceResultDisplay {
                total_steps: result.total_steps, sum_d,
                max_value_digits: result.max_value.to_string().len(),
                reached_one: result.reached_one, cancelled,
                gpk_stats: result.gpk_stats, steps_preview,
                elapsed_ms: elapsed.as_millis(), save_path,
            });
        });
    }

    fn start_verify(&mut self) {
        let start = match BigUint::from_str(&self.range_start_input) { Ok(n) => n, Err(_) => return };
        let end = match BigUint::from_str(&self.range_end_input) { Ok(n) => n, Err(_) => return };
        let start_str = self.range_start_input.clone();
        let end_str = self.range_end_input.clone();
        let x = self.x_val;
        let collect_gpk = self.collect_gpk;
        let use_phase1 = self.use_phase1;
        let use_stopping_time = self.use_stopping_time;
        self.range_cancel.store(false, Ordering::Relaxed);
        {
            let mut s = self.range_state.lock().unwrap();
            s.running = true; s.done = 0; s.total = 0; s.nps = 0.0; s.elapsed_s = 0.0; s.result = None;
        }
        let state = self.range_state.clone();
        let cancel = self.range_cancel.clone();
        let max_steps = self.max_steps_input.parse::<u64>().unwrap_or(10_000);

        thread::spawn(move || {
            // パニック時も running = false を保証するガード
            let state_guard = state.clone();
            struct RunGuard(Arc<Mutex<RangeState>>);
            impl Drop for RunGuard {
                fn drop(&mut self) {
                    if let Ok(mut s) = self.0.lock() {
                        s.running = false;
                    }
                }
            }
            let _guard = RunGuard(state_guard);

            let timer = Instant::now();
            let state_cb = state.clone();
            let last_update = Mutex::new(Instant::now());
            let result = verify_range_parallel_cancellable(&start, &end, x, max_steps, collect_gpk, use_phase1, use_stopping_time, &cancel, |done, total| {
                let now = Instant::now();
                if let Ok(mut lu) = last_update.try_lock() {
                    if now.duration_since(*lu).as_millis() >= 200 {
                        let elapsed = timer.elapsed();
                        let mut s = state_cb.lock().unwrap();
                        s.done = done; s.total = total;
                        s.elapsed_s = elapsed.as_secs_f64();
                        s.nps = if elapsed.as_secs_f64() > 0.0 { done as f64 / elapsed.as_secs_f64() } else { 0.0 };
                        *lu = now;
                    }
                }
            });
            let elapsed = timer.elapsed();
            let cancelled = cancel.load(Ordering::Relaxed);
            let save_path = save_verify_log(&start_str, &end_str, x, max_steps, collect_gpk, use_phase1, use_stopping_time, &result, cancelled, elapsed);
            let mut s = state.lock().unwrap();
            s.running = false;
            s.result = Some(VerifyResultDisplay {
                total_checked: result.total_checked,
                all_converged: result.all_converged,
                max_stopping_time: result.max_stopping_time,
                max_stopping_time_number: result.max_stopping_time_number.to_string(),
                cancelled, gpk_stats: result.gpk_stats,
                elapsed_s: elapsed.as_secs_f64(), save_path,
            });
        });
    }
}

// ─── ログ保存 ───────────────────────────────────

fn save_trace_log(
    n_str: &str, x: u64, max_steps: u64, collect_gpk: bool, result: &TrajectoryResult,
    cancelled: bool, elapsed: std::time::Duration,
) -> Option<String> {
    let ts = timestamp();
    let tag = if cancelled { "_stopped" } else { "" };
    let gpk_tag = if collect_gpk { "_gpk" } else { "" };
    let sn = short_n(n_str);

    let csv_name = format!("gui_trace_{}n1_{}_s{}{}{}.csv", x, sn, max_steps, gpk_tag, tag);
    let csv_path = output_dir().join(&csv_name);
    if let Ok(file) = File::create(&csv_path) {
        let mut w = BufWriter::new(file);
        // ヘッダー: 奇数n'の16述語 + 偶数xn+1の16述語 + GPK
        write!(w, "step,n,d,exchanged,pairs").ok();
        for p in 1..=16u8 {
            write!(w, ",m{}", p).ok();
        }
        write!(w, ",raw_pairs").ok();
        for p in 1..=16u8 {
            write!(w, ",raw_m{}", p).ok();
        }
        writeln!(w, ",digits,gpk,G,P,K,max_carry_chain").ok();

        // 初期値（pair_steps[0]）
        if let Some(ps0) = result.pair_steps.first() {
            write!(w, "0,{},0,false,{}", n_str, ps0.pair_count).ok();
            for p in 1..=16u8 {
                write!(w, ",0b{}", predicate_bits_msb(&ps0.m4_words, &ps0.m6_words, ps0.pair_count, p)).ok();
            }
            write!(w, ",0").ok(); // raw_pairs は初期値なし
            for _ in 1..=16u8 {
                write!(w, ",").ok();
            }
            writeln!(w, ",{},,0,0,0,0", n_str.len()).ok();
        }

        for (i, ((next_n, d), gpk)) in result.steps.iter().zip(result.gpk_per_step.iter()).enumerate() {
            let gs: String = gpk_to_str(gpk);
            let ps = &result.pair_steps[i + 1];
            write!(w, "{},{},{},{},{}", i + 1, next_n, d, ps.exchanged, ps.pair_count).ok();
            // 奇数n'の16述語
            for p in 1..=16u8 {
                write!(w, ",0b{}", predicate_bits_msb(&ps.m4_words, &ps.m6_words, ps.pair_count, p)).ok();
            }
            // 偶数xn+1の16述語
            if ps.raw_pair_count > 0 {
                write!(w, ",{}", ps.raw_pair_count).ok();
                for p in 1..=16u8 {
                    write!(w, ",0b{}", predicate_bits_msb(&ps.raw_m4_words, &ps.raw_m6_words, ps.raw_pair_count, p)).ok();
                }
            } else {
                write!(w, ",0").ok();
                for _ in 1..=16u8 {
                    write!(w, ",").ok();
                }
            }
            writeln!(w, ",{},{},{},{},{},{}", next_n.to_string().len(), gs, gpk.g_count, gpk.p_count, gpk.k_count, gpk.max_carry_chain).ok();
        }
        w.flush().ok();
    }

    let sum_name = format!("gui_trace_{}n1_{}_s{}{}{}_{}.txt", x, sn, max_steps, gpk_tag, tag, ts);
    let sum_path = output_dir().join(&sum_name);
    if let Ok(mut f) = File::create(&sum_path) {
        let sum_d: u64 = result.steps.iter().map(|(_, d)| d).sum();
        let gs = &result.gpk_stats;
        let total_gpk = gs.total_g + gs.total_p + gs.total_k;
        writeln!(f, "# collatz-m4m6 trace{}", if cancelled { " (stopped)" } else { "" }).ok();
        writeln!(f, "start = {}", n_str).ok();
        writeln!(f, "x = {}", x).ok();
        writeln!(f, "max_steps = {}", max_steps).ok();
        writeln!(f, "total_steps = {}", result.total_steps).ok();
        writeln!(f, "sum_d = {}", sum_d).ok();
        writeln!(f, "max_value_digits = {}", result.max_value.to_string().len()).ok();
        writeln!(f, "reached_one = {}", result.reached_one).ok();
        if cancelled { writeln!(f, "cancelled = true").ok(); }
        writeln!(f, "").ok();
        writeln!(f, "# GPK").ok();
        writeln!(f, "G = {}", gs.total_g).ok();
        writeln!(f, "P = {}", gs.total_p).ok();
        writeln!(f, "K = {}", gs.total_k).ok();
        writeln!(f, "total_pairs = {}", total_gpk).ok();
        if total_gpk > 0 {
            writeln!(f, "G% = {:.4}", gs.total_g as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "P% = {:.4}", gs.total_p as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "K% = {:.4}", gs.total_k as f64 / total_gpk as f64 * 100.0).ok();
        }
        writeln!(f, "").ok();
        writeln!(f, "# Carry chain histogram").ok();
        for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
            if count > 0 { writeln!(f, "{}: {}", dist, count).ok(); }
        }
        writeln!(f, "elapsed = {:?}", elapsed).ok();
        writeln!(f, "csv = {}", csv_name).ok();
        return Some(sum_path.display().to_string());
    }
    None
}

fn save_verify_log(
    start_str: &str, end_str: &str, x: u64, max_steps: u64, collect_gpk: bool, use_phase1: bool, use_stopping_time: bool,
    result: &VerifyResult, cancelled: bool, elapsed: std::time::Duration,
) -> Option<String> {
    let ts = timestamp();
    let tag = if cancelled { "_stopped" } else { "" };
    let gpk_tag = if collect_gpk { "_gpk" } else { "" };
    let p1_tag = if !use_phase1 { "_nop1" } else { "" };
    let st_tag = if !use_stopping_time { "_fullpath" } else { "" };
    let filename = format!("gui_verify_{}n1_{}-{}_s{}{}{}{}{}_{}.txt", x, short_n(start_str), short_n(end_str), max_steps, gpk_tag, p1_tag, st_tag, tag, ts);
    let path = output_dir().join(&filename);
    if let Ok(mut f) = File::create(&path) {
        let gs = &result.gpk_stats;
        let total_gpk = gs.total_g + gs.total_p + gs.total_k;
        writeln!(f, "# collatz-m4m6 verify{}", if cancelled { " (stopped)" } else { "" }).ok();
        writeln!(f, "range = [{}, {}]", start_str, end_str).ok();
        writeln!(f, "x = {}", x).ok();
        writeln!(f, "max_steps_per_number = {}", max_steps).ok();
        writeln!(f, "use_phase1 = {}", use_phase1).ok();
        writeln!(f, "use_stopping_time = {}", use_stopping_time).ok();
        writeln!(f, "total_checked = {}", result.total_checked).ok();
        writeln!(f, "all_converged = {}", result.all_converged).ok();
        writeln!(f, "max_stopping_time = {}", result.max_stopping_time).ok();
        writeln!(f, "max_stopping_time_n = {}", result.max_stopping_time_number).ok();
        if cancelled { writeln!(f, "cancelled = true").ok(); }
        writeln!(f, "").ok();
        writeln!(f, "# GPK").ok();
        writeln!(f, "G = {}", gs.total_g).ok();
        writeln!(f, "P = {}", gs.total_p).ok();
        writeln!(f, "K = {}", gs.total_k).ok();
        writeln!(f, "total_pairs = {}", total_gpk).ok();
        writeln!(f, "total_steps = {}", gs.total_steps).ok();
        if total_gpk > 0 {
            writeln!(f, "G% = {:.4}", gs.total_g as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "P% = {:.4}", gs.total_p as f64 / total_gpk as f64 * 100.0).ok();
            writeln!(f, "K% = {:.4}", gs.total_k as f64 / total_gpk as f64 * 100.0).ok();
        }
        writeln!(f, "").ok();
        writeln!(f, "# Carry chain histogram").ok();
        for (dist, &count) in gs.carry_chain_hist.iter().enumerate() {
            if count > 0 { writeln!(f, "{}: {}", dist, count).ok(); }
        }
        writeln!(f, "elapsed = {:?}", elapsed).ok();
        return Some(path.display().to_string());
    }
    None
}

// ─── ログファイルパーサー ─────────────────────────

fn parse_log_file(path: &PathBuf) -> Option<LoadedLog> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    let filename = path.file_name()?.to_string_lossy().to_string();
    let mut header = String::new();
    let mut params: Vec<(String, String)> = Vec::new();
    let mut gpk_stats = GpkStats::new();
    let mut in_histogram = false;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            if header.is_empty() {
                header = trimmed.trim_start_matches('#').trim().to_string();
            }
            if trimmed.contains("Carry chain histogram") {
                in_histogram = true;
            } else {
                in_histogram = false;
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        if in_histogram {
            // "距離: 回数" 形式
            if let Some((dist_str, count_str)) = trimmed.split_once(':') {
                if let (Ok(dist), Ok(count)) = (dist_str.trim().parse::<usize>(), count_str.trim().parse::<u64>()) {
                    if dist < 128 {
                        gpk_stats.carry_chain_hist[dist] = count;
                    }
                }
            }
            continue;
        }

        // "key = value" 形式
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "G" => { gpk_stats.total_g = value.parse().unwrap_or(0); }
                "P" => { gpk_stats.total_p = value.parse().unwrap_or(0); }
                "K" => { gpk_stats.total_k = value.parse().unwrap_or(0); }
                "total_steps" | "total_gpk_steps" => { gpk_stats.total_steps = value.parse().unwrap_or(0); }
                "total_pairs" => { gpk_stats.total_pairs = value.parse().unwrap_or(0); }
                _ => {}
            }

            // G%, P%, K%, csv, elapsed は params に入れない（冗長）
            if !key.ends_with('%') && key != "csv" && key != "elapsed"
                && key != "G" && key != "P" && key != "K"
                && key != "total_pairs" && key != "total_gpk_steps"
            {
                params.push((key.to_string(), value.to_string()));
            }
        }
    }

    Some(LoadedLog { filename, header, params, gpk_stats })
}
