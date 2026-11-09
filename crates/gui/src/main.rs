#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Drumfire desktop UI (egui/eframe).

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use drumfire_core::{
    classify, initial_win_probability, simulate, strength_breakdown, BattleLog, Ending, RoundLog,
    Side, Winner,
};
use drumfire_model::{
    BattleConfig, BattleInput, ForceCount, Mode, Morale, Rating, Seed, TeamInput,
};
use drumfire_ns::{suggested_forces, NationSummary};
use eframe::egui;

fn main() -> eframe::Result<()> {
    let demo =
        std::env::args().any(|arg| arg == "--demo") || std::env::var_os("DRUMFIRE_DEMO").is_some();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Drumfire",
        options,
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx, Theme::Paper);
            let app = if demo {
                DrumfireApp::demo()
            } else {
                DrumfireApp::default()
            };
            Ok(Box::new(app))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HomeSide {
    Neutral,
    Attacker,
    Defender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    Paper,
    Night,
}

struct Palette {
    bg: egui::Color32,
    panel: egui::Color32,
    panel_alt: egui::Color32,
    ink: egui::Color32,
    muted: egui::Color32,
    line: egui::Color32,
    sand: egui::Color32,
    sand_soft: egui::Color32,
    blue: egui::Color32,
    red: egui::Color32,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Paper => Palette {
            bg: egui::Color32::from_rgb(239, 233, 221),
            panel: egui::Color32::from_rgb(251, 247, 239),
            panel_alt: egui::Color32::from_rgb(245, 239, 228),
            ink: egui::Color32::from_rgb(43, 42, 36),
            muted: egui::Color32::from_rgb(123, 116, 100),
            line: egui::Color32::from_rgb(217, 208, 191),
            sand: egui::Color32::from_rgb(176, 122, 46),
            sand_soft: egui::Color32::from_rgb(234, 215, 184),
            blue: egui::Color32::from_rgb(63, 111, 140),
            red: egui::Color32::from_rgb(162, 75, 63),
        },
        Theme::Night => Palette {
            bg: egui::Color32::from_rgb(13, 18, 21),
            panel: egui::Color32::from_rgb(21, 29, 33),
            panel_alt: egui::Color32::from_rgb(26, 36, 41),
            ink: egui::Color32::from_rgb(233, 227, 214),
            muted: egui::Color32::from_rgb(141, 154, 161),
            line: egui::Color32::from_rgb(42, 55, 61),
            sand: egui::Color32::from_rgb(214, 164, 93),
            sand_soft: egui::Color32::from_rgb(58, 47, 29),
            blue: egui::Color32::from_rgb(119, 169, 201),
            red: egui::Color32::from_rgb(201, 122, 109),
        },
    }
}

fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let p = palette(theme);
    let mut style = (*ctx.style()).clone();
    style.visuals = if theme == Theme::Paper {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    style.visuals.panel_fill = p.bg;
    style.visuals.window_fill = p.panel;
    style.visuals.extreme_bg_color = p.panel_alt;
    style.visuals.faint_bg_color = p.panel_alt;
    style.visuals.override_text_color = Some(p.ink);
    style.visuals.hyperlink_color = p.blue;
    style.visuals.selection.bg_fill = p.sand_soft;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, p.sand);
    style.visuals.widgets.noninteractive.bg_fill = p.panel;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.line);
    style.visuals.widgets.inactive.bg_fill = p.panel_alt;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, p.line);
    style.visuals.widgets.hovered.bg_fill = p.sand_soft;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, p.sand);
    style.visuals.widgets.active.bg_fill = p.sand_soft;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.sand);
    style.visuals.window_stroke = egui::Stroke::new(1.0, p.line);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.slider_width = 200.0;
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(26.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    ctx.set_style(style);
}

fn card(p: &Palette) -> egui::Frame {
    egui::Frame::NONE
        .fill(p.panel)
        .stroke(egui::Stroke::new(1.0, p.line))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(16))
}

fn tiny(text: impl Into<String>, p: &Palette) -> egui::RichText {
    egui::RichText::new(text.into()).size(10.0).color(p.muted)
}

#[derive(Debug, Clone)]
struct TeamForm {
    name: String,
    forces: String,
    morale: i32,
    tech: u8,
    training: u8,
    experience: u8,
    logistics: u8,
    intel: u8,
    flavor: Option<NationSummary>,
}

impl TeamForm {
    fn from_team(team: &TeamInput) -> Self {
        Self {
            name: team.name.clone(),
            forces: team.forces.get().to_string(),
            morale: team.morale.get() as i32,
            tech: team.tech.get(),
            training: team.training.get(),
            experience: team.experience.get(),
            logistics: team.logistics.get(),
            intel: team.intel.get(),
            flavor: None,
        }
    }

