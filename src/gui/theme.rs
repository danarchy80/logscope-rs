//! Multi-theme system for LogScope.
//!
//! Three complete themes share one [`Palette`] shape so every view (heatmap,
//! log table, inspector) can ask the current theme for its colors:
//!
//! - [`ThemeId::Synthwave`] — the original retro identity: violet sky,
//!   magenta selection, sun-orange hero.
//! - [`ThemeId::GraphiteAmber`] — warm charcoal, matte surfaces, a single
//!   amber accent (default).
//! - [`ThemeId::LightHighContrast`] — paper-white, ink text, indigo accent.

use eframe::egui::{self, Color32, Stroke, Visuals};

use crate::models::Level;

/// Selectable UI themes. Persisted as a short string via eframe storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Synthwave,
    GraphiteAmber,
    LightHighContrast,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [
        ThemeId::Synthwave,
        ThemeId::GraphiteAmber,
        ThemeId::LightHighContrast,
    ];

    pub fn next(self) -> ThemeId {
        match self {
            ThemeId::Synthwave => ThemeId::GraphiteAmber,
            ThemeId::GraphiteAmber => ThemeId::LightHighContrast,
            ThemeId::LightHighContrast => ThemeId::Synthwave,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeId::Synthwave => "synthwave",
            ThemeId::GraphiteAmber => "graphite_amber",
            ThemeId::LightHighContrast => "light_high_contrast",
        }
    }

    pub fn from_str(s: &str) -> Option<ThemeId> {
        match s {
            "synthwave" => Some(ThemeId::Synthwave),
            "graphite_amber" => Some(ThemeId::GraphiteAmber),
            "light_high_contrast" => Some(ThemeId::LightHighContrast),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Synthwave => "Synthwave",
            ThemeId::GraphiteAmber => "Graphite Amber",
            ThemeId::LightHighContrast => "Light High-Contrast",
        }
    }
}

impl Default for ThemeId {
    fn default() -> Self {
        ThemeId::GraphiteAmber
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(((hex >> 16) & 0xFF) as u8, ((hex >> 8) & 0xFF) as u8, (hex & 0xFF) as u8)
}

/// Every color a view needs, resolved per theme.
pub struct Palette {
    pub canvas: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub hero: Color32,
    pub hero_text: Color32,
    pub focus: Color32,
    pub kicker: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub error: Color32,
    /// Heatmap cell fill for "no events" (Unknown level).
    pub empty_cell: Color32,
    /// Row background for the selected log entry.
    pub selection_fill: Color32,
    /// Row background on hover.
    pub hover_fill: Color32,
    /// True when the theme is light (affects a few blend choices).
    pub light: bool,
    level_colors: [Color32; 7], // index by level_index()
}

const fn level_index(l: Level) -> usize {
    match l {
        Level::Trace => 0,
        Level::Debug => 1,
        Level::Info => 2,
        Level::Warning => 3,
        Level::Error => 4,
        Level::Critical => 5,
        Level::Unknown => 6,
    }
}

impl Palette {
    pub fn level_color(&self, level: Level) -> Color32 {
        self.level_colors[level_index(level)]
    }
}

static SYNTHWAVE: Palette = Palette {
    canvas: rgb(0x1F_0B_26),
    panel: rgb(0x24_10_2E),
    raised: rgb(0x30_18_38),
    hover: rgb(0x3C_1E_44),
    border: rgb(0x3C_1E_44),
    accent: rgb(0xB4_54_A8),
    accent_text: rgb(0x1F_0B_26),
    hero: rgb(0xF5_99_2C),
    hero_text: rgb(0x1F_0B_26),
    focus: rgb(0x37_78_EE),
    kicker: rgb(0xFF_CF_7F),
    text: rgb(0xF2_EA_F4),
    text_muted: rgb(0xBF_A0_C0),
    error: rgb(0xFF_6B_81),
    empty_cell: rgb(0x1F_0B_26),
    selection_fill: rgb(0x3C_1E_44),
    hover_fill: rgb(0x30_18_38),
    light: false,
    level_colors: [
        rgb(0xBF_A0_C0), // Trace
        rgb(0xB4_54_A8), // Debug
        rgb(0x37_78_EE), // Info
        rgb(0xF5_99_2C), // Warning
        rgb(0xFF_4D_4D), // Error
        rgb(0xFF_6B_81), // Critical
        rgb(0x1F_0B_26), // Unknown
    ],
};

static GRAPHITE_AMBER: Palette = Palette {
    canvas: rgb(0x16_15_1A),
    panel: rgb(0x1D_1C_22),
    raised: rgb(0x26_25_2C),
    hover: rgb(0x2E_2D_36),
    border: rgb(0x34_33_3C),
    accent: rgb(0xF0_A6_4F),
    accent_text: rgb(0x16_15_1A),
    hero: rgb(0xF0_A6_4F),
    hero_text: rgb(0x16_15_1A),
    focus: rgb(0xF0_A6_4F),
    kicker: rgb(0xF0_A6_4F),
    text: rgb(0xED_EA_E4),
    text_muted: rgb(0x9A_96_8E),
    error: rgb(0xE0_52_52),
    empty_cell: rgb(0x16_15_1A),
    selection_fill: rgb(0x2E_2D_36),
    hover_fill: rgb(0x26_25_2C),
    light: false,
    level_colors: [
        rgb(0x6E_6A_64), // Trace
        rgb(0x8E_7B_B0), // Debug
        rgb(0x6C_8F_B5), // Info
        rgb(0xD9_9A_3D), // Warning
        rgb(0xD9_6A_5B), // Error
        rgb(0xE0_52_52), // Critical
        rgb(0x16_15_1A), // Unknown
    ],
};

static LIGHT_HIGH_CONTRAST: Palette = Palette {
    canvas: rgb(0xF7_F7_F5),
    panel: rgb(0xFF_FF_FF),
    raised: rgb(0xF0_EF_EB),
    hover: rgb(0xE8_E7_E2),
    border: rgb(0xE2_E1_DC),
    accent: rgb(0x3D_3A_C0),
    accent_text: rgb(0xFF_FF_FF),
    hero: rgb(0x3D_3A_C0),
    hero_text: rgb(0xFF_FF_FF),
    focus: rgb(0x25_63_EB),
    kicker: rgb(0x3D_3A_C0),
    text: rgb(0x1A_1A_18),
    text_muted: rgb(0x6E_6C_66),
    error: rgb(0xDC_26_26),
    empty_cell: rgb(0xE5_E5_E5),
    selection_fill: rgb(0xE0_DF_F7),
    hover_fill: rgb(0xF0_EF_EB),
    light: true,
    level_colors: [
        rgb(0x52_52_52), // Trace
        rgb(0x6D_28_D9), // Debug
        rgb(0x1D_4E_D8), // Info
        rgb(0xB4_53_09), // Warning
        rgb(0xB9_1C_1C), // Error
        rgb(0xDC_26_26), // Critical
        rgb(0xE5_E5_E5), // Unknown
    ],
};

/// The palette for a theme id.
pub fn palette(id: ThemeId) -> &'static Palette {
    match id {
        ThemeId::Synthwave => &SYNTHWAVE,
        ThemeId::GraphiteAmber => &GRAPHITE_AMBER,
        ThemeId::LightHighContrast => &LIGHT_HIGH_CONTRAST,
    }
}

