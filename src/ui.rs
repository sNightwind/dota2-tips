use crate::config::{self, BarCfg, Config};
use crate::dota_setup::{
    self, ActionReport, CfgOutcome, CfgPresence, LaunchOutcome, Note, SetupStatus,
};
use crate::heroes;
use crate::model::{self, Live};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PANEL: Color32 = Color32::from_rgb(31, 31, 31);
const BORDER: Color32 = Color32::from_rgb(62, 62, 62);
const TEXT: Color32 = Color32::from_rgb(212, 212, 212);
const MUTED: Color32 = Color32::from_rgb(140, 140, 140);
const TRACK: Color32 = Color32::from_rgb(46, 46, 46);
const ACCENT: Color32 = Color32::from_rgb(55, 148, 255);
const STACK: Color32 = Color32::from_rgb(230, 180, 60);
const READY: Color32 = Color32::from_rgb(80, 200, 120);

pub fn run(bind: String, token: String) -> Result<(), String> {
    let path = config::beside_exe();
    let (config, _) = config::repair(&path)?;
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
        .with_transparent(true)
        .with_always_on_top()
        .with_resizable(true)
        .with_clamp_size_to_monitor_size(true)
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
    let server =
        tiny_http::Server::http(bind).map_err(|error| format!("无法监听 {bind}：{error}"))?;
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
            println!("已收到 GSI。点右上角「设置」。");
        }
    }
    Ok(())
}

enum SetupJob {
    Probe,
    Install,
    Uninstall,
}

enum SetupMsg {
    Probe(SetupStatus),
    Action(ActionReport),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Install,
    Timing,
    Cooldown,
    Minimap,
}

impl SettingsTab {
    fn salt(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Timing => "timing",
            Self::Cooldown => "cooldown",
            Self::Minimap => "minimap",
        }
    }
}

struct TipsApp {
    config: Config,
    path: std::path::PathBuf,
    live: Arc<Mutex<Live>>,
    settings: bool,
    settings_tab: SettingsTab,
    content_height: f32,
    /// 设置正文比当前窗口高时，下一帧把窗口加高到这么多，但不超过屏幕。
    scroll_extra: f32,
    /// 这一帧指针压在按钮、勾选或数值上，不能把这次按下当成拖窗口。
    block_window_drag: bool,
    applied_height: f32,
    panel_rect: Option<egui::Rect>,
    save_error: Option<String>,
    setup_status: Option<SetupStatus>,
    setup_notice: Option<ActionReport>,
    setup_busy: bool,
    setup_latched: bool,
    setup_rx: Option<mpsc::Receiver<SetupMsg>>,
}

impl TipsApp {
    fn new(config: Config, path: std::path::PathBuf, live: Arc<Mutex<Live>>) -> Self {
        Self {
            settings: false,
            settings_tab: SettingsTab::Install,
            content_height: 0.0,
            scroll_extra: 0.0,
            block_window_drag: false,
            applied_height: 0.0,
            panel_rect: None,
            config,
            path,
            live,
            save_error: None,
            setup_status: None,
            setup_notice: None,
            setup_busy: false,
            setup_latched: false,
            setup_rx: None,
        }
    }

    fn tr(&self, zh: &'static str, en: &'static str) -> &'static str {
        if self.config.language_is_en() { en } else { zh }
    }

    fn persist(&mut self) {
        self.save_error = config::save(&self.path, &self.config).err();
    }
}

impl eframe::App for TipsApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_style(ctx);
        self.poll_setup();
        if self.settings && !self.setup_latched {
            self.setup_latched = true;
            if !self.setup_busy {
                self.setup_notice = None;
                self.start_setup(ctx.clone(), SetupJob::Probe);
            }
        }
        if !self.settings {
            self.setup_latched = false;
        }
        let mut height = self.content_height.ceil();
        if let Some(limit) = window_height_limit(ctx) {
            height = height.min(limit);
        }
        if height > 0.0 && (height - self.applied_height).abs() > 1.0 {
            self.applied_height = height;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(360.0, height)));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        if self.settings && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.settings = false;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let live = self.live.lock().expect("live lock").clone();
        // 拖动层必须比按钮先登记，后画的控件才在上面。只感应拖动时，一按下就会变成拖动，
        // 所以指针落在按钮、勾选或数值上时不把窗口交出去。
        self.block_window_drag = false;
        let mut drag_started = false;
        if let Some(rect) = self.panel_rect {
            let drag = ui.interact(rect, ui.id().with("panel-drag"), egui::Sense::drag());
            drag_started = drag.drag_started();
        }
        self.scroll_extra = 0.0;
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
                if drag_started && !self.block_window_drag {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            });
        self.panel_rect = Some(response.response.rect);
        self.content_height = response.response.rect.height() + self.scroll_extra;
    }
}