    fn blank(label: &str) -> Self {
        Self {
            name: label.to_string(),
            forces: "10000".to_string(),
            morale: 70,
            tech: 5,
            training: 5,
            experience: 5,
            logistics: 5,
            intel: 5,
            flavor: None,
        }
    }

    fn to_team(&self) -> Result<TeamInput, String> {
        let forces_text = self.forces.trim().replace([',', '_'], "");
        let forces: u32 = forces_text
            .parse()
            .map_err(|_| format!("forces must be a whole number, got {:?}", self.forces))?;
        let forces = ForceCount::new(forces).map_err(|err| err.to_string())?;
        let morale = Morale::new(self.morale.clamp(0, 100) as u8).map_err(|err| err.to_string())?;

        let team = TeamInput {
            name: self.name.clone(),
            forces,
            morale,
            tech: Rating::new(self.tech).map_err(|err| err.to_string())?,
            training: Rating::new(self.training).map_err(|err| err.to_string())?,
            experience: Rating::new(self.experience).map_err(|err| err.to_string())?,
            logistics: Rating::new(self.logistics).map_err(|err| err.to_string())?,
            intel: Rating::new(self.intel).map_err(|err| err.to_string())?,
            home: false,
        };
        team.validate().map_err(|err| err.to_string())?;
        Ok(team)
    }
}

struct DrumfireApp {
    theme: Theme,
    applied_theme: Option<Theme>,
    selected_phase: Option<usize>,
    mode: Mode,
    attacker: TeamForm,
    defender: TeamForm,
    config: BattleConfig,
    seed_hex: String,
    home_side: HomeSide,
    log: Option<BattleLog>,
    error: Option<String>,
    import_name: String,
    import_target: Side,
    import_rx: Option<Receiver<Result<NationSummary, String>>>,
    import_status: Option<String>,
    import_error: Option<String>,
}

impl Default for DrumfireApp {
    fn default() -> Self {
        Self::from_input(&example_input())
    }
}

impl DrumfireApp {
    fn demo() -> Self {
        let mut app = Self::default();
        app.run_battle();
        app
    }

    fn from_input(input: &BattleInput) -> Self {
        let home_side = if input.attacker.home {
            HomeSide::Attacker
        } else if input.defender.home {
            HomeSide::Defender
        } else {
            HomeSide::Neutral
        };

        Self {
            theme: Theme::Paper,
            applied_theme: None,
            selected_phase: None,
            mode: input.mode,
            attacker: TeamForm::from_team(&input.attacker),
            defender: TeamForm::from_team(&input.defender),
            config: input.config.clone(),
            seed_hex: input.seed.to_hex(),
            home_side,
            log: None,
            error: None,
            import_name: String::new(),
            import_target: Side::Attacker,
            import_rx: None,
            import_status: None,
            import_error: None,
        }
    }

    fn build_input(&self) -> Result<BattleInput, String> {
        let mut attacker = self
            .attacker
            .to_team()
            .map_err(|err| format!("Attacker: {err}"))?;
        let mut defender = self
            .defender
            .to_team()
            .map_err(|err| format!("Defender: {err}"))?;

        attacker.home = self.home_side == HomeSide::Attacker;
        defender.home = self.home_side == HomeSide::Defender;

        let seed = Seed::from_hex(&self.seed_hex).map_err(|err| format!("Seed: {err}"))?;

        let input = BattleInput {
            mode: self.mode,
            attacker,
            defender,
            config: self.config.clone(),
            seed,
        };
        input.validate().map_err(|err| err.to_string())?;
        Ok(input)
    }

    fn run_battle(&mut self) {
        match self.build_input() {
            Ok(input) => match simulate(&input) {
                Ok(log) => {
                    let middle = log.rounds.len() / 2;
                    self.selected_phase = Some(middle);
                    self.log = Some(log);
                    self.error = None;
                }
                Err(err) => {
                    self.error = Some(err.to_string());
                    self.log = None;
                }
            },
            Err(err) => {
                self.error = Some(err);
                self.log = None;
            }
        }
    }

    fn new_seed(&mut self) {
        let mut bytes = [0u8; 32];
        if getrandom::getrandom(&mut bytes).is_ok() {
            self.seed_hex = Seed::from_bytes(bytes).to_hex();
        }
    }

    fn load_example(&mut self) {
        let input = example_input();
        self.mode = input.mode;
        self.attacker = TeamForm::from_team(&input.attacker);
        self.defender = TeamForm::from_team(&input.defender);
        self.config = input.config;
        self.seed_hex = input.seed.to_hex();
        self.home_side = if input.attacker.home {
            HomeSide::Attacker
        } else if input.defender.home {
            HomeSide::Defender
        } else {
            HomeSide::Neutral
        };
        self.log = None;
        self.error = None;
        self.selected_phase = None;
    }

