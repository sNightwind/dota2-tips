use crate::config::{self, BarCfg, Config};
use crate::heroes;
use crate::model::{self, Live};
use eframe::egui::{self, Color32, Pos2, RichText, Stroke, Vec2};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PANEL: Color32 = Color32::from_rgb(31, 31, 31);
const BORDER: Color32 = Color32::from_rgb(62, 62, 62);
const TEXT: Color32 = Color32::from_rgb(212, 212, 212);
const MUTED: Color32 = Color32::from_rgb(140, 140, 140);
const TRACK: Color32 = Color32::from_rgb(46, 46, 46);
const ACCENT: Color32 = Color32::from_rgb(55, 148, 255);

pub fn run(bind: String, token: String) -> Result<(), String> {
    let path = config::beside_exe();
    let config = config::load_or_create(&path)?;
    let live = Arc::new(Mutex::new(Live::default()));
    let server_live = Arc::clone(&live);
    let server_bind = bind.clone();
    let server_token = token.clone();
    std::thread::spawn(move || {
        if let Err(error) = listen(&server_bind, &server_token, server_live) {
            eprintln!("{error}");
        }
    });

    let viewport = egui::ViewportBuilder::default()
        .with_title("dota2-tips")
        .with_decorations(false)
        .with_transparent(false)
        .with_always_on_top()
        .with_resizable(true)
        .with_inner_size([360.0, 240.0])
        .with_position(egui::pos2(32.0, 80.0));
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "dota2-tips",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(TipsApp::new(config, path, live)))
        }),
    )
    .map_err(|error| format!("窗口启动失败：{error}"))
}

fn listen(bind: &str, token: &str, live: Arc<Mutex<Live>>) -> Result<(), String> {
    let server = tiny_http::Server::http(bind).map_err(|error| format!("无法监听 {bind}：{error}"))?;
    println!("dota2-tips 监听 http://{bind}");
    let mut announced = false;
    for mut request in server.incoming_requests() {
        if request.method() != &tiny_http::Method::Post {
            let _ = request.respond(tiny_http::Response::from_string("ok"));
            continue;
        }
        let mut bytes = Vec::new();
        if request.as_reader().read_to_end(&mut bytes).is_err() {
            let _ = request.respond(tiny_http::Response::empty(400));
            continue;
        }
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                let _ = request.respond(tiny_http::Response::empty(400));
                continue;
            }
        };
        if !crate::catalog::auth_matches(&value, token) {
            let _ = request.respond(tiny_http::Response::empty(401));
            continue;
        }
        let _ = request.respond(tiny_http::Response::empty(200));
        live.lock().expect("live lock").apply(&value);
        if !announced {
            announced = true;
            println!("已收到 GSI。Ctrl+Shift+F8 打开设置。");
        }
    }
    Ok(())
}

struct TipsApp {
    config: Config,
    path: std::path::PathBuf,
    live: Arc<Mutex<Live>>,
    settings: bool,
    content_height: f32,
    applied_height: f32,
    hotkey: Option<GlobalHotKeyManager>,
    hotkey_error: Option<String>,
    save_error: Option<String>,
}

impl TipsApp {
    fn new(config: Config, path: std::path::PathBuf, live: Arc<Mutex<Live>>) -> Self {
        let (hotkey, hotkey_error) = match register_hotkey() {
            Ok(manager) => (Some(manager), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            settings: hotkey_error.is_some(),
            content_height: 0.0,
            applied_height: 0.0,
            config,
            path,
            live,
            hotkey,
            hotkey_error,
            save_error: None,
        }
    }

    fn tr(&self, zh: &'static str, en: &'static str) -> &'static str {
        if self.config.language_is_en() { en } else { zh }
    }

    fn persist(&mut self) {
        self.save_error = config::save(&self.path, &self.config).err();
    }

    fn poll_hotkey(&mut self) {
        let Some(receiver) = self.hotkey.as_ref().map(|_| GlobalHotKeyEvent::receiver()) else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            if event.state() == HotKeyState::Pressed {
                self.settings = !self.settings;
            }
        }
    }
}

impl eframe::App for TipsApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        PANEL.to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_style(ctx);
        self.poll_hotkey();
        if self.content_height > 0.0 && (self.content_height - self.applied_height).abs() > 8.0 {
            self.applied_height = self.content_height;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                360.0,
                self.content_height + 8.0,
            )));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        if self.settings && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.settings = false;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let live = self.live.lock().expect("live lock").clone();
        let response = egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                self.header(ui);
                ui.add_space(8.0);
                self.play_view(ui, &live);
                if self.settings {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);
                    self.settings_view(ui);
                }
            });
        self.content_height = response.response.rect.height();
    }
}