impl TipsApp {
    fn note_control(&mut self, response: &egui::Response) {
        self.block_window_drag |= response.is_pointer_button_down_on();
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("dota2-tips").size(14.0).color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let quit = ui.button(self.tr("退出", "Quit"));
                if quit.clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                self.note_control(&quit);
                let label = if self.settings {
                    self.tr("关闭", "Close")
                } else {
                    self.tr("设置", "Settings")
                };
                let settings = ui.button(label);
                if settings.clicked() {
                    self.settings = !self.settings;
                }
                self.note_control(&settings);
            });
        });
        if self.settings {
            ui.label(
                RichText::new(self.tr("Esc 关闭设置", "Esc closes settings"))
                    .color(MUTED)
                    .size(12.0),
            );
        }
    }

    fn play_view(&self, ui: &mut egui::Ui, live: &Live) {
        if !live.showing_match() {
            let text = if live.seen_payload {
                self.tr("等待对局", "Waiting for a match")
            } else {
                self.tr("等待 Dota", "Waiting for Dota")
            };
            ui.label(RichText::new(text).color(MUTED));
            return;
        }
        let paused = live.clock.as_ref().is_some_and(|clock| clock.paused);
        let Some(clock) = &live.clock else {
            return;
        };
        let now = clock.game_now();
        self.rule_bar(
            ui,
            &self.config.bars.camp,
            self.tr("野怪", "Camps"),
            now,
            self.config.rules.camp_first_sec as f64,
            self.config.rules.camp_interval_sec as f64,
        );
        if self.config.bars.stack.enabled {
            if let Some(window) = model::stack_window(
                now,
                self.config.rules.camp_first_sec as f64,
                self.config.rules.camp_interval_sec as f64,
                self.config.rules.stack_left_sec as f64,
                self.config.rules.stack_right_sec as f64,
            ) {
                meter(
                    ui,
                    self.tr("拉野", "Stack"),
                    window.remaining,
                    window.total,
                    self.bar_color(window.remaining, if window.inside { STACK } else { ACCENT }),
                    None,
                );
            }
        }
        self.rule_bar(
            ui,
            &self.config.bars.bounty,
            self.tr("野区赏金", "Jungle bounty"),
            now,
            self.config.rules.bounty_first_sec as f64,
            self.config.rules.bounty_interval_sec as f64,
        );
        if self.config.bars.water.enabled {
            if let Some((remaining, total)) = model::water_remaining(
                now,
                self.config.rules.water_first_sec as f64,
                self.config.rules.water_second_sec as f64,
            ) {
                meter(
                    ui,
                    self.tr("中路圣水", "River water"),
                    remaining,
                    total,
                    self.bar_color(remaining, ACCENT),
                    None,
                );
            }
        }
        self.rule_bar(
            ui,
            &self.config.bars.power,
            self.tr("中路神符", "Power rune"),
            now,
            self.config.rules.power_first_sec as f64,
            self.config.rules.power_interval_sec as f64,
        );
        self.rule_bar(
            ui,
            &self.config.bars.lotus,
            self.tr("莲花池", "Lotus pool"),
            now,
            self.config.rules.lotus_first_sec as f64,
            self.config.rules.lotus_interval_sec as f64,
        );
        self.rule_bar(
            ui,
            &self.config.bars.wisdom,
            self.tr("智慧神龛", "Wisdom shrine"),
            now,
            self.config.rules.wisdom_first_sec as f64,
            self.config.rules.wisdom_interval_sec as f64,
        );
        if self.config.bars.items.enabled && !live.items.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(self.tr("道具", "Items"))
                    .color(MUTED)
                    .size(12.0),
            );
            let ready = self.tr("已就绪", "Ready");
            for item in &live.items {
                let remaining = item.remaining_now(paused);
                let title = heroes::display_item(&item.name);
                meter(ui, &title, remaining, item.max, ACCENT, Some(ready));
            }
        }
        self.enemy_column(ui, live, now);
    }

    fn enemy_column(&self, ui: &mut egui::Ui, live: &Live, now: f64) {
        let enemies = &self.config.bars.enemies;
        if enemies.visible {
            ui.add_space(6.0);
            ui.label(
                RichText::new(self.tr("已出现", "Visible"))
                    .color(MUTED)
                    .size(12.0),
            );
            if live.visible.is_empty() {
                ui.label(RichText::new(self.tr("当前没有", "None")).color(MUTED));
            } else {
                let names: Vec<String> = live
                    .visible
                    .iter()
                    .map(|unit| {
                        heroes::display_hero(unit, &self.config.language, &self.config.hero_name)
                    })
                    .collect();
                ui.label(RichText::new(names.join("   ")).color(TEXT));
            }
        }
        if enemies.missing {
            ui.add_space(6.0);
            ui.label(
                RichText::new(self.tr("未出现", "Missing"))
                    .color(MUTED)
                    .size(12.0),
            );
            if live.missing.is_empty() {
                ui.label(RichText::new(self.tr("当前没有", "None")).color(MUTED));
            } else {
                let due: Vec<_> = live
                    .missing
                    .iter()
                    .filter(|hero| {
                        (now - hero.gone_at).max(0.0) >= f64::from(enemies.missing_after_sec)
                    })
                    .collect();
                if due.is_empty() {
                    ui.label(RichText::new(self.tr("当前没有", "None")).color(MUTED));
                } else {
                    for hero in due {
                        let name = heroes::display_hero(
                            &hero.unit,
                            &self.config.language,
                            &self.config.hero_name,
                        );
                        let gone = (now - hero.gone_at).max(0.0).floor() as i64;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).color(STACK));
                            ui.label(RichText::new(format!("{gone}s")).color(MUTED));
                        });
                    }
                }
            }
        }
    }

    fn rule_bar(
        &self,
        ui: &mut egui::Ui,
        bar: &BarCfg,
        title: &str,
        clock: f64,
        first: f64,
        interval: f64,
    ) {
        if !bar.enabled {
            return;
        }
        let (remaining, total) = model::cycle_remaining(clock, first, interval);
        meter(
            ui,
            title,
            remaining,
            total,
            self.bar_color(remaining, ACCENT),
            None,
        );
    }

    fn bar_color(&self, remaining: f64, base: Color32) -> Color32 {
        let warn = self.config.warn_under_sec;
        if warn > 0 && remaining < f64::from(warn) {
            STACK
        } else {
            base
        }
    }

    fn start_setup(&mut self, ctx: egui::Context, job: SetupJob) {
        if self.setup_busy {
            return;
        }
        self.setup_busy = true;
        let (tx, rx) = mpsc::channel();
        self.setup_rx = Some(rx);
        std::thread::spawn(move || {
            let msg = match job {
                SetupJob::Probe => SetupMsg::Probe(dota_setup::probe()),
                SetupJob::Install => SetupMsg::Action(dota_setup::install()),
                SetupJob::Uninstall => SetupMsg::Action(dota_setup::uninstall()),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    fn poll_setup(&mut self) {
        let Some(rx) = &self.setup_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(msg) => {
                self.setup_rx = None;
                self.setup_busy = false;
                match msg {
                    SetupMsg::Probe(status) => self.setup_status = Some(status),
                    SetupMsg::Action(report) => {
                        self.setup_status = Some(report.status.clone());
                        self.setup_notice = Some(report);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.setup_rx = None;
                self.setup_busy = false;
            }
        }
    }

    fn dota_setup_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            RichText::new(self.tr("Dota 接入", "Dota setup"))
                .color(TEXT)
                .strong(),
        );
        let en = self.config.language_is_en();
        if self.setup_status.is_none() {
            ui.label(
                RichText::new(self.tr("正在检查本机 Dota 与 Steam…", "Checking Dota and Steam…"))
                    .color(MUTED)
                    .size(12.0),
            );
        }
        if let Some(status) = &self.setup_status {
            let dota = match &status.dota_dir {
                Some(path) => format!("{}{}", self.tr("Dota：", "Dota: "), path.display()),
                None => self.tr("Dota：未找到", "Dota: not found").to_string(),
            };
            ui.label(RichText::new(dota).color(MUTED).size(12.0));
            if let Some(persona) = &status.persona {
                ui.label(
                    RichText::new(format!("{}{persona}", self.tr("账号：", "Account: ")))
                        .color(MUTED)
                        .size(12.0),
                );
            }
            ui.label(
                RichText::new(cfg_line(&status.cfg, en))
                    .color(MUTED)
                    .size(12.0),
            );
            ui.label(
                RichText::new(launch_line(status, en))
                    .color(MUTED)
                    .size(12.0),
            );
            if status.steam_found && status.steam_running {
                ui.label(
                    RichText::new(self.tr(
                        "Steam 正在运行。可以写入配置文件；启动项要等完全退出 Steam（含托盘）后再点。",
                        "Steam is running. The cfg can be written; exit Steam completely, including the tray icon, before launch options can change.",
                    ))
                    .color(STACK)
                    .size(12.0),
                );
            }
            for note in &status.notes {
                let (color, text) = note_line(note, en);
                ui.label(RichText::new(text).color(color).size(12.0));
            }
        }
        ui.label(
            RichText::new(self.tr(
                "安装后请重新打开 Steam，并完全退出后重启 Dota。卸载只移除本工具的配置和启动参数。",
                "After installing, start Steam again and fully restart Dota. Remove setup only deletes this tool's cfg and launch option.",
            ))
            .color(MUTED)
            .size(12.0),
        );
        let windows_only = self.setup_status.as_ref().is_some_and(|status| {
            status
                .notes
                .iter()
                .any(|note| matches!(note, Note::WindowsOnly))
        });
        let ready = self.setup_status.is_some() && !self.setup_busy && !windows_only;
        let mut job = None;
        ui.horizontal(|ui| {
            let install = ui.add_enabled(
                ready,
                egui::Button::new(self.tr("安装配置", "Install setup")),
            );
            self.note_control(&install);
            if install.clicked() {
                job = Some(SetupJob::Install);
            }
            let remove = ui.add_enabled(
                ready,
                egui::Button::new(self.tr("卸载配置", "Remove setup")),
            );
            self.note_control(&remove);
            if remove.clicked() {
                job = Some(SetupJob::Uninstall);
            }
        });
        if self.setup_busy {
            ui.label(
                RichText::new(self.tr("正在处理…", "Working…"))
                    .color(MUTED)
                    .size(12.0),
            );
        }
        if let Some(report) = &self.setup_notice {
            let (color, text) = describe_setup(report, en);
            ui.label(RichText::new(text).color(color).size(12.0));
        }
        if let Some(job) = job {
            self.start_setup(ui.ctx().clone(), job);
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(self.tr("设置", "Settings"))
                .color(TEXT)
                .strong(),
        );
        self.settings_tab_bar(ui);
        ui.add_space(6.0);
        let cap = settings_scroll_max(ui);
        let tab = self.settings_tab;
        let output = egui::ScrollArea::vertical()
            .id_salt(("settings-body", tab.salt()))
            .auto_shrink([false, true])
            .max_height(cap)
            .show(ui, |ui| {
                match tab {
                    SettingsTab::Install => self.install_page(ui),
                    SettingsTab::Timing => self.timing_page(ui),
                    SettingsTab::Cooldown => self.cooldown_page(ui),
                    SettingsTab::Minimap => self.minimap_page(ui),
                }
                self.settings_footer(ui);
            });
        let viewport = output.inner_rect.height();
        let shown = output.content_size.y.min(cap);
        let extra = (shown - viewport).max(0.0);
        self.scroll_extra = if extra > 1.0 { extra } else { 0.0 };
    }

    fn settings_tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = [
            (SettingsTab::Install, self.tr("安装", "Install")),
            (SettingsTab::Timing, self.tr("计时", "Timers")),
            (SettingsTab::Cooldown, self.tr("冷却", "Cooldown")),
            (SettingsTab::Minimap, self.tr("小地图", "Minimap")),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding.x = 2.0;
            let gap = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - gap * (tabs.len() - 1) as f32 - 1.0)
                / tabs.len() as f32)
                .max(48.0);
            for (tab, label) in tabs {
                let selected = self.settings_tab == tab;
                let response = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, 24.0),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| ui.selectable_label(selected, label),
                    )
                    .inner;
                self.note_control(&response);
                if response.clicked() {
                    self.settings_tab = tab;
                }
            }
        });
    }

    fn install_page(&mut self, ui: &mut egui::Ui) {
        self.dota_setup_section(ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tr("语言", "Language")).color(MUTED));
            let zh = ui.selectable_label(self.config.language == "zh", "中文");
            self.note_control(&zh);
            if zh.clicked() {
                self.config.language = "zh".to_string();
                self.persist();
            }
            let en = ui.selectable_label(self.config.language == "en", "English");
            self.note_control(&en);
            if en.clicked() {
                self.config.language = "en".to_string();
                self.persist();
            }
        });
    }

    fn timing_page(&mut self, ui: &mut egui::Ui) {
        self.bar_editor(ui, self.tr("野怪", "Camps"), |config| {
            &mut config.bars.camp
        });
        self.bar_editor(ui, self.tr("拉野", "Stack"), |config| {
            &mut config.bars.stack
        });
        self.bar_editor(ui, self.tr("野区赏金", "Jungle bounty"), |config| {
            &mut config.bars.bounty
        });
        self.bar_editor(ui, self.tr("中路圣水", "River water"), |config| {
            &mut config.bars.water
        });
        self.bar_editor(ui, self.tr("中路神符", "Power rune"), |config| {
            &mut config.bars.power
        });
        self.bar_editor(ui, self.tr("莲花池", "Lotus pool"), |config| {
            &mut config.bars.lotus
        });
        self.bar_editor(ui, self.tr("智慧神龛", "Wisdom shrine"), |config| {
            &mut config.bars.wisdom
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tr("剩余低于", "Yellow under")).color(TEXT));
            let suffix = self.tr(" 秒变黄", " s");
            let value = &mut self.config.warn_under_sec;
            let response = ui.add(egui::DragValue::new(value).range(0u32..=600).suffix(suffix));
            self.note_control(&response);
            if response.changed() {
                self.persist();
            }
        });
        ui.label(
            RichText::new(self.tr(
                "只改野怪、拉野、赏金、圣水、神符、莲花和神龛。道具冷却保持蓝色。0 表示关闭。",
                "Camps, stack, bounty, water, power, lotus and shrines. Item bars stay blue. 0 turns this off.",
            ))
            .color(MUTED)
            .size(12.0),
        );
        ui.label(
            RichText::new(self.tr(
                "7.42：野区赏金每 4 分钟。中路 2:00 和 4:00 两侧圣水，6:00 起每 2 分钟一枚神符。莲花池 3:00 起每 3 分钟，智慧神龛 7:00 起每 7 分钟。",
                "7.42: jungle bounty every 4 min. River water at 2:00 and 4:00. One power rune every 2 min from 6:00. Lotus every 3 min from 3:00. Wisdom shrines every 7 min from 7:00.",
            ))
            .color(MUTED)
            .size(12.0),
        );

        ui.add_space(6.0);
        ui.label(
            RichText::new(self.tr("刷新周期（秒）", "Cycles (seconds)"))
                .color(MUTED)
                .size(12.0),
        );
        self.rule_field(ui, self.tr("野怪首次", "Camp first"), |rules| {
            &mut rules.camp_first_sec
        });
        self.rule_field(ui, self.tr("野怪间隔", "Camp every"), |rules| {
            &mut rules.camp_interval_sec
        });
        self.rule_field(ui, self.tr("拉野左边界", "Stack from"), |rules| {
            &mut rules.stack_left_sec
        });
        self.rule_field(ui, self.tr("拉野右边界", "Stack to"), |rules| {
            &mut rules.stack_right_sec
        });
        self.rule_field(ui, self.tr("赏金首次", "Bounty first"), |rules| {
            &mut rules.bounty_first_sec
        });
        self.rule_field(ui, self.tr("赏金间隔", "Bounty every"), |rules| {
            &mut rules.bounty_interval_sec
        });
        self.rule_field(ui, self.tr("圣水第一次", "Water first"), |rules| {
            &mut rules.water_first_sec
        });
        self.rule_field(ui, self.tr("圣水第二次", "Water second"), |rules| {
            &mut rules.water_second_sec
        });
        self.rule_field(ui, self.tr("神符首次", "Power first"), |rules| {
            &mut rules.power_first_sec
        });
        self.rule_field(ui, self.tr("神符间隔", "Power every"), |rules| {
            &mut rules.power_interval_sec
        });
        self.rule_field(ui, self.tr("莲花首次", "Lotus first"), |rules| {
            &mut rules.lotus_first_sec
        });
        self.rule_field(ui, self.tr("莲花间隔", "Lotus every"), |rules| {
            &mut rules.lotus_interval_sec
        });
        self.rule_field(ui, self.tr("神龛首次", "Shrine first"), |rules| {
            &mut rules.wisdom_first_sec
        });
        self.rule_field(ui, self.tr("神龛间隔", "Shrine every"), |rules| {
            &mut rules.wisdom_interval_sec
        });
    }

    fn cooldown_page(&mut self, ui: &mut egui::Ui) {
        self.bar_editor(ui, self.tr("道具", "Items"), |config| {
            &mut config.bars.items
        });
        ui.label(
            RichText::new(self.tr(
                "显示道具冷却。进度条保持蓝色。",
                "Shows item cooldowns. Bars stay blue.",
            ))
            .color(MUTED)
            .size(12.0),
        );
    }

    fn minimap_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tr("英雄名", "Hero name")).color(MUTED));
            let full = ui.selectable_label(
                !self.config.hero_name_is_short() && !self.config.hero_name_is_both(),
                self.tr("全名", "Full"),
            );
            self.note_control(&full);
            if full.clicked() {
                self.config.hero_name = "full".to_string();
                self.persist();
            }
            let short =
                ui.selectable_label(self.config.hero_name_is_short(), self.tr("简称", "Short"));
            self.note_control(&short);
            if short.clicked() {
                self.config.hero_name = "short".to_string();
                self.persist();
            }
            let both = ui.selectable_label(
                self.config.hero_name_is_both(),
                self.tr("全名+简称", "Full+short"),
            );
            self.note_control(&both);
            if both.clicked() {
                self.config.hero_name = "both".to_string();
                self.persist();
            }
        });
        ui.label(
            RichText::new(self.tr(
                "全名+简称的例子：幻影刺客PA",
                "Full+short looks like 幻影刺客PA",
            ))
            .color(MUTED)
            .size(12.0),
        );
        ui.horizontal(|ui| {
            let mut visible = self.config.bars.enemies.visible;
            let visible_box = ui.checkbox(&mut visible, self.tr("已出现", "Visible"));
            self.note_control(&visible_box);
            if visible_box.changed() {
                self.config.bars.enemies.visible = visible;
                self.persist();
            }
            let mut missing = self.config.bars.enemies.missing;
            let missing_box = ui.checkbox(&mut missing, self.tr("未出现", "Missing"));
            self.note_control(&missing_box);
            if missing_box.changed() {
                self.config.bars.enemies.missing = missing;
                self.persist();
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tr("消失超过", "Missing after")).color(TEXT));
            let suffix = self.tr(" 秒", " s");
            let value = &mut self.config.bars.enemies.missing_after_sec;
            let response = ui.add(egui::DragValue::new(value).range(0u32..=600).suffix(suffix));
            self.note_control(&response);
            if response.changed() {
                self.persist();
            }
        });
    }

    fn settings_footer(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = &self.save_error {
            ui.label(RichText::new(error).color(Color32::from_rgb(220, 120, 120)));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let close = ui.button(self.tr("关闭设置", "Close"));
            self.note_control(&close);
            if close.clicked() {
                self.settings = false;
            }
        });
    }

    fn bar_editor(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        field: impl Fn(&mut Config) -> &mut BarCfg,
    ) {
        ui.horizontal(|ui| {
            let mut enabled = field(&mut self.config).enabled;
            let response = ui.checkbox(&mut enabled, title);
            self.note_control(&response);
            if response.changed() {
                field(&mut self.config).enabled = enabled;
                self.persist();
            }
        });
    }

    fn rule_field(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        field: impl Fn(&mut crate::config::Rules) -> &mut u32,
    ) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).color(TEXT));
            let value = field(&mut self.config.rules);
            let response = ui.add(egui::DragValue::new(value).range(0u32..=7200));
            self.note_control(&response);
            if response.changed() {
                self.persist();
            }
        });
    }
}