    fn reset(&mut self) {
        self.mode = Mode::Battle;
        self.attacker = TeamForm::blank("Attacker");
        self.defender = TeamForm::blank("Defender");
        self.config = BattleConfig::default();
        self.home_side = HomeSide::Neutral;
        self.seed_hex = String::new();
        self.new_seed();
        self.log = None;
        self.error = None;
        self.selected_phase = None;
        self.import_status = None;
        self.import_error = None;
    }

    fn start_import(&mut self) {
        if self.import_rx.is_some() {
            return;
        }
        let name = self.import_name.trim().to_string();
        if name.is_empty() {
            self.import_error = Some("Enter a nation name first.".to_string());
            return;
        }

        let (tx, rx) = mpsc::channel();
        let status_name = name.clone();
        std::thread::spawn(move || {
            let result = drumfire_ns::fetch_nation(&name).map_err(|err| err.to_string());
            let _ = tx.send(result);
        });

        self.import_rx = Some(rx);
        self.import_status = Some(format!("Fetching {status_name}…"));
        self.import_error = None;
    }

    fn poll_import(&mut self, ctx: &egui::Context) {
        let mut disconnected = false;
        let received = if let Some(rx) = &self.import_rx {
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    None
                }
                Err(mpsc::TryRecvError::Empty) => None,
            }
        } else {
            None
        };

        if disconnected {
            self.import_rx = None;
            self.import_status = None;
            self.import_error = Some("The import worker stopped unexpectedly.".to_string());
        }

        if let Some(result) = received {
            self.import_rx = None;
            match result {
                Ok(nation) => self.apply_nation(nation),
                Err(err) => {
                    self.import_status = None;
                    self.import_error = Some(err);
                }
            }
        }

        if self.import_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn apply_nation(&mut self, nation: NationSummary) {
        let forces = suggested_forces(nation.population).to_string();
        let label = nation.name.clone();
        let population = nation.population;

        let team = match self.import_target {
            Side::Attacker => &mut self.attacker,
            Side::Defender => &mut self.defender,
        };
        team.name = nation.name.clone();
        team.forces = forces;
        team.flavor = Some(nation);

        self.import_status = Some(format!(
            "Imported {label} ({population} residents). Force count is a suggestion; edit freely."
        ));
        self.import_error = None;
    }
}

impl eframe::App for DrumfireApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.applied_theme != Some(self.theme) {
            apply_theme(ctx, self.theme);
            self.applied_theme = Some(self.theme);
        }
        self.poll_import(ctx);
        let p = palette(self.theme);

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::NONE
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("DRUMFIRE")
                            .size(19.0)
                            .strong()
                            .color(p.ink),
                    );
                    ui.label(
                        egui::RichText::new("battle resolution for NationStates roleplay")
                            .size(11.0)
                            .color(p.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.selectable_value(&mut self.theme, Theme::Night, "Night");
                        ui.selectable_value(&mut self.theme, Theme::Paper, "Paper");
                        ui.label(tiny("theme", &p));
                    });
                });
            });

        egui::SidePanel::left("setup")
            .resizable(true)
            .default_width(430.0)
            .width_range(350.0..=560.0)
            .frame(
                egui::Frame::NONE
                    .fill(p.panel_alt)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading("Battle setup");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(tiny("no credentials", &p));
                                },
                            );
                        });
                        ui.add_space(2.0);
                        self.mode_ui(ui);
                        ui.add_space(8.0);
                        self.seed_ui(ui);
                        ui.add_space(6.0);
                        self.home_ui(ui);
                        ui.add_space(10.0);
                        card(&p).show(ui, |ui| {
                            team_ui(ui, "Attacker", &mut self.attacker, self.mode, &p);
                        });
                        ui.add_space(8.0);
                        card(&p).show(ui, |ui| {
                            team_ui(ui, "Defender", &mut self.defender, self.mode, &p);
                        });
                        ui.add_space(8.0);
                        self.config_ui(ui);
                        ui.add_space(8.0);
                        self.actions_ui(ui, &p);
                        ui.add_space(8.0);
                        self.import_ui(ui);
                        ui.add_space(14.0);
                    });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.output_ui(ui, &p);
                    });
            });
    }
}

