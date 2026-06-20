#![cfg_attr(windows, windows_subsystem = "windows")]

mod platform;

use eframe::egui;
use serde::Deserialize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use typetext_core::{
    export_snippets, import_droptext_ini, load_or_create_settings, load_or_create_snippets,
    save_settings, save_snippets, search_snippets, AppSettings, PortablePaths,
    QueuedSnippetClickAction, SearchResult, Snippet, SnippetFile, SnippetGroup,
};

const APP_VERSION: &str = env!("TYPETEXT_APP_VERSION");
const APP_TITLE: &str = concat!("TypeText ", env!("TYPETEXT_APP_VERSION"));
const OFFLINE_PORTABLE: bool = cfg!(all(windows, feature = "offline-portable"));
const UPDATE_CHECK_INTERVAL_SECONDS: u64 = 60 * 60 * 24;
const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/Joshndroid/TypeText/releases/latest";

fn main() -> eframe::Result {
    if let Err(error) = platform::install_app_mutex() {
        eprintln!("Could not install app mutex: {error}");
    }

    let icon = app_icon_data();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_inner_size([780.0, 520.0])
        .with_min_inner_size([560.0, 380.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(TypeTextApp::new(cc)))),
    )
}

fn app_icon_data() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../icon/typetext-appicon.png")).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Choose,
    Edit,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    Open,
    Settings,
    Exit,
}

struct ChainInsertion {
    title: String,
    body: String,
}

fn join_snippet_chain<'a>(
    bodies: impl IntoIterator<Item = &'a str>,
    settings: &AppSettings,
) -> String {
    let separator = if settings.start_snippets_on_new_line {
        "\n".repeat((settings.empty_lines_between_snippets + 1) as usize)
    } else {
        String::new()
    };

    bodies.into_iter().collect::<Vec<_>>().join(&separator)
}

#[derive(Debug, Clone)]
struct UpdateInfo {
    version: String,
    release_url: String,
    download_url: String,
    asset_name: String,
}

#[derive(Debug)]
enum UpdateCheckMessage {
    Available(UpdateInfo),
    Current { notify: bool },
    Failed { error: String, notify: bool },
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

struct TypeTextApp {
    paths: PortablePaths,
    snippets: SnippetFile,
    settings: AppSettings,
    results: Vec<SearchResult>,
    search: String,
    view: View,
    chooser_group: Option<usize>,
    selected_result: usize,
    selected_group: usize,
    selected_snippet: usize,
    edit_group_active: bool,
    edit_snippet_active: bool,
    edit_group_name: String,
    edit_title: String,
    edit_body: String,
    status: String,
    error_message: Option<String>,
    confirm_clear_all: bool,
    capturing_hotkey: bool,
    settings_dirty: bool,
    snippet_chain: Vec<SearchResult>,
    insert_when_focus_lost: bool,
    registered_hotkey: Option<String>,
    hotkey_tx: Sender<()>,
    hotkey_rx: Receiver<()>,
    tray_rx: Receiver<TrayCommand>,
    tray_handle: Option<platform::TrayHandle>,
    update_rx: Receiver<UpdateCheckMessage>,
    update_info: Option<UpdateInfo>,
    update_check_in_progress: bool,
    allow_quit: bool,
    show_background_notice: bool,
    background_notice_seen: bool,
}

fn parse_hex_color(value: &str) -> Option<egui::Color32> {
    let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(red, green, blue))
}

fn format_hex_color(color: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

fn accent_text_color(accent: egui::Color32) -> egui::Color32 {
    let luminance =
        (0.299 * accent.r() as f32) + (0.587 * accent.g() as f32) + (0.114 * accent.b() as f32);
    if luminance > 150.0 {
        egui::Color32::from_rgb(17, 24, 22)
    } else {
        egui::Color32::from_rgb(248, 255, 253)
    }
}

fn accent_hover_color(accent: egui::Color32, dark: bool) -> egui::Color32 {
    if dark {
        accent.gamma_multiply(1.18)
    } else {
        accent.gamma_multiply(0.92)
    }
}

fn apply_modern_style(ctx: &egui::Context, accent_hex: &str) {
    ctx.all_styles_mut(|style| {
        let dark = style.visuals.dark_mode;
        let accent =
            parse_hex_color(accent_hex).unwrap_or_else(|| egui::Color32::from_rgb(10, 126, 118));
        let accent_hover = accent_hover_color(accent, dark);
        let accent_text = accent_text_color(accent);
        let (panel, raised, raised_hover, text, weak_text, border, input_bg) = if dark {
            (
                egui::Color32::from_rgb(18, 19, 20),
                egui::Color32::from_rgb(31, 33, 34),
                egui::Color32::from_rgb(42, 45, 46),
                egui::Color32::from_rgb(234, 238, 238),
                egui::Color32::from_rgb(153, 161, 161),
                egui::Color32::from_rgb(58, 63, 64),
                egui::Color32::from_rgb(10, 11, 12),
            )
        } else {
            (
                egui::Color32::from_rgb(246, 247, 245),
                egui::Color32::from_rgb(255, 255, 253),
                egui::Color32::from_rgb(235, 241, 238),
                egui::Color32::from_rgb(32, 36, 34),
                egui::Color32::from_rgb(96, 105, 101),
                egui::Color32::from_rgb(206, 213, 209),
                egui::Color32::from_rgb(255, 255, 255),
            )
        };

        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(8);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.indent = 10.0;
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(15.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(11.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(11.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(9.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(11.5, egui::FontFamily::Monospace),
        );

        let visuals = &mut style.visuals;
        visuals.panel_fill = panel;
        visuals.window_fill = panel;
        visuals.faint_bg_color = raised;
        visuals.extreme_bg_color = input_bg;
        visuals.text_edit_bg_color = Some(input_bg);
        visuals.code_bg_color = raised;
        visuals.weak_text_color = Some(weak_text);
        visuals.hyperlink_color = accent_hover;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, accent_text);
        visuals.window_stroke = egui::Stroke::new(1.0, border);
        visuals.window_corner_radius = egui::CornerRadius::same(8);
        visuals.menu_corner_radius = egui::CornerRadius::same(8);
        visuals.button_frame = true;
        visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(6);
        }

        visuals.widgets.noninteractive.bg_fill = panel;
        visuals.widgets.noninteractive.weak_bg_fill = raised;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);

        visuals.widgets.inactive.bg_fill = raised;
        visuals.widgets.inactive.weak_bg_fill = raised;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);

        visuals.widgets.hovered.bg_fill = raised_hover;
        visuals.widgets.hovered.weak_bg_fill = raised_hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent_hover);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, text);

        visuals.widgets.active.bg_fill = accent;
        visuals.widgets.active.weak_bg_fill = accent;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent_hover);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, accent_text);

        visuals.widgets.open = visuals.widgets.hovered;
    });
}

fn apply_theme(ctx: &egui::Context, settings: &AppSettings) {
    ctx.set_theme(theme_preference(&settings.theme));
    apply_modern_style(ctx, &settings.accent_color);
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "JetBrainsMono".to_string(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMono-Regular.ttf"
        ))),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "JetBrainsMono".to_string());
    }

    ctx.set_fonts(fonts);
}

fn theme_preference(theme: &str) -> egui::ThemePreference {
    match theme.trim().to_ascii_lowercase().as_str() {
        "light" => egui::ThemePreference::Light,
        "dark" => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    }
}

fn normalize_theme(theme: &str) -> String {
    match theme.trim().to_ascii_lowercase().as_str() {
        "light" => "light".to_string(),
        "dark" => "dark".to_string(),
        _ => "system".to_string(),
    }
}

fn snippet_preview(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn title_from_body(body: &str) -> Option<String> {
    let preview = snippet_preview(body);
    if preview.is_empty() {
        return None;
    }

    let max_chars = 48;
    let mut title = preview.chars().take(max_chars).collect::<String>();
    if preview.chars().count() > max_chars {
        title.push_str("...");
    }
    Some(title)
}

fn nav_button(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    ui.add(egui::Button::selectable(selected, label).min_size(egui::vec2(68.0, 22.0)))
        .clicked()
}

fn section_header(ui: &mut egui::Ui, title: &str, meta: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .size(12.5)
                .color(ui.visuals().text_color()),
        );
        let meta = meta.into();
        if !meta.is_empty() {
            ui.label(
                egui::RichText::new(meta)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
    });
}