fn monitor_height_and_top(ctx: &egui::Context) -> Option<(f32, f32)> {
    ctx.input(|input| {
        let monitor = input.viewport().monitor_size.map(|size| size.y)?;
        if monitor <= 1.0 {
            return None;
        }
        let top = input
            .viewport()
            .outer_rect
            .map(|rect| rect.min.y)
            .unwrap_or(80.0)
            .clamp(0.0, monitor);
        Some((monitor, top))
    })
}

fn window_height_limit(ctx: &egui::Context) -> Option<f32> {
    monitor_height_and_top(ctx).map(|(monitor, top)| (monitor - top - 8.0).max(200.0))
}

/// 设置正文最多用掉窗口顶到屏幕底之间还剩下的高度。
fn settings_scroll_max(ui: &egui::Ui) -> f32 {
    let y = ui.cursor().min.y;
    match monitor_height_and_top(ui.ctx()) {
        Some((monitor, top)) => (monitor - top - y - 40.0).max(180.0),
        None => 640.0,
    }
}

fn cfg_line(cfg: &CfgPresence, en: bool) -> String {
    let (zh, english) = match cfg {
        CfgPresence::Same => ("配置：已安装", "Config: installed"),
        CfgPresence::Absent => ("配置：未安装", "Config: not installed"),
        CfgPresence::Different => (
            "配置：文件在，但内容和程序不一致",
            "Config: file differs from this build",
        ),
        CfgPresence::Unreadable(_) => ("配置：无法读取", "Config: unreadable"),
        CfgPresence::Unknown => ("配置：未检查", "Config: not checked"),
    };
    if en {
        english.to_string()
    } else {
        zh.to_string()
    }
}