impl DrumfireApp {
    fn mode_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Mode");
            for mode in [Mode::Skirmish, Mode::Battle] {
                if ui
                    .selectable_label(self.mode == mode, mode.label())
                    .clicked()
                {
                    self.mode = mode;
                    self.log = None;
                    self.error = None;
                }
            }
        });
        ui.label(
            egui::RichText::new(
                "Dogfight, Naval, Frontlines, and Campaign are planned; their schemas are reserved.",
            )
            .small()
            .weak(),
        );
    }

    fn seed_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Seed");
            ui.add(
                egui::TextEdit::singleline(&mut self.seed_hex)
                    .desired_width(360.0)
                    .font(egui::TextStyle::Monospace),
            );
            if ui
                .button("New")
                .on_hover_text("Generate a fresh 32-byte seed")
                .clicked()
            {
                self.new_seed();
            }
        });
        ui.label(
            egui::RichText::new("Publish the seed with the battle so others can replay it.")
                .small()
                .weak(),
        );
    }

    fn home_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Home side");
            ui.selectable_value(&mut self.home_side, HomeSide::Neutral, "Neutral");
            ui.selectable_value(&mut self.home_side, HomeSide::Attacker, "Attacker");
            ui.selectable_value(&mut self.home_side, HomeSide::Defender, "Defender");
        });
    }

    fn config_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Advanced rules")
            .default_open(false)
            .show(ui, |ui| {
                ui.add(egui::Slider::new(&mut self.config.max_rounds, 1..=100).text("Max rounds"));
                ui.add(
                    egui::Slider::new(&mut self.config.scale, 0.1..=20.0)
                        .text("Scale")
                        .fixed_decimals(2),
                );
                ui.add(
                    egui::Slider::new(&mut self.config.home_bonus, -2.0..=2.0)
                        .text("Home bonus")
                        .fixed_decimals(2),
                );
                ui.add(
                    egui::Slider::new(&mut self.config.casualty_rate, 0.0..=0.5)
                        .text("Casualty rate")
                        .fixed_decimals(2),
                );
                ui.add(
                    egui::Slider::new(&mut self.config.morale_rout_threshold, 0..=100)
                        .text("Rout threshold"),
                );
                ui.label(
                    egui::RichText::new(
                        "Defaults follow rules v1. Changing these is a house rule.",
                    )
                    .small()
                    .weak(),
                );
            });
    }

    fn actions_ui(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal(|ui| {
            let run = egui::Button::new(
                egui::RichText::new("Run battle")
                    .strong()
                    .color(egui::Color32::from_rgb(34, 24, 13)),
            )
            .fill(p.sand);
            if ui.add(run).clicked() {
                self.run_battle();
            }
            if ui.button("Example").clicked() {
                self.load_example();
            }
            if ui.button("Reset").clicked() {
                self.reset();
            }
        });

        if let Some(error) = &self.error {
            ui.add_space(4.0);
            ui.colored_label(p.red, error);
        }
    }

    fn import_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Import from NationStates");
        ui.label(
            egui::RichText::new(
                "Public shards only. This reads a nation; it never sends a password, PIN, or key.",
            )
            .small()
            .weak(),
        );
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.import_name)
                    .hint_text("nation name")
                    .desired_width(220.0),
            );
            let button = egui::Button::new("Fetch");
            if ui.add_enabled(self.import_rx.is_none(), button).clicked() {
                self.start_import();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Apply to");
            ui.selectable_value(&mut self.import_target, Side::Attacker, "Attacker");
            ui.selectable_value(&mut self.import_target, Side::Defender, "Defender");
        });

        if let Some(status) = &self.import_status {
            ui.label(egui::RichText::new(status).small());
        }
        if let Some(error) = &self.import_error {
            ui.colored_label(egui::Color32::from_rgb(235, 110, 110), error);
        }

        ui.label(
            egui::RichText::new(
                "Force count is suggested as population / 1,000. It is a roleplay starting point, not a NationStates combat stat.",
            )
            .small()
            .weak(),
        );
    }

    fn output_ui(&mut self, ui: &mut egui::Ui, p: &Palette) {
        if let Some(error) = &self.error {
            ui.colored_label(p.red, error);
            ui.add_space(8.0);
        }

        let Some(log) = self.log.clone() else {
            self.empty_state(ui, p);
            return;
        };

        if self
            .selected_phase
            .is_none_or(|index| index >= log.rounds.len())
        {
            self.selected_phase = Some(log.rounds.len() / 2);
        }

        self.report_header(ui, p, &log);
        ui.add_space(12.0);
        self.momentum_card(ui, p, &log);
        ui.add_space(12.0);
        self.phase_strip(ui, p, &log);
        ui.add_space(12.0);
        self.phase_detail(ui, p, &log);
        ui.add_space(12.0);
        self.factors_card(ui, p, &log);
        ui.add_space(8.0);
        self.log_table(ui, p, &log);
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.add_space(8.0);
        card(p).show(ui, |ui| {
            ui.label(tiny("AFTER-ACTION REPORT", p));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Nothing has been resolved yet.")
                    .size(30.0)
                    .strong()
                    .color(p.ink),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Pick a mode, set two sides, and run. The report will show momentum, every phase, and the reason behind the result.",
                )
                .size(13.0)
                .color(p.muted),
            );
            ui.add_space(12.0);
            let run = egui::Button::new(
                egui::RichText::new("Run the example")
                    .strong()
                    .color(egui::Color32::from_rgb(34, 24, 13)),
            )
            .fill(p.sand);
            if ui.add(run).clicked() {
                self.load_example();
                self.run_battle();
            }
        });

        ui.add_space(12.0);
        ui.columns(3, |columns| {
            for (column, (title, body)) in columns.iter_mut().zip([
                (
                    "1 · Choose a mode",
                    "Skirmish for a small-unit fight; Battle for a large engagement.",
                ),
                (
                    "2 · Set the sides",
                    "Forces, morale, and the five ratings. Import a nation if you want a starting point.",
                ),
                (
                    "3 · Read the report",
                    "Momentum, phase cards, a selected-phase explanation, and the full audit table.",
                ),
            ]) {
                card(p).show(column, |ui| {
                    ui.label(egui::RichText::new(title).strong().color(p.ink));
                    ui.add_space(3.0);
                    ui.label(egui::RichText::new(body).size(11.5).color(p.muted));
                });
            }
        });
    }

    fn report_header(&mut self, ui: &mut egui::Ui, p: &Palette, log: &BattleLog) {
        ui.horizontal(|ui| {
            ui.label(tiny("AFTER-ACTION REPORT", p));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Copy log JSON").clicked() {
                    if let Ok(json) = serde_json::to_string_pretty(log) {
                        ui.ctx().copy_text(json);
                    }
                }
                ui.label(tiny(format!("rules v1 · {}", log.input.mode.label()), p));
            });
        });

        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(&log.input.attacker.name)
                    .size(30.0)
                    .strong()
                    .color(p.sand),
            );
            ui.label(egui::RichText::new("vs").size(12.0).color(p.muted));
            ui.label(
                egui::RichText::new(&log.input.defender.name)
                    .size(30.0)
                    .strong()
                    .color(p.blue),
            );
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(winner_label(log))
                    .size(18.0)
                    .strong()
                    .color(p.ink),
            );
            badge(ui, p, classify(log).label(), p.sand_soft, p.sand);
            ui.label(tiny(
                format!(
                    "{} {} · {}",
                    log.rounds.len(),
                    log.input.mode.phase_label_plural().to_lowercase(),
                    ending_label(log.result.ending, log.input.mode)
                ),
                p,
            ));
        });
        ui.label(
            egui::RichText::new(format!(
                "seed {}   ·   hash {}",
                log.input.seed.to_hex(),
                log.input_hash
            ))
            .size(9.5)
            .monospace()
            .color(p.muted),
        );
    }

    fn momentum_card(&mut self, ui: &mut egui::Ui, p: &Palette, log: &BattleLog) {
        card(p).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tiny("MOMENTUM", p));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(tiny("final force share", p));
                });
            });

            let a = log.result.final_forces.attacker as f32;
            let d = log.result.final_forces.defender as f32;
            let share = if a + d > 0.0 { a / (a + d) } else { 0.5 };

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:.0}%", share * 100.0))
                        .strong()
                        .color(p.sand),
                );
                ui.label(tiny(&log.input.attacker.name, p));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", (1.0 - share) * 100.0))
                            .strong()
                            .color(p.blue),
                    );
                    ui.label(tiny(&log.input.defender.name, p));
                });
            });

            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
            let painter = ui.painter();
            let corner = egui::CornerRadius::same(6);
            painter.rect_filled(rect, corner, p.panel_alt);
            let split = rect.left() + rect.width() * share;
            painter.rect_filled(
                egui::Rect::from_min_max(rect.min, egui::pos2(split, rect.max.y)),
                corner,
                p.sand,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(split, rect.min.y), rect.max),
                corner,
                p.blue,
            );
            painter.line_segment(
                [
                    egui::pos2(split, rect.top()),
                    egui::pos2(split, rect.bottom()),
                ],
                egui::Stroke::new(1.0, p.panel),
            );

            ui.add_space(4.0);
            ui.label(tiny("attacker win probability by phase", p));
            let (spark, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 74.0), egui::Sense::hover());
            paint_sparkline(ui.painter(), spark, &log.rounds, self.selected_phase, p);
        });
    }

    fn phase_strip(&mut self, ui: &mut egui::Ui, p: &Palette, log: &BattleLog) {
        card(p).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tiny("PHASES", p));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(tiny("click to inspect", p));
                });
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                for (index, round) in log.rounds.iter().enumerate() {
                    let selected = self.selected_phase == Some(index);
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(58.0, 54.0), egui::Sense::click());
                    let fill = if selected {
                        p.sand_soft
                    } else if response.hovered() {
                        p.panel
                    } else {
                        p.panel_alt
                    };
                    ui.painter()
                        .rect_filled(rect, egui::CornerRadius::same(7), fill);
                    let dot = match round.round_winner {
                        Side::Attacker => p.sand,
                        Side::Defender => p.blue,
                    };
                    ui.painter().circle_filled(
                        egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
                        3.0,
                        dot,
                    );
                    ui.painter().text(
                        egui::pos2(rect.left() + 18.0, rect.top() + 9.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{}", round.round),
                        egui::FontId::proportional(9.0),
                        p.muted,
                    );
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.top() + 29.0),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", round.attacker_casualties + round.defender_casualties),
                        egui::FontId::proportional(13.0),
                        p.ink,
                    );
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.top() + 44.0),
                        egui::Align2::CENTER_CENTER,
                        format!("{:.0}%", round.attacker_win_probability * 100.0),
                        egui::FontId::proportional(8.0),
                        p.muted,
                    );
                    if response.clicked() {
                        self.selected_phase = Some(index);
                    }
                    response.on_hover_text(format!(
                        "{} {} · {} / {} losses · {:.1}% attacker",
                        log.input.mode.phase_label(),
                        round.round,
                        round.attacker_casualties,
                        round.defender_casualties,
                        round.attacker_win_probability * 100.0
                    ));
                }
            });
        });
    }

    fn phase_detail(&mut self, ui: &mut egui::Ui, p: &Palette, log: &BattleLog) {
        let Some(index) = self
            .selected_phase
            .filter(|index| *index < log.rounds.len())
        else {
            return;
        };
        let round = &log.rounds[index];
        let winner = match round.round_winner {
            Side::Attacker => &log.input.attacker.name,
            Side::Defender => &log.input.defender.name,
        };
        let headline = match round.round_winner {
            Side::Attacker => format!("{winner} presses the attack"),
            Side::Defender => format!("{winner} holds the line"),
        };
        let force = log.input.mode.force_label().to_lowercase();

        card(p).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tiny(
                    format!(
                        "{} {} · {}",
                        log.input.mode.phase_label().to_uppercase(),
                        round.round,
                        winner
                    ),
                    p,
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(tiny(
                        format!(
                            "roll {:.3} · p {:.1}%",
                            round.roll,
                            round.attacker_win_probability * 100.0
                        ),
                        p,
                    ));
                });
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(headline)
                    .size(21.0)
                    .strong()
                    .color(p.ink),
            );
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} loses {} {}; {} loses {}. Morale is now {} / {}.",
                    log.input.attacker.name,
                    round.attacker_casualties,
                    force,
                    log.input.defender.name,
                    round.defender_casualties,
                    round.attacker_morale_after,
                    round.defender_morale_after
                ))
                .size(12.5)
                .color(p.muted),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                badge(
                    ui,
                    p,
                    format!(
                        "losses {} / {}",
                        round.attacker_casualties, round.defender_casualties
                    ),
                    p.panel_alt,
                    p.ink,
                );
                badge(
                    ui,
                    p,
                    format!(
                        "forces {} / {}",
                        round.attacker_forces_after, round.defender_forces_after
                    ),
                    p.panel_alt,
                    p.ink,
                );
                badge(
                    ui,
                    p,
                    format!(
                        "morale {} / {}",
                        round.attacker_morale_after, round.defender_morale_after
                    ),
                    p.panel_alt,
                    p.ink,
                );
            });
        });
    }

    fn factors_card(&mut self, ui: &mut egui::Ui, p: &Palette, log: &BattleLog) {
        let a = strength_breakdown(
            &log.input.attacker,
            log.input.attacker.forces.get(),
            log.input.attacker.morale.get(),
            &log.input.config,
            log.input.mode,
        );
        let d = strength_breakdown(
            &log.input.defender,
            log.input.defender.forces.get(),
            log.input.defender.morale.get(),
            &log.input.config,
            log.input.mode,
        );

        card(p).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tiny("WHY THIS PHASE", p));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(tiny("opening strength contributions", p));
                });
            });
            ui.add_space(3.0);
            factor_row(ui, p, "Forces", a.forces, d.forces);
            factor_row(ui, p, "Morale", a.morale, d.morale);
            factor_row(ui, p, "Technology", a.tech, d.tech);
            factor_row(ui, p, "Training", a.training, d.training);
            factor_row(ui, p, "Experience", a.experience, d.experience);
            factor_row(ui, p, "Logistics", a.logistics, d.logistics);
            factor_row(ui, p, "Intel", a.intel, d.intel);
            factor_row(ui, p, "Home", a.home, d.home);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tiny("TOTAL", p));
                ui.label(
                    egui::RichText::new(format!("{:+.2} / {:+.2}", a.total, d.total))
                        .strong()
                        .color(p.ink),
                );
                if let Ok(probability) = initial_win_probability(&log.input) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(tiny(
                            format!(
                                "opening attack chance {:.1}% · {}",
                                probability * 100.0,
                                log.input.attacker.name
                            ),
                            p,
                        ));
                    });
                }
            });
        });
    }

    fn log_table(&mut self, ui: &mut egui::Ui, _p: &Palette, log: &BattleLog) {
        egui::CollapsingHeader::new("Full phase log")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            egui::Grid::new("rounds")
                                .striped(true)
                                .num_columns(6)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    for header in [
                                        log.input.mode.phase_label(),
                                        "Winner",
                                        "A win p",
                                        "Losses A/D",
                                        "Forces A/D",
                                        "Morale A/D",
                                    ] {
                                        ui.strong(header);
                                    }
                                    ui.end_row();

                                    for r in &log.rounds {
                                        ui.monospace(r.round.to_string());
                                        ui.label(match r.round_winner {
                                            Side::Attacker => &log.input.attacker.name,
                                            Side::Defender => &log.input.defender.name,
                                        });
                                        ui.monospace(format!(
                                            "{:.1}%",
                                            r.attacker_win_probability * 100.0
                                        ));
                                        ui.monospace(format!(
                                            "{} / {}",
                                            r.attacker_casualties, r.defender_casualties
                                        ));
                                        ui.monospace(format!(
                                            "{} / {}",
                                            r.attacker_forces_after, r.defender_forces_after
                                        ));
                                        ui.monospace(format!(
                                            "{} / {}",
                                            r.attacker_morale_after, r.defender_morale_after
                                        ));
                                        ui.end_row();
                                    }
                                });
                        });
                    });
            });
    }
}