impl TipsApp {
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let title = ui.allocate_response(Vec2::new(120.0, 18.0), egui::Sense::drag());
            if title.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            ui.painter().text(
                title.rect.left_center(),
                egui::Align2::LEFT_CENTER,
                "dota2-tips",
                egui::FontId::proportional(14.0),
                TEXT,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.settings {
                    self.tr("关闭", "Close")
                } else {
                    self.tr("设置", "Settings")
                };
                if ui.button(label).clicked() {
                    self.settings = !self.settings;
                }
            });
        });
        let hint = if let Some(error) = &self.hotkey_error {
            error.clone()
        } else if self.settings {
            self.tr("Esc 关闭设置", "Esc closes settings").to_string()
        } else {
            self.tr("Ctrl+Shift+F8 打开设置", "Ctrl+Shift+F8 opens settings")
                .to_string()
        };
        ui.label(RichText::new(hint).color(MUTED).size(12.0));
    }

    fn play_view(&self, ui: &mut egui::Ui, live: &Live) {
        if !live.seen_payload {
            ui.label(RichText::new(self.tr("等待 Dota", "Waiting for Dota")).color(MUTED));
        }
        let paused = live.clock.as_ref().is_some_and(|clock| clock.paused);
        if let Some(clock) = &live.clock {
            let now = clock.game_now();
            self.rule_bar(
                ui,
                &self.config.bars.camp,
                self.tr("野怪", "Camps"),
                now,
                self.config.rules.camp_first_sec as f64,
                self.config.rules.camp_interval_sec as f64,
            );
            self.rule_bar(
                ui,
                &self.config.bars.bounty,
                self.tr("赏金符", "Bounty"),
                now,
                self.config.rules.bounty_first_sec as f64,
                self.config.rules.bounty_interval_sec as f64,
            );
            self.rule_bar(
                ui,
                &self.config.bars.power,
                self.tr("神符", "Power"),
                now,
                self.config.rules.power_first_sec as f64,
                self.config.rules.power_interval_sec as f64,
            );
        }
        if self.config.bars.items.enabled {
            if !live.items.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(self.tr("道具", "Items")).color(MUTED).size(12.0));
            }
            for item in &live.items {
                let remaining = item.remaining_now(paused);
                let title = heroes::display_item(&item.name);
                meter(ui, &title, remaining, item.max, self.config.bars.items.is_ring());
            }
        }
        if self.config.bars.enemies.enabled {
            ui.add_space(6.0);
            ui.label(RichText::new(self.tr("敌方", "Enemies")).color(MUTED).size(12.0));
            if live.enemies.is_empty() {
                ui.label(RichText::new(self.tr("当前没有", "None")).color(MUTED));
            } else {
                let names: Vec<String> = live
                    .enemies
                    .iter()
                    .map(|unit| {
                        heroes::display_hero(
                            unit,
                            &self.config.language,
                            &self.config.hero_name,
                        )
                    })
                    .collect();
                ui.label(RichText::new(names.join("   ")).color(TEXT));
            }
        }
    }

    fn rule_bar(&self, ui: &mut egui::Ui, bar: &BarCfg, title: &str, clock: f64, first: f64, interval: f64) {
        if !bar.enabled {
            return;
        }
        let (remaining, total) = model::cycle_remaining(clock, first, interval);
        meter(ui, title, remaining, total, bar.is_ring());
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(self.tr("设置", "Settings")).color(TEXT).strong());
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tr("语言", "Language")).color(MUTED));
            if ui.selectable_label(self.config.language == "zh", "中文").clicked() {
                self.config.language = "zh".to_string();
                self.persist();
            }
            if ui.selectable_label(self.config.language == "en", "English").clicked() {
                self.config.language = "en".to_string();
                self.persist();
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tr("英雄名", "Hero name")).color(MUTED));
            if ui
                .selectable_label(!self.config.hero_name_is_short(), self.tr("全名", "Full"))
                .clicked()
            {
                self.config.hero_name = "full".to_string();
                self.persist();
            }
            if ui
                .selectable_label(self.config.hero_name_is_short(), self.tr("简称", "Short"))
                .clicked()
            {
                self.config.hero_name = "short".to_string();
                self.persist();
            }
        });
        ui.label(
            RichText::new(self.tr("幻影刺客的简称是 PA", "Phantom Assassin is shown as PA"))
                .color(MUTED)
                .size(12.0),
        );

        self.bar_editor(ui, self.tr("野怪", "Camps"), |config| &mut config.bars.camp);
        self.bar_editor(ui, self.tr("赏金符", "Bounty"), |config| &mut config.bars.bounty);
        self.bar_editor(ui, self.tr("神符", "Power"), |config| &mut config.bars.power);
        self.bar_editor(ui, self.tr("道具", "Items"), |config| &mut config.bars.items);
        ui.horizontal(|ui| {
            let mut enabled = self.config.bars.enemies.enabled;
            if ui.checkbox(&mut enabled, self.tr("敌方英雄名", "Enemy names")).changed() {
                self.config.bars.enemies.enabled = enabled;
                self.persist();
            }
        });

        ui.add_space(6.0);
        ui.label(RichText::new(self.tr("刷新周期（秒）", "Cycles (seconds)")).color(MUTED).size(12.0));
        self.rule_field(ui, self.tr("野怪首次", "Camp first"), |rules| &mut rules.camp_first_sec);
        self.rule_field(ui, self.tr("野怪间隔", "Camp every"), |rules| &mut rules.camp_interval_sec);
        self.rule_field(ui, self.tr("赏金首次", "Bounty first"), |rules| &mut rules.bounty_first_sec);
        self.rule_field(ui, self.tr("赏金间隔", "Bounty every"), |rules| &mut rules.bounty_interval_sec);
        self.rule_field(ui, self.tr("神符首次", "Power first"), |rules| &mut rules.power_first_sec);
        self.rule_field(ui, self.tr("神符间隔", "Power every"), |rules| &mut rules.power_interval_sec);

        if let Some(error) = &self.save_error {
            ui.label(RichText::new(error).color(Color32::from_rgb(220, 120, 120)));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(self.tr("关闭设置", "Close")).clicked() {
                self.settings = false;
            }
            if ui.button(self.tr("退出", "Quit")).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn bar_editor(&mut self, ui: &mut egui::Ui, title: &str, field: impl Fn(&mut Config) -> &mut BarCfg) {
        ui.horizontal(|ui| {
            let enabled_now = field(&mut self.config).enabled;
            let ring_now = field(&mut self.config).is_ring();
            let mut enabled = enabled_now;
            let enabled_changed = ui.checkbox(&mut enabled, title).changed();
            let rect_clicked = ui.selectable_label(!ring_now, self.tr("矩形", "Bar")).clicked();
            let ring_clicked = ui.selectable_label(ring_now, self.tr("圆环", "Ring")).clicked();
            if enabled_changed || rect_clicked || ring_clicked {
                let bar = field(&mut self.config);
                bar.enabled = enabled;
                if rect_clicked {
                    bar.style = "rect".to_string();
                } else if ring_clicked {
                    bar.style = "ring".to_string();
                }
                self.persist();
            }
        });
    }

    fn rule_field(&mut self, ui: &mut egui::Ui, title: &str, field: impl Fn(&mut crate::config::Rules) -> &mut u32) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).color(TEXT));
            let value = field(&mut self.config.rules);
            let response = ui.add(egui::DragValue::new(value).range(0u32..=7200));
            if response.changed() {
                self.persist();
            }
        });
    }
}