fn section_gap(ui: &mut egui::Ui) {
    ui.add_space(6.0);
}

fn framed_section(
    ui: &mut egui::Ui,
    title: &str,
    meta: impl Into<String>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_header(ui, title, meta);
            ui.add_space(5.0);
            add_contents(ui);
        });
}

fn compact_snippet_row(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    queued: bool,
) -> egui::Response {
    let visuals = ui.visuals();
    let fill = if queued {
        visuals.widgets.active.bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };
    let stroke = if queued || selected {
        visuals.selection.stroke
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };

    let text_color = if queued {
        visuals.selection.stroke.color
    } else {
        visuals.text_color()
    };
    let weak_color = if queued {
        visuals.selection.stroke.color
    } else {
        visuals.weak_text_color()
    };
    let frame_response = egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&result.title)
                            .text_style(egui::TextStyle::Button)
                            .color(text_color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(&result.group_name)
                                .text_style(egui::TextStyle::Small)
                                .color(weak_color),
                        );
                    });
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(snippet_preview(&result.body))
                            .text_style(egui::TextStyle::Small)
                            .color(weak_color),
                    )
                    .wrap(),
                );
            });
        });

    frame_response.response.interact(egui::Sense::click())
}

fn sidebar_group_row(ui: &mut egui::Ui, name: &str, selected: bool) -> egui::Response {
    const SINGLE_ROW_HEIGHT: f32 = 28.0;
    const DOUBLE_ROW_HEIGHT: f32 = 42.0;
    const TEXT_HORIZONTAL_INSET: f32 = 8.0;
    const TEXT_VERTICAL_INSET: f32 = 3.0;

    let row_width = ui.available_width().max(120.0);
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let text_width = (row_width - TEXT_HORIZONTAL_INSET * 2.0).max(1.0);
    let mut job = egui::text::LayoutJob::simple(name.to_string(), font_id, text_color, text_width);
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let row_height = if galley.size().y > SINGLE_ROW_HEIGHT - TEXT_VERTICAL_INSET * 2.0 {
        DOUBLE_ROW_HEIGHT
    } else {
        SINGLE_ROW_HEIGHT
    };

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
    let visuals = ui.visuals();
    let fill = if selected {
        visuals.selection.bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };
    let stroke = if selected || response.hovered() {
        visuals.selection.stroke
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter()
        .rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);

    let text_rect = rect.shrink2(egui::vec2(TEXT_HORIZONTAL_INSET, TEXT_VERTICAL_INSET));
    let text_y = text_rect.center().y - galley.size().y / 2.0;
    ui.painter().with_clip_rect(text_rect).galley(
        egui::pos2(text_rect.left(), text_y),
        galley,
        text_color,
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    response
}

impl TypeTextApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let paths =
            PortablePaths::beside_executable().unwrap_or_else(|_| PortablePaths::from_app_dir("."));
        let snippets = load_or_create_snippets(&paths).unwrap_or_default();
        let mut settings = load_or_create_settings(&paths).unwrap_or_default();
        if OFFLINE_PORTABLE {
            settings.open_on_startup = false;
            settings.check_for_updates = false;
            settings.last_update_check_unix = None;
        } else {
            settings.open_on_startup = platform::startup_enabled();
        }
        settings.theme = normalize_theme(&settings.theme);
        configure_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, &settings);
        let results = search_snippets(&snippets, "");
        let (tx, rx) = mpsc::channel();
        let (tray_tx, tray_rx) = mpsc::channel();
        platform::install_reopen_handler(tray_tx.clone(), cc.egui_ctx.clone());
        let (_update_tx, update_rx) = mpsc::channel();
        let (status, error_message, registered_hotkey) = match platform::register_hotkey(
            settings.hotkey.clone(),
            tx.clone(),
            cc.egui_ctx.clone(),
        ) {
            Ok(()) => (
                format!("Ready - {}", settings.hotkey),
                None,
                Some(settings.hotkey.clone()),
            ),
            Err(error) => (
                "Ready".to_string(),
                Some(format!("Hotkey unavailable: {error}")),
                None,
            ),
        };
        let icon_rgba = app_icon_data().map(|icon| (icon.rgba, icon.width, icon.height));
        let (tray_handle, tray_error) =
            match platform::install_tray_icon(tray_tx, cc.egui_ctx.clone(), icon_rgba) {
                Ok(handle) => (Some(handle), None),
                Err(error) if cfg!(any(windows, target_os = "macos")) => {
                    (None, Some(format!("Tray unavailable: {error}")))
                }
                Err(_) => (None, None),
            };

        let mut app = Self {
            paths,
            snippets,
            settings,
            results,
            search: String::new(),
            view: View::Choose,
            chooser_group: None,
            selected_result: 0,
            selected_group: 0,
            selected_snippet: 0,
            edit_group_active: false,
            edit_snippet_active: false,
            edit_group_name: String::new(),
            edit_title: String::new(),
            edit_body: String::new(),
            status,
            error_message,
            confirm_clear_all: false,
            capturing_hotkey: false,
            settings_dirty: false,
            snippet_chain: Vec::new(),
            insert_when_focus_lost: false,
            registered_hotkey,
            hotkey_tx: tx,
            hotkey_rx: rx,
            tray_rx,
            tray_handle,
            update_rx,
            update_info: None,
            update_check_in_progress: false,
            allow_quit: false,
            show_background_notice: false,
            background_notice_seen: false,
        };
        if let Some(error) = tray_error {
            app.show_error(error);
        }
        app.schedule_update_check(false);
        app.load_selected_editor_snippet();
        app
    }

    fn refresh_results(&mut self) {
        if self
            .chooser_group
            .is_some_and(|group_index| group_index >= self.snippets.groups.len())
        {
            self.chooser_group = None;
        }

        self.results = search_snippets(&self.snippets, &self.search)
            .into_iter()
            .filter(|result| {
                self.chooser_group
                    .is_none_or(|group_index| result.group_index == group_index)
            })
            .collect();
        if self.selected_result >= self.results.len() {
            self.selected_result = self.results.len().saturating_sub(1);
        }
    }

    fn select_chooser_group(&mut self, group: Option<usize>) {
        self.chooser_group = group;
        self.selected_result = 0;
        self.refresh_results();
    }

    fn insert_selected(&mut self, ctx: &egui::Context) {
        let used_chain = !self.snippet_chain.is_empty();
        let insertion = if self.snippet_chain.is_empty() {
            let Some(result) = self.results.get(self.selected_result).cloned() else {
                return;
            };
            ChainInsertion {
                title: result.title,
                body: result.body,
            }
        } else {
            ChainInsertion {
                title: format!("{} snippets", self.snippet_chain.len()),
                body: join_snippet_chain(
                    self.snippet_chain.iter().map(|result| result.body.as_str()),
                    &self.settings,
                ),
            }
        };

        if insertion.body.is_empty() {
            return;
        }

        self.hide_to_background(ctx);
        std::thread::sleep(Duration::from_millis(self.settings.typing_delay_ms));

        match platform::type_text(
            &insertion.body,
            self.settings.windows_character_delay_ms,
            self.settings.windows_separator_delay_ms,
        ) {
            Ok(()) => {
                self.status = format!("Typed {}", insertion.title);
                if used_chain {
                    self.snippet_chain.clear();
                }
            }
            Err(error) => {
                self.show_error(error.to_string());
                self.bring_window_to_front(ctx);
            }
        }

        if !self.settings.close_after_insert {
            self.bring_window_to_front(ctx);
        }
    }

    fn hide_to_background(&mut self, ctx: &egui::Context) {
        self.insert_when_focus_lost = false;
        self.status = "Running in the background".to_string();
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn request_hide_to_background(&mut self, ctx: &egui::Context) {
        if self.background_notice_seen {
            self.hide_to_background(ctx);
        } else {
            self.show_background_notice = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn show_window(&mut self, ctx: &egui::Context, view: View) {
        self.switch_view(view);
        self.bring_window_to_front(ctx);
        self.status = "Ready".to_string();
    }

    fn switch_view(&mut self, view: View) {
        if self.view == View::Edit && view != View::Edit {
            self.clear_edit_selection();
        }
        self.view = view;
    }

    fn clear_edit_selection(&mut self) {
        self.edit_group_active = false;
        self.edit_snippet_active = false;
        self.edit_group_name.clear();
        self.edit_title.clear();
        self.edit_body.clear();
    }

    fn bring_window_to_front(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn handle_window_lifecycle(&mut self, ctx: &egui::Context) {
        let (close_requested, minimized) = ctx.input(|input| {
            (
                input.viewport().close_requested(),
                input.viewport().minimized == Some(true),
            )
        });

        if close_requested && !self.allow_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_hide_to_background(ctx);
        } else if minimized {
            self.request_hide_to_background(ctx);
        }
    }

    fn handle_tray_commands(&mut self, ctx: &egui::Context) {
        let _keep_tray_alive = self.tray_handle.as_ref();
        while let Ok(command) = self.tray_rx.try_recv() {
            match command {
                TrayCommand::Open => self.show_window(ctx, View::Choose),
                TrayCommand::Settings => self.show_window(ctx, View::Settings),
                TrayCommand::Exit => {
                    self.allow_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn save_snippets(&mut self) {
        match save_snippets(&self.paths, &self.snippets) {
            Ok(()) => {
                self.refresh_results();
                self.status = "Snippets saved".to_string();
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn import_droptext_snippets(&mut self) {
        let path = match platform::open_droptext_file_dialog() {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                self.show_error(error.to_string());
                return;
            }
        };

        match import_droptext_ini(&path) {
            Ok(imported) => {
                let group_count = imported.groups.len();
                let snippet_count = imported
                    .groups
                    .iter()
                    .map(|group| group.snippets.len())
                    .sum::<usize>();
                merge_snippet_file(&mut self.snippets, imported);
                self.selected_group = 0;
                self.selected_snippet = 0;
                self.edit_group_active = false;
                self.edit_snippet_active = false;
                self.load_selected_editor_snippet();
                match save_snippets(&self.paths, &self.snippets) {
                    Ok(()) => {
                        self.refresh_results();
                        self.status =
                            format!("Imported {snippet_count} snippets from {group_count} groups");
                    }
                    Err(error) => self.show_error(error.to_string()),
                }
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn export_typetext_snippets(&mut self) {
        let path = match platform::open_snippets_export_dialog(&self.paths.data_dir) {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                self.show_error(error.to_string());
                return;
            }
        };

        match export_snippets(&path, &self.snippets) {
            Ok(()) => {
                self.status = "Exported snippets".to_string();
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn schedule_update_check(&mut self, force: bool) {
        if OFFLINE_PORTABLE {
            return;
        }
        if self.update_check_in_progress {
            return;
        }
        if !force && !self.settings.check_for_updates {
            return;
        }

        let now = current_unix_time();
        if !force
            && self
                .settings
                .last_update_check_unix
                .is_some_and(|checked_at| {
                    now.saturating_sub(checked_at) < UPDATE_CHECK_INTERVAL_SECONDS
                })
        {
            return;
        }

        self.settings.last_update_check_unix = Some(now);
        let _ = save_settings(&self.paths, &self.settings);

        let (tx, rx) = mpsc::channel();
        self.update_rx = rx;
        self.update_check_in_progress = true;
        if force {
            self.status = "Checking for updates...".to_string();
        }

        std::thread::spawn(move || {
            let message = match check_latest_release() {
                Ok(Some(update)) => UpdateCheckMessage::Available(update),
                Ok(None) => UpdateCheckMessage::Current { notify: force },
                Err(error) => UpdateCheckMessage::Failed {
                    error: error.to_string(),
                    notify: force,
                },
            };
            let _ = tx.send(message);
        });
    }

    fn handle_update_messages(&mut self) {
        while let Ok(message) = self.update_rx.try_recv() {
            self.update_check_in_progress = false;
            match message {
                UpdateCheckMessage::Available(update) => {
                    self.status = format!("Update available: {}", update.version);
                    self.update_info = Some(update);
                }
                UpdateCheckMessage::Current { notify } => {
                    if notify {
                        self.status = "TypeText is up to date".to_string();
                    }
                    self.update_info = None;
                }
                UpdateCheckMessage::Failed { error, notify } => {
                    if notify {
                        self.status = "Update check failed".to_string();
                        self.show_error(format!("Could not check for updates: {error}"));
                    }
                }
            }
        }
    }

    fn open_update_download(&mut self) {
        let Some(update) = self.update_info.as_ref() else {
            return;
        };

        if let Err(error) = platform::open_url(&update.download_url) {
            self.show_error(error.to_string());
        }
    }

    fn clear_all_snippets(&mut self) {
        self.snippets = SnippetFile {
            version: 1,
            groups: Vec::new(),
        };
        self.selected_group = 0;
        self.selected_snippet = 0;
        self.edit_group_active = false;
        self.edit_snippet_active = false;
        self.chooser_group = None;
        self.selected_result = 0;
        self.snippet_chain.clear();
        self.insert_when_focus_lost = false;
        self.load_selected_editor_snippet();

        match save_snippets(&self.paths, &self.snippets) {
            Ok(()) => {
                self.refresh_results();
                self.status = "Cleared all snippets".to_string();
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn add_result_to_chain(&mut self, index: usize) {
        let Some(result) = self.results.get(index).cloned() else {
            return;
        };
        self.snippet_chain.push(result);
        self.insert_when_focus_lost = true;
        self.status = format!(
            "Queued {} snippets - click the target text field",
            self.snippet_chain.len()
        );
    }

    fn remove_result_from_chain(&mut self, index: usize) {
        let Some(result) = self.results.get(index) else {
            return;
        };

        if let Some(chain_index) = self.snippet_chain.iter().rposition(|queued| {
            queued.group_index == result.group_index && queued.snippet_index == result.snippet_index
        }) {
            self.snippet_chain.remove(chain_index);
            self.insert_when_focus_lost = !self.snippet_chain.is_empty();
            self.status = if self.snippet_chain.is_empty() {
                "Chain cleared".to_string()
            } else {
                format!(
                    "Queued {} snippets - click the target text field",
                    self.snippet_chain.len()
                )
            };
        }
    }

    fn result_is_queued(&self, result: &SearchResult) -> bool {
        self.snippet_chain.iter().any(|queued| {
            queued.group_index == result.group_index && queued.snippet_index == result.snippet_index
        })
    }

    fn insert_queued_into_current_focus(&mut self, ctx: &egui::Context) {
        if !self.insert_when_focus_lost || self.snippet_chain.is_empty() {
            return;
        }

        let insertion = ChainInsertion {
            title: format!("{} snippets", self.snippet_chain.len()),
            body: join_snippet_chain(
                self.snippet_chain.iter().map(|result| result.body.as_str()),
                &self.settings,
            ),
        };

        if insertion.body.is_empty() {
            return;
        }

        self.hide_to_background(ctx);
        std::thread::sleep(Duration::from_millis(self.settings.typing_delay_ms));

        match platform::type_text_current_focus(
            &insertion.body,
            self.settings.windows_character_delay_ms,
            self.settings.windows_separator_delay_ms,
        ) {
            Ok(()) => {
                self.status = format!("Typed {}", insertion.title);
                self.snippet_chain.clear();
            }
            Err(error) => {
                self.show_error(error.to_string());
                self.show_window(ctx, View::Choose);
            }
        }
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        self.settings.theme = normalize_theme(&self.settings.theme);
        let Some(accent_color) = parse_hex_color(&self.settings.accent_color) else {
            self.show_error("Accent color must be a 6-digit hex value, like #0A7E76");
            return;
        };
        self.settings.accent_color = format_hex_color(accent_color);
        match save_settings_with_effects(
            &self.paths,
            &mut self.settings,
            &self.hotkey_tx,
            &mut self.registered_hotkey,
        ) {
            Ok(()) => {
                apply_theme(ctx, &self.settings);
                self.settings_dirty = false;
                self.status = "Settings saved".to_string();
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn mark_settings_dirty(&mut self) {
        self.settings_dirty = true;
        self.status = "Settings changed. Save settings to apply them.".to_string();
    }

    fn handle_hotkey_capture(&mut self, ctx: &egui::Context) {
        if !self.capturing_hotkey {
            return;
        }

        let captured = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    physical_key: _,
                    pressed: true,
                    repeat: false,
                    modifiers,
                } => hotkey_from_event(*key, *modifiers),
                _ => None,
            })
        });

        if let Some(hotkey) = captured {
            self.settings.hotkey = hotkey;
            self.capturing_hotkey = false;
            self.mark_settings_dirty();
        }
    }

    fn selected_group_mut(&mut self) -> Option<&mut SnippetGroup> {
        self.snippets.groups.get_mut(self.selected_group)
    }

    fn selected_snippet_mut(&mut self) -> Option<&mut Snippet> {
        self.snippets
            .groups
            .get_mut(self.selected_group)?
            .snippets
            .get_mut(self.selected_snippet)
    }

    fn show_error(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
    }

    fn load_selected_editor_snippet(&mut self) {
        self.edit_group_name = self
            .snippets
            .groups
            .get(self.selected_group)
            .map(|group| group.name.clone())
            .unwrap_or_default();

        if let Some(snippet) = self
            .snippets
            .groups
            .get(self.selected_group)
            .and_then(|group| group.snippets.get(self.selected_snippet))
        {
            self.edit_title = snippet.title.clone();
            self.edit_body = snippet.body.clone();
        } else {
            self.edit_title.clear();
            self.edit_body.clear();
        }
    }
}

impl eframe::App for TypeTextApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_window_lifecycle(ctx);
        self.handle_tray_commands(ctx);
        self.handle_hotkey_capture(ctx);
        self.handle_update_messages();
        self.schedule_update_check(false);

        while self.hotkey_rx.try_recv().is_ok() {
            self.show_window(ctx, View::Choose);
        }

        let lost_focus = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        });
        if lost_focus {
            self.insert_queued_into_current_focus(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let style = ctx.global_style();

        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(style.visuals.panel_fill)
                    .stroke(style.visuals.window_stroke)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show_inside(ui, |ui| self.ui_header(ui, &ctx));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(style.visuals.panel_fill)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show_inside(ui, |ui| match self.view {
                View::Choose => self.ui_choose(ui, &ctx),
                View::Edit => self.ui_edit(ui),
                View::Settings => self.ui_settings(ui, &ctx),
            });

        self.ui_clear_all_confirmation(&ctx);
        self.ui_background_notice(&ctx);
        self.ui_error_popup(&ctx);
    }
}

impl TypeTextApp {
    fn ui_background_notice(&mut self, ctx: &egui::Context) {
        if !self.show_background_notice {
            return;
        }

        let mut keep_running = false;
        let mut exit = false;
        egui::Area::new(egui::Id::new("background_notice_dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        ui.set_max_width(460.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("TypeText will keep running")
                                    .strong()
                                    .size(15.5)
                                    .color(ui.visuals().text_color()),
                            );
                        });
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    "Closing or hiding the window leaves TypeText running in the background. Use the tray icon to Open, go to Settings, or Exit.",
                                )
                                .size(11.5),
                            )
                            .wrap(),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add_sized([78.0, 24.0], egui::Button::new("Exit"))
                                    .clicked()
                                {
                                    exit = true;
                                }
                                if ui
                                    .add_sized([120.0, 24.0], egui::Button::new("Keep Running"))
                                    .clicked()
                                {
                                    keep_running = true;
                                }
                            });
                        });
                    });
            });

        if keep_running {
            self.show_background_notice = false;
            self.background_notice_seen = true;
            self.hide_to_background(ctx);
        } else if exit {
            self.show_background_notice = false;
            self.background_notice_seen = true;
            self.allow_quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ui_clear_all_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_clear_all {
            return;
        }

        let snippet_count = self
            .snippets
            .groups
            .iter()
            .map(|group| group.snippets.len())
            .sum::<usize>();
        let group_count = self.snippets.groups.len();
        let mut cancel = false;
        let mut confirm = false;

        egui::Area::new(egui::Id::new("clear_all_confirmation_dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        ui.set_max_width(460.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("Clear all snippets?")
                                    .strong()
                                    .size(15.5)
                                    .color(ui.visuals().text_color()),
                            );
                        });
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "This will permanently remove {snippet_count} snippets from {group_count} groups."
                                ))
                                .size(11.5),
                            )
                            .wrap(),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add_sized([88.0, 24.0], egui::Button::new("Clear All"))
                                    .clicked()
                                {
                                    confirm = true;
                                }
                                if ui
                                    .add_sized([78.0, 24.0], egui::Button::new("Cancel"))
                                    .clicked()
                                {
                                    cancel = true;
                                }
                            });
                        });
                    });
            });

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }

        if confirm {
            self.confirm_clear_all = false;
            self.clear_all_snippets();
        } else if cancel {
            self.confirm_clear_all = false;
        }
    }

    fn ui_error_popup(&mut self, ctx: &egui::Context) {
        let Some(message) = self.error_message.as_deref() else {
            return;
        };

        let mut dismiss = false;
        egui::Area::new(egui::Id::new("error_popup_dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        ui.set_max_width(460.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("Error")
                                    .strong()
                                    .size(15.5)
                                    .color(ui.visuals().text_color()),
                            );
                        });
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.add(egui::Label::new(egui::RichText::new(message).size(11.5)).wrap());
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| {
                            if ui
                                .add_sized([78.0, 24.0], egui::Button::new("OK"))
                                .clicked()
                            {
                                dismiss = true;
                            }
                        });
                    });
            });

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            dismiss = true;
        }

        if dismiss {
            self.error_message = None;
        }
    }

    fn ui_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            ui.label(
                egui::RichText::new("TypeText")
                    .strong()
                    .size(15.0)
                    .color(ui.visuals().text_color()),
            );
            ui.label(
                egui::RichText::new(APP_VERSION)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.label(
                egui::RichText::new(&self.status)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Hide").clicked() {
                    self.request_hide_to_background(ctx);
                }
                if nav_button(ui, self.view == View::Settings, "Settings") {
                    self.switch_view(View::Settings);
                }
                if nav_button(ui, self.view == View::Edit, "Edit") {
                    self.switch_view(View::Edit);
                }
                if nav_button(ui, self.view == View::Choose, "Choose") {
                    self.switch_view(View::Choose);
                }
                if !OFFLINE_PORTABLE
                    && self.update_info.is_some()
                    && ui.button("Download Update").clicked()
                {
                    self.open_update_download();
                }
            });
        });
    }

    fn ui_choose(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        section_header(
            ui,
            "Choose Snippet",
            format!("{} available", self.results.len()),
        );
        section_gap(ui);

        framed_section(ui, "Search", "filter snippets", |ui| {
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 84.0, 24.0],
                    egui::TextEdit::singleline(&mut self.search).hint_text("Search snippets"),
                );
                if response.changed() {
                    self.refresh_results();
                }
                if ui.button("Reload").clicked() {
                    match load_or_create_snippets(&self.paths) {
                        Ok(snippets) => {
                            self.snippets = snippets;
                            self.refresh_results();
                            self.status = "Reloaded".to_string();
                        }
                        Err(error) => self.show_error(error.to_string()),
                    }
                }
            });

            ui.add_space(5.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(egui::Button::selectable(
                        self.chooser_group.is_none(),
                        "All",
                    ))
                    .clicked()
                {
                    self.select_chooser_group(None);
                }

                let group_tabs: Vec<(usize, String)> = self
                    .snippets
                    .groups
                    .iter()
                    .enumerate()
                    .map(|(index, group)| (index, group.name.clone()))
                    .collect();

                for (index, name) in group_tabs {
                    if ui
                        .add(egui::Button::selectable(
                            self.chooser_group == Some(index),
                            name,
                        ))
                        .clicked()
                    {
                        self.select_chooser_group(Some(index));
                    }
                }
            });
        });

        if !self.snippet_chain.is_empty() {
            section_gap(ui);
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        section_header(
                            ui,
                            "Queue",
                            format!("{} snippets", self.snippet_chain.len()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Clear").clicked() {
                                self.snippet_chain.clear();
                                self.insert_when_focus_lost = false;
                                self.status = "Chain cleared".to_string();
                            }
                            if ui.button("Undo Last").clicked() {
                                self.snippet_chain.pop();
                                self.insert_when_focus_lost = !self.snippet_chain.is_empty();
                                self.status = if self.snippet_chain.is_empty() {
                                    "Chain cleared".to_string()
                                } else {
                                    format!(
                                        "Queued {} snippets - click the target text field",
                                        self.snippet_chain.len()
                                    )
                                };
                            }
                        });
                    });
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        for (index, result) in self.snippet_chain.iter().enumerate() {
                            ui.label(
                                egui::RichText::new(format!("{}. {}", index + 1, result.title))
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
                });
        }

        section_gap(ui);
        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            self.insert_selected(ctx);
        }

        egui::ScrollArea::vertical()
            .id_salt("choose_results")
            .show(ui, |ui| {
                for index in 0..self.results.len() {
                    let result = self.results[index].clone();
                    let selected = self.selected_result == index;
                    let queued = self.result_is_queued(&result);
                    let response = compact_snippet_row(ui, &result, selected, queued);
                    if response.clicked() {
                        if queued
                            && self.settings.queued_snippet_click_action
                                == QueuedSnippetClickAction::Remove
                        {
                            self.remove_result_from_chain(index);
                            self.selected_result = usize::MAX;
                        } else {
                            self.selected_result = index;
                            self.add_result_to_chain(index);
                        }
                    }
                    if response.double_clicked() {
                        self.selected_result = index;
                        if self.snippet_chain.len() == 1 {
                            self.insert_selected(ctx);
                        }
                    }
                }
            });
    }

    fn ui_edit(&mut self, ui: &mut egui::Ui) {
        const MIN_LIST_HEIGHT: f32 = 82.0;
        const MAX_SNIPPET_LIST_HEIGHT: f32 = 150.0;

        let edit_size = ui.available_size();
        let sidebar_width = edit_size
            .x
            .mul_add(0.24, 0.0)
            .clamp(230.0, 310.0)
            .min(edit_size.x * 0.36);

        let (edit_rect, _) = ui.allocate_exact_size(edit_size, egui::Sense::hover());
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(edit_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
            |ui| {
                let (sidebar_rect, _) = ui.allocate_exact_size(
                    egui::vec2(sidebar_width, edit_rect.height()),
                    egui::Sense::hover(),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(sidebar_rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| self.ui_edit_groups_sidebar(ui, sidebar_rect, sidebar_width),
                );

                ui.separator();

                let editor_width = ui.available_width();
                let editor_height = edit_rect.height();
                let (editor_rect, _) = ui.allocate_exact_size(
                    egui::vec2(editor_width, editor_height),
                    egui::Sense::hover(),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(editor_rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| self.ui_edit_snippet_editor(ui, MIN_LIST_HEIGHT, MAX_SNIPPET_LIST_HEIGHT),
                );
            },
        );
    }

    fn ui_edit_groups_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        sidebar_rect: egui::Rect,
        sidebar_width: f32,
    ) {
        ui.set_clip_rect(sidebar_rect);
        ui.set_width_range(sidebar_width..=sidebar_width);
        ui.set_height_range(sidebar_rect.height()..=sidebar_rect.height());

        ui.horizontal(|ui| {
            section_header(ui, "Groups", format!("{}", self.snippets.groups.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Add").on_hover_text("Add group").clicked() {
                    self.add_editor_group();
                }
            });
        });

        section_gap(ui);
        if self.edit_group_active {
            framed_section(ui, "Group Details", "selected group", |ui| {
                ui.label(egui::RichText::new("Name").small());
                ui.text_edit_singleline(&mut self.edit_group_name);
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let name = self.edit_group_name.trim().to_string();
                        if name.is_empty() {
                            self.show_error("Group name is required");
                        } else if let Some(group) = self.selected_group_mut() {
                            group.name = name;
                            self.save_snippets();
                        }
                    }

                    if ui.button("Delete").clicked()
                        && self.selected_group < self.snippets.groups.len()
                    {
                        self.snippets.groups.remove(self.selected_group);
                        self.selected_group = self.selected_group.saturating_sub(1);
                        self.selected_snippet = 0;
                        self.edit_group_active = false;
                        self.edit_snippet_active = false;
                        self.load_selected_editor_snippet();
                        self.save_snippets();
                    }
                });
            });
            section_gap(ui);
        }

        let list_top = ui.cursor().top();
        let list_height = (sidebar_rect.bottom() - list_top).max(0.0);
        let list_rect = egui::Rect::from_min_size(
            egui::pos2(sidebar_rect.left(), list_top),
            egui::vec2(sidebar_width, list_height),
        );
        ui.painter().rect_filled(
            list_rect,
            6.0,
            ui.visuals().widgets.noninteractive.weak_bg_fill,
        );
        ui.painter().rect_stroke(
            list_rect,
            6.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );

        let group_names: Vec<String> = self
            .snippets
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect();

        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(list_rect.shrink2(egui::vec2(5.0, 3.0)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                let content_rect = list_rect.shrink2(egui::vec2(5.0, 3.0));
                ui.set_clip_rect(content_rect);
                ui.set_width_range(content_rect.width()..=content_rect.width());
                ui.set_min_height(content_rect.height());
                egui::ScrollArea::vertical()
                    .id_salt("edit_groups")
                    .max_height(content_rect.height())
                    .min_scrolled_height(content_rect.height())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, name) in group_names.iter().enumerate() {
                            let selected = self.edit_group_active && self.selected_group == index;
                            if sidebar_group_row(ui, name, selected).clicked() {
                                self.selected_group = index;
                                self.selected_snippet = 0;
                                self.edit_group_active = true;
                                self.edit_snippet_active = false;
                                self.load_selected_editor_snippet();
                                self.edit_title.clear();
                                self.edit_body.clear();
                            }
                            ui.add_space(1.0);
                        }
                    });
            },
        );
    }

    fn ui_edit_snippet_editor(
        &mut self,
        ui: &mut egui::Ui,
        min_list_height: f32,
        max_snippet_list_height: f32,
    ) {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let snippet_count = self
                .snippets
                .groups
                .get(self.selected_group)
                .map(|group| group.snippets.len())
                .unwrap_or_default();
            section_header(ui, "Snippets", format!("{snippet_count} in group"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Add").on_hover_text("Add snippet").clicked() {
                    self.add_editor_snippet();
                }
            });
        });

        section_gap(ui);

        if !self.edit_group_active {
            return;
        }

        let snippet_list_height =
            (ui.available_height() * 0.28).clamp(min_list_height, max_snippet_list_height);
        let snippet_titles: Vec<String> = self
            .snippets
            .groups
            .get(self.selected_group)
            .map(|group| {
                group
                    .snippets
                    .iter()
                    .map(|snippet| snippet.title.clone())
                    .collect()
            })
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt("edit_snippets")
            .max_height(snippet_list_height)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, title) in snippet_titles.iter().enumerate() {
                        let button_width =
                            (title.chars().count() as f32 * 7.0 + 22.0).clamp(80.0, 190.0);
                        let selected = self.edit_snippet_active && self.selected_snippet == index;
                        if ui
                            .add_sized(
                                [button_width, 24.0],
                                egui::Button::selectable(selected, title),
                            )
                            .clicked()
                        {
                            self.selected_snippet = index;
                            self.edit_snippet_active = true;
                            self.load_selected_editor_snippet();
                        }
                    }
                });
            });

        section_gap(ui);

        if !self.edit_snippet_active
            || self
                .snippets
                .groups
                .get(self.selected_group)
                .and_then(|group| group.snippets.get(self.selected_snippet))
                .is_none()
        {
            return;
        }

        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Edit Snippet")
                            .strong()
                            .size(12.5)
                            .color(ui.visuals().text_color()),
                    );
                    ui.add_space(8.0);
                    ui.add_sized(
                        [42.0, 24.0],
                        egui::Label::new(egui::RichText::new("Title").small()),
                    );
                    let button_width = 66.0;
                    let reserved_width = (button_width * 2.0) + (ui.spacing().item_spacing.x * 2.0);
                    let title_width = (ui.available_width() - reserved_width).max(120.0);
                    ui.add_sized(
                        [title_width, 24.0],
                        egui::TextEdit::singleline(&mut self.edit_title),
                    );
                    if ui
                        .add_sized([button_width, 24.0], egui::Button::new("Save"))
                        .clicked()
                    {
                        self.save_selected_editor_snippet();
                    }
                    if ui
                        .add_sized([button_width, 24.0], egui::Button::new("Delete"))
                        .clicked()
                    {
                        self.delete_selected_editor_snippet();
                    }
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Body").small());
                let body_height = (ui.available_height() - 2.0).max(108.0);
                ui.add_sized(
                    [ui.available_width(), body_height],
                    egui::TextEdit::multiline(&mut self.edit_body),
                );
            });
    }

    fn add_editor_group(&mut self) {
        self.snippets.groups.push(SnippetGroup {
            name: "New Group".to_string(),
            snippets: Vec::new(),
        });
        self.selected_group = self.snippets.groups.len() - 1;
        self.selected_snippet = 0;
        self.edit_group_active = true;
        self.edit_snippet_active = false;
        self.load_selected_editor_snippet();
        self.save_snippets();
    }

    fn add_editor_snippet(&mut self) {
        if self.snippets.groups.is_empty() {
            self.snippets.groups.push(SnippetGroup {
                name: "Common Replies".to_string(),
                snippets: Vec::new(),
            });
            self.selected_group = 0;
        }
        self.edit_group_active = true;

        if let Some(group) = self.selected_group_mut() {
            group.snippets.push(Snippet {
                title: "New Snippet".to_string(),
                body: "Type your reusable text here.".to_string(),
            });
            self.selected_snippet = group.snippets.len() - 1;
            self.edit_snippet_active = true;
            self.load_selected_editor_snippet();
            self.save_snippets();
        }
    }

    fn save_selected_editor_snippet(&mut self) {
        let body = self.edit_body.clone();
        let title = self.edit_title.trim().to_string();
        let title = if title.is_empty() {
            title_from_body(&body)
        } else {
            Some(title)
        };

        let Some(title) = title else {
            self.show_error("Snippet title or body is required");
            return;
        };

        if let Some(snippet) = self.selected_snippet_mut() {
            snippet.title = title.clone();
            snippet.body = body;
            self.edit_title = title;
            self.save_snippets();
            return;
        }

        if self.snippets.groups.is_empty() {
            self.snippets.groups.push(SnippetGroup {
                name: "Common Replies".to_string(),
                snippets: Vec::new(),
            });
            self.selected_group = 0;
        }

        if let Some(group) = self.selected_group_mut() {
            group.snippets.push(Snippet { title, body });
            self.selected_snippet = group.snippets.len() - 1;
            self.load_selected_editor_snippet();
            self.save_snippets();
        }
    }

    fn delete_selected_editor_snippet(&mut self) {
        let selected_snippet = self.selected_snippet;
        if let Some(group) = self.selected_group_mut() {
            if selected_snippet < group.snippets.len() {
                group.snippets.remove(selected_snippet);
                self.selected_snippet = self.selected_snippet.saturating_sub(1);
                self.edit_snippet_active = false;
                self.load_selected_editor_snippet();
                self.save_snippets();
            }
        }
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Settings")
                    .strong()
                    .size(12.5)
                    .color(ui.visuals().text_color()),
            );
            ui.label(
                egui::RichText::new("preferences and app data")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            if self.settings_dirty {
                ui.label(
                    egui::RichText::new("Unsaved changes - click Save Settings")
                        .small()
                        .strong()
                        .color(ui.visuals().hyperlink_color),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save Settings").clicked() {
                    self.save_settings(ctx);
                }
            });
        });
        section_gap(ui);

        egui::ScrollArea::vertical()
            .id_salt("settings_sections")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                framed_section(ui, "Hotkey", "global summon shortcut", |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                [220.0, 24.0],
                                egui::TextEdit::singleline(&mut self.settings.hotkey),
                            )
                            .changed()
                        {
                            self.mark_settings_dirty();
                        }
                        let label = if self.capturing_hotkey {
                            "Press keys..."
                        } else {
                            "Capture Hotkey"
                        };
                        if ui.button(label).clicked() {
                            self.capturing_hotkey = !self.capturing_hotkey;
                        }
                    });
                });

                if !OFFLINE_PORTABLE {
                    section_gap(ui);
                    framed_section(ui, "Startup", "launch behavior", |ui| {
                        if ui
                            .checkbox(&mut self.settings.open_on_startup, "Open on Startup")
                            .changed()
                        {
                            self.mark_settings_dirty();
                        }
                    });
                }

                section_gap(ui);
                framed_section(ui, "Typing", "insertion behavior", |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Delay before typing").small());
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.settings.typing_delay_ms)
                                    .range(0..=2_000),
                            )
                            .changed()
                        {
                            self.mark_settings_dirty();
                        }
                        ui.label(egui::RichText::new("milliseconds").small().weak());
                        if self.settings.typing_delay_ms != typetext_core::DEFAULT_TYPING_DELAY_MS
                            && ui.button("Reset to Default").clicked()
                        {
                            self.settings.typing_delay_ms = typetext_core::DEFAULT_TYPING_DELAY_MS;
                            self.mark_settings_dirty();
                        }
                    });
                    #[cfg(windows)]
                    {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Windows character delay").small());
                            if ui
                                .add(
                                    egui::DragValue::new(
                                        &mut self.settings.windows_character_delay_ms,
                                    )
                                    .range(0..=250),
                                )
                                .changed()
                            {
                                self.mark_settings_dirty();
                            }
                            ui.label(egui::RichText::new("milliseconds").small().weak());
                            if self.settings.windows_character_delay_ms
                                != typetext_core::DEFAULT_WINDOWS_CHARACTER_DELAY_MS
                                && ui.button("Reset to Default").clicked()
                            {
                                self.settings.windows_character_delay_ms =
                                    typetext_core::DEFAULT_WINDOWS_CHARACTER_DELAY_MS;
                                self.mark_settings_dirty();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Windows separator delay").small());
                            if ui
                                .add(
                                    egui::DragValue::new(
                                        &mut self.settings.windows_separator_delay_ms,
                                    )
                                    .range(0..=250),
                                )
                                .changed()
                            {
                                self.mark_settings_dirty();
                            }
                            ui.label(egui::RichText::new("milliseconds").small().weak());
                            if self.settings.windows_separator_delay_ms
                                != typetext_core::DEFAULT_WINDOWS_SEPARATOR_DELAY_MS
                                && ui.button("Reset to Default").clicked()
                            {
                                self.settings.windows_separator_delay_ms =
                                    typetext_core::DEFAULT_WINDOWS_SEPARATOR_DELAY_MS;
                                self.mark_settings_dirty();
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                "Defaults are 22ms and 35ms. Try small reductions, such as 12-18ms, on faster machines.",
                            )
                            .small()
                            .weak(),
                        );
                    }
                    if ui
                        .checkbox(
                            &mut self.settings.close_after_insert,
                            "Hide after inserting text",
                        )
                        .changed()
                    {
                        self.mark_settings_dirty();
                    }
                    if ui
                        .checkbox(
                            &mut self.settings.start_snippets_on_new_line,
                            "Start each queued snippet on a new line",
                        )
                        .changed()
                    {
                        self.mark_settings_dirty();
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Empty lines between snippets").small());
                        if ui
                            .add_enabled(
                                self.settings.start_snippets_on_new_line,
                                egui::DragValue::new(
                                    &mut self.settings.empty_lines_between_snippets,
                                )
                                .range(0..=12),
                            )
                            .changed()
                        {
                            self.mark_settings_dirty();
                        }
                        if self.settings.empty_lines_between_snippets
                            != typetext_core::DEFAULT_EMPTY_LINES_BETWEEN_SNIPPETS
                            && ui.button("Reset to Default").clicked()
                        {
                            self.settings.empty_lines_between_snippets =
                                typetext_core::DEFAULT_EMPTY_LINES_BETWEEN_SNIPPETS;
                            self.mark_settings_dirty();
                        }
                    });
                });

                section_gap(ui);
                framed_section(ui, "Selection", "queued snippet clicks", |ui| {
                    ui.horizontal(|ui| {
                        for (value, label) in [
                            (QueuedSnippetClickAction::AddAgain, "Add again"),
                            (QueuedSnippetClickAction::Remove, "Remove from chain"),
                        ] {
                            if ui
                                .selectable_label(
                                    self.settings.queued_snippet_click_action == value,
                                    label,
                                )
                                .clicked()
                            {
                                self.settings.queued_snippet_click_action = value;
                                self.mark_settings_dirty();
                            }
                        }
                    });
                });

                section_gap(ui);
                framed_section(ui, "Appearance", "theme", |ui| {
                    ui.horizontal(|ui| {
                        for (value, label) in
                            [("system", "System"), ("light", "Light"), ("dark", "Dark")]
                        {
                            if ui
                                .add(egui::Button::selectable(
                                    self.settings.theme == value,
                                    label,
                                ))
                                .clicked()
                            {
                                self.settings.theme = value.to_string();
                                self.mark_settings_dirty();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Accent").small());
                        if ui
                            .add_sized(
                                [86.0, 24.0],
                                egui::TextEdit::singleline(&mut self.settings.accent_color)
                                    .hint_text("#0A7E76"),
                            )
                            .changed()
                        {
                            self.mark_settings_dirty();
                        }

                        let mut accent_color = parse_hex_color(&self.settings.accent_color)
                            .unwrap_or_else(|| egui::Color32::from_rgb(10, 126, 118));
                        if ui.color_edit_button_srgba(&mut accent_color).changed() {
                            self.settings.accent_color = format_hex_color(accent_color);
                            self.mark_settings_dirty();
                        }

                        if parse_hex_color(&self.settings.accent_color).is_none() {
                            ui.label(
                                egui::RichText::new("Use #RRGGBB")
                                    .small()
                                    .color(ui.visuals().warn_fg_color),
                            );
                        }
                    });
                });

                if !OFFLINE_PORTABLE {
                    section_gap(ui);
                    framed_section(ui, "Updates", "GitHub releases", |ui| {
                    if ui
                        .checkbox(
                            &mut self.settings.check_for_updates,
                            "Check for updates once per day",
                        )
                        .changed()
                    {
                        self.mark_settings_dirty();
                    }
                    ui.horizontal(|ui| {
                        let check_label = if self.update_check_in_progress {
                            "Checking..."
                        } else {
                            "Check Now"
                        };
                        if ui
                            .add_enabled(
                                !self.update_check_in_progress,
                                egui::Button::new(check_label),
                            )
                            .clicked()
                        {
                            self.schedule_update_check(true);
                        }
                        if let Some(checked_at) = self.settings.last_update_check_unix {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Last checked {}",
                                    relative_time_label(checked_at)
                                ))
                                .small()
                                .weak(),
                            );
                        }
                    });
                    if let Some(update) = self.update_info.clone() {
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} is available for this platform",
                                update.version
                            ))
                            .strong()
                            .color(ui.visuals().text_color()),
                        );
                        ui.label(egui::RichText::new(&update.asset_name).small().weak());
                        ui.horizontal(|ui| {
                            if ui.button("Download").clicked() {
                                self.open_update_download();
                            }
                            if ui.button("Release Notes").clicked() {
                                if let Err(error) = platform::open_url(&update.release_url) {
                                    self.show_error(error.to_string());
                                }
                            }
                        });
                    }
                    });
                }

                section_gap(ui);
                framed_section(ui, "Snippet Data", "import, export, and reset", |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Import").clicked() {
                            self.import_droptext_snippets();
                        }
                        if ui.button("Export").clicked() {
                            self.export_typetext_snippets();
                        }
                        if ui.button("Clear All").clicked() {
                            self.confirm_clear_all = true;
                        }
                    });
                });

                section_gap(ui);
                framed_section(ui, "Storage", "data folder", |ui| {
                    ui.monospace(self.paths.data_dir.display().to_string());
                    ui.label(egui::RichText::new(platform::tray_status()).small().weak());
                    ui.add_space(2.0);
                    if ui.button("Open Data").clicked() {
                        if let Err(error) = platform::open_folder(&self.paths.data_dir) {
                            self.show_error(error.to_string());
                        }
                    }
                });
            });
    }
}