/// Apply a theme to an egui context. Call at startup and whenever the user
/// cycles themes.
pub fn install(ctx: &egui::Context, id: ThemeId) {
    let pal = palette(id);
    let theme = if pal.light { egui::Theme::Light } else { egui::Theme::Dark };
    ctx.set_theme(theme);

    let mut v = if pal.light { Visuals::light() } else { Visuals::dark() };
    v.override_text_color = Some(pal.text);

    // Surfaces
    v.panel_fill = pal.panel;
    v.window_fill = pal.panel;
    v.faint_bg_color = pal.canvas;
    v.extreme_bg_color = if pal.light { pal.raised } else { pal.canvas };

    // Widgets
    v.widgets.noninteractive.bg_fill = pal.raised;
    v.widgets.noninteractive.fg_stroke.color = pal.text_muted;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);

    v.widgets.inactive.bg_fill = pal.raised;
    v.widgets.inactive.fg_stroke.color = pal.text;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.border);
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(4);

    v.widgets.hovered.bg_fill = pal.hover;
    v.widgets.hovered.fg_stroke.color = if pal.light { pal.accent } else { pal.text };
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, pal.accent);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(4);

    v.widgets.active.bg_fill = pal.accent;
    v.widgets.active.fg_stroke.color = pal.accent_text;
    v.widgets.active.bg_stroke = Stroke::new(1.0, pal.accent);
    v.widgets.active.corner_radius = egui::CornerRadius::same(4);

    v.widgets.open.bg_fill = pal.hover;
    v.widgets.open.fg_stroke.color = pal.text;
    v.widgets.open.corner_radius = egui::CornerRadius::same(4);

    // Selection (text selection, checkbox check, selectable labels)
    v.selection.bg_fill = pal.accent.linear_multiply(0.45);
    v.selection.stroke.color = pal.accent;

    v.hyperlink_color = pal.focus;
    v.window_stroke.color = pal.border;
    v.window_corner_radius = egui::CornerRadius::same(8);

    let mut style = (*ctx.style_of(theme)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.visuals = v;
    ctx.set_style_of(theme, style);
}

/// Style the single hero action button (Export) with the theme's hero accent.
pub fn hero_button(ui: &mut egui::Ui, pal: &Palette, text: &str, enabled: bool) -> egui::Response {
    let (bg, fg) = if enabled {
        (pal.hero, pal.hero_text)
    } else {
        (pal.raised, pal.text_muted)
    };
    let btn = egui::Button::new(
        egui::RichText::new(text).color(fg).strong(),
    )
    .fill(bg)
    .corner_radius(6.0);
    ui.add_enabled(enabled, btn)
}