fn launch_line(status: &SetupStatus, en: bool) -> String {
    let (zh, english) = if status.launch_readable {
        if status.flag_present {
            (
                "启动项：已包含 -gamestateintegration",
                "Launch options: -gamestateintegration is present",
            )
        } else {
            (
                "启动项：未包含 -gamestateintegration",
                "Launch options: -gamestateintegration is missing",
            )
        }
    } else {
        ("启动项：未能读取", "Launch options: unavailable")
    };
    if en {
        english.to_string()
    } else {
        zh.to_string()
    }
}

fn note_line(note: &Note, en: bool) -> (Color32, String) {
    match note {
        Note::WindowsOnly => (
            MUTED,
            if en {
                "Automatic setup is only available on Windows.".to_string()
            } else {
                "仅支持 Windows 自动安装。".to_string()
            },
        ),
        Note::SteamNotFound => (
            MUTED,
            if en {
                "Steam was not found. Install Steam, or copy the cfg and add the launch option manually.".to_string()
            } else {
                "未找到 Steam。请确认已安装 Steam，或按说明手动复制配置。".to_string()
            },
        ),
        Note::DotaNotFound => (
            MUTED,
            if en {
                "Dota 2 is not installed in a Steam library.".to_string()
            } else {
                "未在 Steam 库中找到已安装的 Dota 2。".to_string()
            },
        ),
        Note::MultiAccount => (
            MUTED,
            if en {
                "Several Steam accounts were found, and none is marked most recent. Sign in to Steam once, then try again.".to_string()
            } else {
                "找到多个 Steam 账号，且没有最近登录标记。请先打开 Steam 登录一次。".to_string()
            },
        ),
        Note::NoAccount => (
            MUTED,
            if en {
                "No Steam account was found.".to_string()
            } else {
                "未找到 Steam 登录账号。".to_string()
            },
        ),
        Note::LocalconfigMissing => (
            MUTED,
            if en {
                "localconfig.vdf for this account was not found.".to_string()
            } else {
                "未找到这个账号的 localconfig.vdf。".to_string()
            },
        ),
        Note::Error(message) => (Color32::from_rgb(220, 120, 120), message.clone()),
    }
}