fn team_ui(ui: &mut egui::Ui, label: &str, form: &mut TeamForm, mode: Mode, p: &Palette) {
    ui.spacing_mut().slider_width = 210.0;
    ui.horizontal(|ui| {
        ui.heading(label);
        if form.flavor.is_some() {
            ui.label(
                egui::RichText::new("imported")
                    .small()
                    .color(egui::Color32::from_rgb(120, 150, 110)),
            );
        }
    });

    egui::Grid::new(format!("{label}_identity"))
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(280.0));
            ui.end_row();

            ui.label(mode.force_label());
            ui.add(
                egui::TextEdit::singleline(&mut form.forces)
                    .desired_width(130.0)
                    .font(egui::TextStyle::Monospace),
            );
            ui.end_row();
        });

    ui.add(egui::Slider::new(&mut form.morale, 0..=100).text("Morale"));
    ui.add_space(3.0);
    egui::Grid::new(format!("{label}_ratings"))
        .num_columns(5)
        .spacing([6.0, 3.0])
        .show(ui, |ui| {
            for name in ["Tech", "Train", "Exp", "Log", "Intel"] {
                ui.label(egui::RichText::new(name).size(9.0).color(p.muted));
            }
            ui.end_row();
            for value in [
                &mut form.tech,
                &mut form.training,
                &mut form.experience,
                &mut form.logistics,
                &mut form.intel,
            ] {
                ui.add(egui::DragValue::new(value).range(1..=10).speed(1.0));
            }
        });

    if let Some(flavor) = &form.flavor {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "{} · {} residents · {}",
                flavor.region, flavor.population, flavor.category
            ))
            .small()
            .weak(),
        );
        if !flavor.motto.is_empty() {
            ui.label(
                egui::RichText::new(format!("“{}”", flavor.motto))
                    .small()
                    .italics()
                    .weak(),
            );
        }
        if flavor.flag.starts_with("http") {
            ui.hyperlink_to("Flag", flavor.flag.as_str());
        }
    }
}

