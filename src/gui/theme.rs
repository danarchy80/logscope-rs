//! Synthwave theme for LogScope — palette derived from the retro sci-fi
//! sunrise/sunset source image (violet sky -> magenta horizon -> peach sun,
//! electric-blue grid over a purple floor).
//!
//! Roles (grounded in measured image pixels):
//! - deep violet   #1F0B26  -> window/panel background
//! - sky violet    #3A1D3C  -> subtle raised surfaces
//! - magenta       #B454A8  -> selection / interactive accent
//! - bright pink   #D878D8  -> active widget accent
//! - sun orange    #F5992C  -> the ONE hero accent (Export button)
//! - grid blue     #3778EE  -> hyperlinks / focus
//! - peach         #FFCF7F  -> emphasis / kicker text only

use eframe::egui::{self, Visuals};

/// Palette constants, exposed so other modules (and future views) share them.
pub mod palette {
    use eframe::egui::Color32;
    pub const DEEPEST: Color32 = Color32::from_rgb(0x1F, 0x0B, 0x26); // corner violet-black
    pub const PANEL: Color32 = Color32::from_rgb(0x24, 0x10, 0x2E); // slightly lifted deep violet
    pub const WIDGET_BG: Color32 = Color32::from_rgb(0x30, 0x18, 0x38); // inactive widget fill
    pub const WIDGET_HOVER: Color32 = Color32::from_rgb(0x3C, 0x1E, 0x44); // grid-floor purple
    pub const MAGENTA: Color32 = Color32::from_rgb(0xB4, 0x54, 0xA8); // selection / hover accent
    pub const PINK: Color32 = Color32::from_rgb(0xD8, 0x78, 0xD8); // active accent
    pub const ORANGE: Color32 = Color32::from_rgb(0xF5, 0x99, 0x2C); // hero: Export button
    pub const BLUE: Color32 = Color32::from_rgb(0x37, 0x78, 0xEE); // links / focus ring
    pub const PEACH: Color32 = Color32::from_rgb(0xFF, 0xCF, 0x7F); // kicker / emphasis text
    pub const TEXT: Color32 = Color32::from_rgb(0xF2, 0xEA, 0xF4); // primary text (near-white, cool)
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xBF, 0xA0, 0xC0); // secondary text
    pub const ERROR: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x81); // danger (stays red for semantic truth)
}

/// Apply the synthwave theme to an egui context. Call once at startup via the
/// app-creation closure's `cc.egui_ctx`, or defensively on the first frame.
pub fn install(ctx: &egui::Context) {
    // Force the dark theme regardless of OS light/dark preference; otherwise a
    // light-mode OS makes the active theme resolve to Light and our palette
    // only lands in the unused Dark slot.
    ctx.set_theme(egui::Theme::Dark);

    let mut v = Visuals::dark();
    v.override_text_color = Some(palette::TEXT);

    // Surfaces
    v.panel_fill = palette::PANEL;
    v.window_fill = palette::PANEL;
    v.faint_bg_color = palette::DEEPEST;
    v.extreme_bg_color = palette::DEEPEST;

    // Widgets
    v.widgets.noninteractive.bg_fill = palette::WIDGET_BG;
    v.widgets.noninteractive.fg_stroke.color = palette::TEXT_MUTED;
    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);

    v.widgets.inactive.bg_fill = palette::WIDGET_BG;
    v.widgets.inactive.fg_stroke.color = palette::TEXT;
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(4);

    v.widgets.hovered.bg_fill = palette::WIDGET_HOVER;
    v.widgets.hovered.fg_stroke.color = palette::PINK;
    v.widgets.hovered.bg_stroke.color = palette::MAGENTA;
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(4);

    v.widgets.active.bg_fill = palette::MAGENTA;
    v.widgets.active.fg_stroke.color = palette::DEEPEST;
    v.widgets.active.corner_radius = egui::CornerRadius::same(4);

    v.widgets.open.bg_fill = palette::WIDGET_HOVER;
    v.widgets.open.fg_stroke.color = palette::TEXT;
    v.widgets.open.corner_radius = egui::CornerRadius::same(4);

    // Selection (text selection, checkbox check, etc.)
    v.selection.bg_fill = palette::MAGENTA;
    v.selection.stroke.color = palette::PINK;

    // Links / focus
    v.hyperlink_color = palette::BLUE;

    // Window chrome
    v.window_stroke.color = palette::WIDGET_HOVER;
    v.window_corner_radius = egui::CornerRadius::same(8);

    ctx.set_visuals_of(egui::Theme::Dark, v.clone());

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.visuals = v;
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Style the single hero action button with the sun-orange accent. Call this on
/// the Export button's `Ui` before adding it so it reads as "the one important
/// thing" against the violet/magenta field.
pub fn hero_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let (bg, fg) = if enabled {
        (palette::ORANGE, palette::DEEPEST)
    } else {
        (palette::WIDGET_HOVER, palette::TEXT_MUTED)
    };
    let btn = egui::Button::new(
        egui::RichText::new(text).color(fg).strong(),
    )
    .fill(bg)
    .corner_radius(6.0);
    ui.add_enabled(enabled, btn)
}