fn describe_setup(report: &ActionReport, en: bool) -> (Color32, String) {
    if report.install {
        describe_install(report, en)
    } else {
        describe_uninstall(report, en)
    }
}

fn describe_install(report: &ActionReport, en: bool) -> (Color32, String) {
    let cfg_ok = matches!(report.cfg, CfgOutcome::Written | CfgOutcome::Already);
    let err = Color32::from_rgb(220, 120, 120);
    match &report.launch {
        LaunchOutcome::Added | LaunchOutcome::AlreadyPresent if cfg_ok => {
            let text = if matches!(report.cfg, CfgOutcome::Already)
                && matches!(report.launch, LaunchOutcome::AlreadyPresent)
            {
                if en {
                    "The cfg and launch option are already in place. If Dota is running, exit it completely and start it again."
                } else {
                    "配置和启动项都已就绪。若 Dota 正在运行，请完全退出后重新打开。"
                }
            } else if en {
                "Setup is installed. Start Steam again, then fully restart Dota."
            } else {
                "配置已写入。请重新打开 Steam，并完全退出后重启 Dota。"
            };
            (READY, text.to_string())
        }
        LaunchOutcome::SkippedSteamRunning if cfg_ok => (
            STACK,
            if en {
                "The cfg is in the Dota folder. Launch options are unchanged: exit Steam completely, including the tray icon, then click Install setup again."
            } else {
                "配置文件已放入 Dota 目录。启动项尚未修改：请完全退出 Steam（含托盘图标）后再次点击安装配置。"
            }
            .to_string(),
        ),
        LaunchOutcome::SkippedNoSteam if cfg_ok => (
            STACK,
            if en {
                "The cfg is in the Dota folder. Steam was not found; add -gamestateintegration to the launch options manually."
            } else {
                "配置文件已放入 Dota 目录。未找到 Steam，请手动在启动项加上 -gamestateintegration。"
            }
            .to_string(),
        ),
        LaunchOutcome::SkippedNoAccount if cfg_ok => (
            STACK,
            if en {
                "The cfg is in the Dota folder. The Steam account could not be determined, so launch options were left unchanged. Sign in to Steam once, then click Install setup again."
            } else {
                "配置文件已放入 Dota 目录。未能确定 Steam 账号，启动项未改。请先登录一次 Steam 后再点安装配置。"
            }
            .to_string(),
        ),
        LaunchOutcome::SkippedNoFile if cfg_ok => (
            STACK,
            if en {
                "The cfg is in the Dota folder. localconfig.vdf was not found, so launch options were left unchanged."
            } else {
                "配置文件已放入 Dota 目录。未找到 localconfig.vdf，启动项未改。"
            }
            .to_string(),
        ),
        LaunchOutcome::Failed(message) if cfg_ok => (
            err,
            if en {
                format!("The cfg was written. Launch options could not be updated: {message}")
            } else {
                format!("配置文件已写入。修改启动项失败：{message}")
            },
        ),
        _ => (err, join_outcome(report, en, true))
    }
}