fn badge(
    ui: &mut egui::Ui,
    _p: &Palette,
    text: impl Into<String>,
    fill: egui::Color32,
    color: egui::Color32,
) {
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text.into()).size(10.0).color(color));
        });
}

fn factor_row(ui: &mut egui::Ui, p: &Palette, label: &str, a: f64, d: f64) {
    let bar_width = (ui.available_width() - 190.0).clamp(80.0, 320.0);
    ui.horizontal(|ui| {
        ui.add_sized([78.0, 16.0], egui::Label::new(tiny(label, p)));
        let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_width, 8.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, egui::CornerRadius::same(4), p.panel_alt);
        let max = a.abs().max(d.abs()).max(0.05);
        let mid = rect.center().x;
        let half = rect.width() / 2.0;
        let aw = (a.abs() / max) as f32 * half;
        let dw = (d.abs() / max) as f32 * half;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(mid - aw, rect.top()),
                egui::pos2(mid, rect.bottom()),
            ),
            egui::CornerRadius::same(4),
            p.sand,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(mid, rect.top()),
                egui::pos2(mid + dw, rect.bottom()),
            ),
            egui::CornerRadius::same(4),
            p.blue,
        );
        ui.add_sized(
            [104.0, 16.0],
            egui::Label::new(
                egui::RichText::new(format!("{a:+.2} / {d:+.2}"))
                    .size(10.0)
                    .monospace()
                    .color(p.muted),
            ),
        );
    });
}