fn check_latest_release() -> anyhow::Result<Option<UpdateInfo>> {
    let raw = platform::fetch_text(LATEST_RELEASE_API_URL)?;
    let raw = raw.trim_start_matches('\u{feff}').trim();
    let release_start = raw
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("Update response did not contain JSON"))?;
    let release: GitHubRelease = serde_json::from_str(&raw[release_start..])?;

    if compare_versions(&release.tag_name, APP_VERSION) != std::cmp::Ordering::Greater {
        return Ok(None);
    }

    let Some(asset) = release
        .assets
        .into_iter()
        .filter_map(|asset| asset_platform_rank(&asset.name).map(|rank| (rank, asset)))
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, asset)| asset)
    else {
        return Ok(None);
    };

    Ok(Some(UpdateInfo {
        version: release.tag_name,
        release_url: release.html_url,
        download_url: asset.browser_download_url,
        asset_name: asset.name,
    }))
}

fn asset_platform_rank(name: &str) -> Option<u8> {
    if cfg!(target_os = "macos") {
        match name {
            "TypeText-macOS.dmg" => Some(0),
            "TypeText-macOS.zip" => Some(1),
            _ => None,
        }
    } else if cfg!(windows) {
        match name {
            "TypeText-Windows-x64-Setup.exe" => Some(0),
            "TypeText-Windows-x64.zip" => Some(1),
            _ => None,
        }
    } else {
        None
    }
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    parse_version_triplet(left).cmp(&parse_version_triplet(right))
}