fn describe_uninstall(report: &ActionReport, en: bool) -> (Color32, String) {
    let cfg_ok = matches!(report.cfg, CfgOutcome::Removed | CfgOutcome::Absent);
    let err = Color32::from_rgb(220, 120, 120);
    match &report.launch {
        LaunchOutcome::Removed | LaunchOutcome::AlreadyAbsent if cfg_ok => {
            let text = if matches!(report.cfg, CfgOutcome::Absent)
                && matches!(report.launch, LaunchOutcome::AlreadyAbsent)
            {
                if en {
                    "There was nothing to remove."
                } else {
                    "没有需要卸载的配置。"
                }
            } else if en {
                "Removed this tool's cfg and -gamestateintegration from the launch options."
            } else {
                "已移除本工具的配置文件，并去掉启动项里的 -gamestateintegration。"
            };
            (READY, text.to_string())
        }
        LaunchOutcome::SkippedSteamRunning if cfg_ok => {
            let text = if matches!(report.cfg, CfgOutcome::Absent) {
                if en {
                    "The cfg was already absent. Launch options are unchanged: exit Steam completely, including the tray icon, then click Remove setup again."
                } else {
                    "配置文件本来就不在。启动项尚未修改：请完全退出 Steam（含托盘图标）后再次点击卸载配置。"
                }
            } else if en {
                "The cfg was deleted. Launch options are unchanged: exit Steam completely, including the tray icon, then click Remove setup again."
            } else {
                "已删除配置文件。启动项尚未修改：请完全退出 Steam（含托盘图标）后再次点击卸载配置。"
            };
            (STACK, text.to_string())
        }
        _ => (err, join_outcome(report, en, false)),
    }
}