fn paint_sparkline(
    painter: &egui::Painter,
    rect: egui::Rect,
    rounds: &[RoundLog],
    selected: Option<usize>,
    p: &Palette,
) {
    painter.rect_filled(rect, egui::CornerRadius::same(7), p.panel_alt);
    if rounds.is_empty() {
        return;
    }

    let mut lo = 1.0_f64;
    let mut hi = 0.0_f64;
    for round in rounds {
        lo = lo.min(round.attacker_win_probability);
        hi = hi.max(round.attacker_win_probability);
    }
    if hi - lo < 0.10 {
        let mid = (hi + lo) / 2.0;
        lo = (mid - 0.05).max(0.0);
        hi = (mid + 0.05).min(1.0);
    }

    if lo <= 0.5 && hi >= 0.5 {
        let y = rect.bottom() - rect.height() * ((0.5 - lo) / (hi - lo)) as f32;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, p.line),
        );
    }

    let count = rounds.len() as f32;
    let points: Vec<egui::Pos2> = rounds
        .iter()
        .enumerate()
        .map(|(index, round)| {
            let x = rect.left() + rect.width() * (index as f32 + 0.5) / count;
            let t = ((round.attacker_win_probability - lo) / (hi - lo)) as f32;
            let y = rect.bottom() - rect.height() * t;
            egui::pos2(x, y)
        })
        .collect();

    painter.add(egui::Shape::line(
        points.clone(),
        egui::Stroke::new(2.0, p.blue),
    ));
    for point in &points {
        painter.circle_filled(*point, 2.0, p.blue);
    }
    if let Some(index) = selected.and_then(|index| points.get(index)) {
        painter.line_segment(
            [
                egui::pos2(index.x, rect.top()),
                egui::pos2(index.x, rect.bottom()),
            ],
            egui::Stroke::new(1.0, p.sand),
        );
        painter.circle_filled(*index, 4.0, p.sand);
    }

    painter.text(
        egui::pos2(rect.left() + 5.0, rect.top() + 4.0),
        egui::Align2::LEFT_TOP,
        format!("{:.0}%", hi * 100.0),
        egui::FontId::proportional(8.0),
        p.muted,
    );
    painter.text(
        egui::pos2(rect.left() + 5.0, rect.bottom() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{:.0}%", lo * 100.0),
        egui::FontId::proportional(8.0),
        p.muted,
    );
}