fn parse_version_triplet(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().trim_start_matches('v');
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn relative_time_label(timestamp: u64) -> String {
    let elapsed = current_unix_time().saturating_sub(timestamp);
    if elapsed < 60 {
        "just now".to_string()
    } else if elapsed < 60 * 60 {
        format!("{} minutes ago", elapsed / 60)
    } else if elapsed < 60 * 60 * 24 {
        format!("{} hours ago", elapsed / (60 * 60))
    } else {
        format!("{} days ago", elapsed / (60 * 60 * 24))
    }
}

fn merge_snippet_file(target: &mut SnippetFile, imported: SnippetFile) {
    target.version = target.version.max(imported.version);

    for imported_group in imported.groups {
        if let Some(group) = target
            .groups
            .iter_mut()
            .find(|group| group.name == imported_group.name)
        {
            group.snippets.extend(imported_group.snippets);
        } else {
            target.groups.push(imported_group);
        }
    }
}

trait SettingsEffects {
    fn set_startup_enabled(&self, enabled: bool) -> anyhow::Result<()>;
    fn reregister_hotkey(&self, hotkey: &str, tx: Sender<()>) -> anyhow::Result<()>;
}

struct PlatformSettingsEffects;

impl SettingsEffects for PlatformSettingsEffects {
    fn set_startup_enabled(&self, enabled: bool) -> anyhow::Result<()> {
        platform::set_startup_enabled(enabled)
    }

    fn reregister_hotkey(&self, hotkey: &str, tx: Sender<()>) -> anyhow::Result<()> {
        platform::reregister_hotkey(hotkey.to_string(), tx)
    }
}

fn save_settings_with_effects(
    paths: &PortablePaths,
    settings: &mut AppSettings,
    hotkey_tx: &Sender<()>,
    registered_hotkey: &mut Option<String>,
) -> anyhow::Result<()> {
    save_settings_with_effects_impl(
        paths,
        settings,
        hotkey_tx,
        registered_hotkey,
        &PlatformSettingsEffects,
    )
}

fn save_settings_with_effects_impl(
    paths: &PortablePaths,
    settings: &mut AppSettings,
    hotkey_tx: &Sender<()>,
    registered_hotkey: &mut Option<String>,
    effects: &dyn SettingsEffects,
) -> anyhow::Result<()> {
    settings.theme = normalize_theme(&settings.theme);
    if !OFFLINE_PORTABLE {
        effects.set_startup_enabled(settings.open_on_startup)?;
    }
    let requested_hotkey = settings.hotkey.clone();
    if registered_hotkey.as_deref() != Some(requested_hotkey.as_str()) {
        if let Err(error) = effects.reregister_hotkey(&requested_hotkey, hotkey_tx.clone()) {
            if let Some(previous_hotkey) = registered_hotkey.clone() {
                settings.hotkey = previous_hotkey;
            }
            return Err(error);
        }
        *registered_hotkey = Some(requested_hotkey);
    }
    save_settings(paths, settings)?;
    Ok(())
}

fn hotkey_from_event(key: egui::Key, modifiers: egui::Modifiers) -> Option<String> {
    let key_name = hotkey_key_name(key)?;
    let mut parts = Vec::new();

    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if cfg!(target_os = "macos") {
        if modifiers.mac_cmd || modifiers.command {
            parts.push("Cmd");
        }
    } else if modifiers.mac_cmd {
        parts.push("Win");
    }

    if parts.is_empty() {
        return None;
    }

    parts.push(key_name);
    Some(parts.join("+"))
}

fn hotkey_key_name(key: egui::Key) -> Option<&'static str> {
    match key {
        egui::Key::Space => Some("Space"),
        egui::Key::Enter => Some("Enter"),
        egui::Key::Escape => Some("Escape"),
        egui::Key::Tab => Some("Tab"),
        egui::Key::A => Some("A"),
        egui::Key::B => Some("B"),
        egui::Key::C => Some("C"),
        egui::Key::D => Some("D"),
        egui::Key::E => Some("E"),
        egui::Key::F => Some("F"),
        egui::Key::G => Some("G"),
        egui::Key::H => Some("H"),
        egui::Key::I => Some("I"),
        egui::Key::J => Some("J"),
        egui::Key::K => Some("K"),
        egui::Key::L => Some("L"),
        egui::Key::M => Some("M"),
        egui::Key::N => Some("N"),
        egui::Key::O => Some("O"),
        egui::Key::P => Some("P"),
        egui::Key::Q => Some("Q"),
        egui::Key::R => Some("R"),
        egui::Key::S => Some("S"),
        egui::Key::T => Some("T"),
        egui::Key::U => Some("U"),
        egui::Key::V => Some("V"),
        egui::Key::W => Some("W"),
        egui::Key::X => Some("X"),
        egui::Key::Y => Some("Y"),
        egui::Key::Z => Some("Z"),
        egui::Key::F1 => Some("F1"),
        egui::Key::F2 => Some("F2"),
        egui::Key::F3 => Some("F3"),
        egui::Key::F4 => Some("F4"),
        egui::Key::F5 => Some("F5"),
        egui::Key::F6 => Some("F6"),
        egui::Key::F7 => Some("F7"),
        egui::Key::F8 => Some("F8"),
        egui::Key::F9 => Some("F9"),
        egui::Key::F10 => Some("F10"),
        egui::Key::F11 => Some("F11"),
        egui::Key::F12 => Some("F12"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::cmp::Ordering;
    use std::fs;
    use std::path::PathBuf;

    #[derive(Default)]
    struct MockSettingsEffects {
        startup_calls: RefCell<Vec<bool>>,
        hotkey_calls: RefCell<Vec<String>>,
        hotkey_result: RefCell<Option<anyhow::Error>>,
    }

    impl SettingsEffects for MockSettingsEffects {
        fn set_startup_enabled(&self, enabled: bool) -> anyhow::Result<()> {
            self.startup_calls.borrow_mut().push(enabled);
            Ok(())
        }

        fn reregister_hotkey(&self, hotkey: &str, _tx: Sender<()>) -> anyhow::Result<()> {
            self.hotkey_calls.borrow_mut().push(hotkey.to_string());
            if let Some(error) = self.hotkey_result.borrow_mut().take() {
                Err(error)
            } else {
                Ok(())
            }
        }
    }

    fn test_paths(name: &str) -> PortablePaths {
        let data_dir = std::env::temp_dir().join(format!(
            "typetext-desktop-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&data_dir).unwrap();
        PortablePaths {
            snippets_path: data_dir.join("snippets.json"),
            settings_path: data_dir.join("settings.json"),
            data_dir,
        }
    }

    fn cleanup_paths(paths: &PortablePaths) {
        let _ = fs::remove_dir_all(&paths.data_dir);
    }

    #[test]
    fn queued_snippets_join_without_separator_by_default() {
        let settings = AppSettings::default();

        assert_eq!(join_snippet_chain(["One", "Two"], &settings), "OneTwo");
    }

    #[test]
    fn queued_snippets_can_start_on_new_lines() {
        let settings = AppSettings {
            start_snippets_on_new_line: true,
            empty_lines_between_snippets: 0,
            ..Default::default()
        };

        assert_eq!(join_snippet_chain(["One", "Two"], &settings), "One\nTwo");
    }

    #[test]
    fn queued_snippets_can_have_empty_lines_between_them() {
        let settings = AppSettings {
            start_snippets_on_new_line: true,
            empty_lines_between_snippets: 1,
            ..Default::default()
        };

        assert_eq!(join_snippet_chain(["One", "Two"], &settings), "One\n\nTwo");
    }

    fn read_settings_hotkey(settings_path: PathBuf) -> String {
        let raw = fs::read_to_string(settings_path).unwrap();
        let saved: AppSettings = serde_json::from_str(&raw).unwrap();
        saved.hotkey
    }

    #[test]
    fn saving_changed_hotkey_reregisters_and_persists_immediately() {
        let paths = test_paths("hotkey-save-success");
        let (tx, _rx) = mpsc::channel();
        let effects = MockSettingsEffects::default();
        let mut settings = AppSettings {
            hotkey: "Ctrl+Alt+K".to_string(),
            open_on_startup: true,
            ..Default::default()
        };
        let mut registered_hotkey = Some("Ctrl+Alt+Space".to_string());

        save_settings_with_effects_impl(
            &paths,
            &mut settings,
            &tx,
            &mut registered_hotkey,
            &effects,
        )
        .unwrap();

        assert_eq!(
            effects.hotkey_calls.borrow().as_slice(),
            &["Ctrl+Alt+K".to_string()]
        );
        assert_eq!(registered_hotkey, Some("Ctrl+Alt+K".to_string()));
        assert_eq!(
            read_settings_hotkey(paths.settings_path.clone()),
            "Ctrl+Alt+K"
        );
        cleanup_paths(&paths);
    }

    #[test]
    fn saving_changed_hotkey_restores_previous_value_when_reregister_fails() {
        let paths = test_paths("hotkey-save-failure");
        let (tx, _rx) = mpsc::channel();
        let effects = MockSettingsEffects::default();
        *effects.hotkey_result.borrow_mut() = Some(anyhow::anyhow!("taken"));
        let mut settings = AppSettings {
            hotkey: "Ctrl+Alt+K".to_string(),
            ..Default::default()
        };
        let mut registered_hotkey = Some("Ctrl+Alt+Space".to_string());

        let error = save_settings_with_effects_impl(
            &paths,
            &mut settings,
            &tx,
            &mut registered_hotkey,
            &effects,
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "taken");
        assert_eq!(settings.hotkey, "Ctrl+Alt+Space");
        assert_eq!(registered_hotkey, Some("Ctrl+Alt+Space".to_string()));
        assert!(!paths.settings_path.exists());
        cleanup_paths(&paths);
    }

    #[test]
    fn hotkey_capture_does_not_treat_platform_command_as_win() {
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let expected = if cfg!(target_os = "macos") {
            "Ctrl+Cmd+Space"
        } else {
            "Ctrl+Space"
        };

        assert_eq!(
            hotkey_from_event(egui::Key::Space, modifiers),
            Some(expected.to_string())
        );
    }

    #[test]
    fn hotkey_capture_keeps_actual_command_modifier_separate() {
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            mac_cmd: true,
            ..Default::default()
        };
        let command_name = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Win"
        };

        assert_eq!(
            hotkey_from_event(egui::Key::Space, modifiers),
            Some(format!("Ctrl+{command_name}+Space"))
        );
    }

    #[test]
    fn hotkey_capture_accepts_macos_command_alias() {
        let modifiers = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let expected = if cfg!(target_os = "macos") {
            Some("Cmd+Space".to_string())
        } else {
            None
        };

        assert_eq!(hotkey_from_event(egui::Key::Space, modifiers), expected);
    }

    #[test]
    fn compares_release_tags_against_app_versions() {
        assert_eq!(compare_versions("v0.2.2", "v0.2.1"), Ordering::Greater);
        assert_eq!(compare_versions("v0.2.1", "v0.2.1"), Ordering::Equal);
        assert_eq!(compare_versions("v0.2.0", "v0.2.1"), Ordering::Less);
        assert_eq!(compare_versions("not-a-version", "v0.2.1"), Ordering::Less);
    }

    #[test]
    fn matches_current_platform_release_asset() {
        let matching_asset = if cfg!(target_os = "macos") {
            "TypeText-macOS.dmg"
        } else if cfg!(windows) {
            "TypeText-Windows-x64-Setup.exe"
        } else {
            "unsupported"
        };

        assert_eq!(
            asset_platform_rank(matching_asset).is_some(),
            cfg!(any(target_os = "macos", windows))
        );
        assert!(asset_platform_rank("TypeText-source.zip").is_none());
    }
}