fn meter(ui: &mut egui::Ui, title: &str, remaining: f64, total: f64, ring: bool) {
    ui.horizontal(|ui| {
        if ring {
            ring_icon(ui, model::ratio(remaining, total));
        }
        ui.label(RichText::new(title).color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format_remaining(remaining)).color(MUTED));
        });
    });
    if !ring {
        bar_icon(ui, model::ratio(remaining, total));
    }
}

fn bar_icon(ui: &mut egui::Ui, ratio: f32) {
    let width = ui.available_width().max(80.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, TRACK);
    let mut fill = rect;
    fill.set_width(rect.width() * ratio.clamp(0.0, 1.0));
    if fill.width() > 0.0 {
        painter.rect_filled(fill, 3.0, ACCENT);
    }
}

fn ring_icon(ui: &mut egui::Ui, ratio: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
    let center = rect.center();
    let painter = ui.painter();
    painter.circle_stroke(center, 6.0, Stroke::new(2.0, TRACK));
    if ratio <= 0.0 {
        return;
    }
    let steps = ((24.0 * ratio).ceil() as usize).max(1);
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::TAU * ratio.clamp(0.0, 1.0);
    let points: Vec<Pos2> = (0..=steps)
        .map(|step| {
            let angle = start + sweep * (step as f32 / steps as f32);
            center + Vec2::new(angle.cos(), angle.sin()) * 6.0
        })
        .collect();
    painter.line(points, Stroke::new(2.0, ACCENT));
}

fn format_remaining(seconds: f64) -> String {
    let total = seconds.max(0.0).ceil() as i64;
    let minutes = total / 60;
    let secs = total % 60;
    format!("{minutes}:{secs:02}")
}

fn install_fonts(ctx: &egui::Context) {
    let bytes = [r"C:\Windows\Fonts\Deng.ttf", r"C:\Windows\Fonts\simhei.ttf"]
        .into_iter()
        .find_map(|path| std::fs::read(path).ok());
    let Some(bytes) = bytes else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "ui".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "ui".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::TRANSPARENT;
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.extreme_bg_color = Color32::from_rgb(24, 24, 24);
    visuals.faint_bg_color = Color32::from_rgb(37, 37, 37);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(46, 46, 46);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(58, 58, 58);
    visuals.widgets.active.bg_fill = ACCENT;
    ctx.set_visuals(visuals);
}

fn register_hotkey() -> Result<GlobalHotKeyManager, String> {
    let manager = GlobalHotKeyManager::new().map_err(|error| format!("热键不可用：{error}"))?;
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::F8);
    manager
        .register(hotkey)
        .map_err(|error| format!("无法注册 Ctrl+Shift+F8：{error}"))?;
    Ok(manager)
}