fn join_outcome(report: &ActionReport, en: bool, install: bool) -> String {
    let cfg = match &report.cfg {
        CfgOutcome::Written => {
            if en {
                "The cfg was written."
            } else {
                "配置文件已写入。"
            }
        }
        CfgOutcome::Already => {
            if en {
                "The cfg was already installed."
            } else {
                "配置文件本来就在。"
            }
        }
        CfgOutcome::Removed => {
            if en {
                "The cfg was deleted."
            } else {
                "已删除配置文件。"
            }
        }
        CfgOutcome::Absent => {
            if en {
                "The cfg was already absent."
            } else {
                "配置文件本来就不在。"
            }
        }
        CfgOutcome::SkippedNoDota => {
            if en {
                "Dota 2 was not found."
            } else {
                "未找到 Dota 目录。"
            }
        }
        CfgOutcome::Failed(message) => {
            return if en {
                format!("Setup could not update the cfg: {message}")
            } else {
                format!("配置文件操作失败：{message}")
            };
        }
    };
    let launch = match &report.launch {
        LaunchOutcome::Added => {
            if en {
                "The launch option was added."
            } else {
                "已加上启动项。"
            }
        }
        LaunchOutcome::AlreadyPresent => {
            if en {
                "The launch option was already present."
            } else {
                "启动项里本来就有。"
            }
        }
        LaunchOutcome::Removed => {
            if en {
                "The launch option was removed."
            } else {
                "已去掉启动项。"
            }
        }
        LaunchOutcome::AlreadyAbsent => {
            if en {
                "The launch option was already absent."
            } else {
                "启动项里本来就没有。"
            }
        }
        LaunchOutcome::SkippedSteamRunning => {
            if en {
                "Exit Steam completely before launch options can change."
            } else {
                "需要先完全退出 Steam 才能改启动项。"
            }
        }
        LaunchOutcome::SkippedNoSteam => {
            if en {
                "Steam was not found."
            } else {
                "未找到 Steam。"
            }
        }
        LaunchOutcome::SkippedNoAccount => {
            if en {
                "The Steam account could not be determined."
            } else {
                "未能确定 Steam 账号。"
            }
        }
        LaunchOutcome::SkippedNoFile => {
            if en {
                "localconfig.vdf was not found."
            } else {
                "未找到 localconfig.vdf。"
            }
        }
        LaunchOutcome::Failed(message) => {
            let head = if en {
                format!("{cfg} ")
            } else {
                format!("{cfg}")
            };
            return if en {
                format!("{head}Launch options could not be updated: {message}")
            } else {
                format!("{head}修改启动项失败：{message}")
            };
        }
    };
    let lead = if install {
        if en {
            "Setup was not fully installed. "
        } else {
            "安装没有完成。"
        }
    } else if en {
        "Removal did not finish. "
    } else {
        "卸载没有完成。"
    };
    format!("{lead}{cfg} {launch}")
}

fn meter(
    ui: &mut egui::Ui,
    title: &str,
    remaining: f64,
    total: f64,
    accent: Color32,
    ready: Option<&str>,
) {
    if remaining <= 0.05 {
        if let Some(ready) = ready {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).color(READY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(ready).color(READY));
                });
            });
            return;
        }
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format_remaining(remaining)).color(MUTED));
        });
    });
    bar_icon(ui, model::ratio(remaining, total), accent);
}

fn bar_icon(ui: &mut egui::Ui, ratio: f32, accent: Color32) {
    let width = ui.available_width().max(80.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, TRACK);
    let mut fill = rect;
    fill.set_width(rect.width() * ratio.clamp(0.0, 1.0));
    if fill.width() > 0.0 {
        painter.rect_filled(fill, 3.0, accent);
    }
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
    ctx.all_styles_mut(|style| {
        // 文字不参与选择，按在说明上才能拖动窗口。
        style.interaction.selectable_labels = false;
    });
}