fn winner_label(log: &BattleLog) -> String {
    match log.result.winner {
        Winner::Attacker => format!("{} wins", log.input.attacker.name),
        Winner::Defender => format!("{} wins", log.input.defender.name),
        Winner::Draw => "Draw".to_string(),
    }
}

fn ending_label(ending: Ending, mode: Mode) -> String {
    match ending {
        Ending::Annihilation => "annihilation".to_string(),
        Ending::Rout => "rout".to_string(),
        Ending::MaxRounds => format!("{} cap", mode.phase_label().to_lowercase()),
        Ending::Draw => "draw".to_string(),
    }
}

fn example_input() -> BattleInput {
    serde_json::from_str(include_str!("../../../fixtures/example.json"))
        .expect("committed example fixture must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_battle_populates_the_report() {
        let mut app = DrumfireApp::default();
        app.run_battle();
        assert!(app.error.is_none());
        assert!(app.log.is_some());
    }

    #[test]
    fn invalid_seed_is_reported_not_panicked() {
        let mut app = DrumfireApp {
            seed_hex: "not-hex".to_string(),
            ..DrumfireApp::default()
        };
        app.run_battle();
        assert!(app.log.is_none());
        assert!(app
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Seed")));
    }

    #[test]
    fn skirmish_mode_runs() {
        let mut app = DrumfireApp {
            mode: Mode::Skirmish,
            ..DrumfireApp::default()
        };
        app.run_battle();
        assert!(app.error.is_none());
        assert_eq!(app.log.unwrap().input.mode, Mode::Skirmish);
    }
}
