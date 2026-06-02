//! The tunable theme: a refined neutral ramp, a brighter accent, two-tier radii,
//! hairline borders and soft depth. `apply` is the single source of truth that the
//! dashboard's `theme::set_theme` should grow into; `export_rust` emits it verbatim.

use crate::typography::{self, TypeScale};
use egui::{
    Color32, Context, CornerRadius, Margin, Shadow, Stroke, ThemePreference, Vec2, Visuals,
};

#[derive(Clone)]
pub struct ThemeParams {
    // Accent
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    // Surfaces (dark → light elevation)
    pub bg: Color32,
    pub elevated_bg: Color32,
    pub section_bg: Color32,
    pub darker_bg: Color32,
    pub hover_bg: Color32,
    // Lines
    pub separator: Color32,
    pub border: Color32,
    // Text
    pub fg: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    // Status
    pub ok_green: Color32,
    pub ko_red: Color32,
    pub warning: Color32,
    // Logs
    pub log_error: Color32,
    pub log_warning: Color32,
    pub log_info: Color32,
    pub log_debug: Color32,
    pub log_event: Color32,
    // Shape & spacing
    pub widget_radius: u8,
    pub card_radius: u8,
    pub frame_padding: f32,
    pub item_spacing: Vec2,
    pub button_padding: Vec2,
    pub interact_size: f32,
    pub slider_width: f32,
    pub frame_text_spacing: f32,
    pub scrollbar_dot_diameter: f32,
}

impl Default for ThemeParams {
    fn default() -> Self {
        Self {
            accent: rgb(74, 134, 247),
            accent_hover: rgb(120, 168, 255),
            on_accent: rgb(245, 248, 255),
            bg: rgb(17, 18, 21),
            elevated_bg: rgb(23, 25, 29),
            section_bg: rgb(28, 31, 36),
            darker_bg: rgb(13, 14, 16),
            hover_bg: rgb(35, 38, 44),
            separator: rgb(38, 41, 47),
            border: rgb(49, 53, 60),
            fg: rgb(233, 236, 240),
            text_muted: rgb(150, 156, 167),
            text_faint: rgb(101, 107, 118),
            ok_green: rgb(61, 196, 142),
            ko_red: rgb(235, 91, 91),
            warning: rgb(228, 173, 64),
            log_error: rgb(240, 110, 110),
            log_warning: rgb(224, 170, 64),
            log_info: rgb(122, 162, 247),
            log_debug: rgb(150, 156, 167),
            log_event: rgb(108, 114, 126),
            widget_radius: 7,
            card_radius: 12,
            frame_padding: 14.0,
            item_spacing: Vec2::new(10.0, 9.0),
            button_padding: Vec2::new(13.0, 7.0),
            interact_size: 28.0,
            slider_width: 200.0,
            frame_text_spacing: 8.0,
            scrollbar_dot_diameter: 16.0,
        }
    }
}

impl ThemeParams {
    /// Soft drop shadow for cards, popups and menus.
    pub fn shadow(&self) -> Shadow {
        Shadow {
            offset: [0, 3],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(90),
        }
    }

    /// Apply colors, spacing and the type scale to `ctx`. Call once per frame.
    pub fn apply(&self, ctx: &Context, scale: &TypeScale) {
        ctx.set_theme(ThemePreference::Dark);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = self.item_spacing;
        style.spacing.button_padding = self.button_padding;
        style.spacing.interact_size = Vec2::splat(self.interact_size);
        style.spacing.slider_width = self.slider_width;
        style.spacing.window_margin = Margin::from(self.frame_padding);
        style.spacing.menu_margin = Margin::same(6);
        style.interaction.tooltip_delay = 0.15;
        typography::apply_text_styles(&mut style, scale);
        ctx.set_style(style);

        ctx.set_visuals(self.visuals());
    }

    fn visuals(&self) -> Visuals {
        let mut v = Visuals::dark();
        let wr = CornerRadius::same(self.widget_radius);
        let card = CornerRadius::same(self.card_radius);

        v.panel_fill = self.bg;
        v.window_fill = self.section_bg;
        v.window_stroke = Stroke::new(1.0_f32, self.border);
        v.window_corner_radius = card;
        v.window_shadow = self.shadow();
        v.menu_corner_radius = wr;
        v.popup_shadow = self.shadow();
        v.faint_bg_color = self.elevated_bg;
        v.extreme_bg_color = self.darker_bg;
        v.code_bg_color = self.darker_bg;
        v.hyperlink_color = self.accent_hover;
        v.selection.bg_fill = with_alpha(self.accent, 80);
        v.selection.stroke = Stroke::new(1.0_f32, self.accent_hover);

        // Labels, separators, card outlines.
        let n = &mut v.widgets.noninteractive;
        n.bg_fill = self.section_bg;
        n.weak_bg_fill = self.section_bg;
        n.bg_stroke = Stroke::new(1.0_f32, self.separator);
        n.fg_stroke = Stroke::new(1.0_f32, self.fg);
        n.corner_radius = card;

        // Buttons / sliders / combos at rest.
        let i = &mut v.widgets.inactive;
        i.bg_fill = self.elevated_bg;
        i.weak_bg_fill = self.elevated_bg;
        i.bg_stroke = Stroke::new(1.0_f32, self.border);
        i.fg_stroke = Stroke::new(1.0_f32, self.fg);
        i.corner_radius = wr;

        // Hover: lift the surface, hint the accent on the edge.
        let h = &mut v.widgets.hovered;
        h.bg_fill = self.hover_bg;
        h.weak_bg_fill = self.hover_bg;
        h.bg_stroke = Stroke::new(1.0_f32, with_alpha(self.accent, 140));
        h.fg_stroke = Stroke::new(1.0_f32, self.fg);
        h.corner_radius = wr;

        // Active / pressed / on: accent fill, light text.
        let a = &mut v.widgets.active;
        a.bg_fill = self.accent;
        a.weak_bg_fill = self.accent;
        a.bg_stroke = Stroke::new(1.0_f32, self.accent);
        a.fg_stroke = Stroke::new(1.0_f32, self.on_accent);
        a.corner_radius = wr;

        // Open combo / collapsing.
        let o = &mut v.widgets.open;
        o.bg_fill = self.elevated_bg;
        o.weak_bg_fill = self.elevated_bg;
        o.bg_stroke = Stroke::new(1.0_f32, self.border);
        o.fg_stroke = Stroke::new(1.0_f32, self.fg);
        o.corner_radius = wr;

        v
    }

    /// Emit a paste-ready `theme.rs` (+ a `set_fonts` helper) for the dashboard.
    pub fn export_rust(&self) -> String {
        let p = self;
        format!(
            "// Generated by alvr_theme_playground — paste into alvr/gui_common/src/theme.rs\n\
// Fonts: bundle the 5 TTFs (Space Grotesk + IBM Plex Sans/Mono) and call set_fonts() at startup.\n\
use egui::{{Color32, Context, CornerRadius, Margin, Shadow, Stroke, ThemePreference, Visuals}};\n\
\n\
// Accent\n\
pub const ACCENT: Color32 = {accent};\n\
pub const ACCENT_HOVER: Color32 = {accent_hover};\n\
pub const ON_ACCENT: Color32 = {on_accent};\n\
// Surfaces\n\
pub const BG: Color32 = {bg};\n\
pub const ELEVATED_BG: Color32 = {elevated_bg};\n\
pub const SECTION_BG: Color32 = {section_bg};\n\
pub const DARKER_BG: Color32 = {darker_bg};\n\
pub const HOVER_BG: Color32 = {hover_bg};\n\
// Lines\n\
pub const SEPARATOR_BG: Color32 = {separator};\n\
pub const BORDER: Color32 = {border};\n\
// Text\n\
pub const FG: Color32 = {fg};\n\
pub const TEXT_MUTED: Color32 = {text_muted};\n\
pub const TEXT_FAINT: Color32 = {text_faint};\n\
// Status\n\
pub const OK_GREEN: Color32 = {ok};\n\
pub const KO_RED: Color32 = {ko};\n\
pub const WARNING: Color32 = {warning};\n\
// Shape & spacing\n\
pub const WIDGET_RADIUS: u8 = {wr};\n\
pub const CORNER_RADIUS: u8 = {card};\n\
pub const FRAME_PADDING: f32 = {fp:?};\n\
pub const FRAME_TEXT_SPACING: f32 = {fts:?};\n\
pub const SCROLLBAR_DOT_DIAMETER: f32 = {sdd:?};\n\
pub const SWITCH_DOT_DIAMETER: f32 = SCROLLBAR_DOT_DIAMETER;\n\
\n\
pub mod log_colors {{\n\
    use egui::Color32;\n\
    pub const ERROR_LIGHT: Color32 = {le};\n\
    pub const WARNING_LIGHT: Color32 = {lw};\n\
    pub const INFO_LIGHT: Color32 = {li};\n\
    pub const DEBUG_LIGHT: Color32 = {ld};\n\
    pub const EVENT_LIGHT: Color32 = {lev};\n\
}}\n\
\n\
fn shadow() -> Shadow {{\n\
    Shadow {{ offset: [0, 3], blur: 16, spread: 0, color: Color32::from_black_alpha(90) }}\n\
}}\n\
\n\
pub fn set_theme(ctx: &Context) {{\n\
    ctx.set_theme(ThemePreference::Dark);\n\
\n\
    let mut style = (*ctx.style()).clone();\n\
    style.spacing.item_spacing = egui::vec2({isx:?}, {isy:?});\n\
    style.spacing.button_padding = egui::vec2({bpx:?}, {bpy:?});\n\
    style.spacing.interact_size = egui::Vec2::splat({is:?});\n\
    style.spacing.slider_width = {slw:?};\n\
    style.spacing.window_margin = Margin::from(FRAME_PADDING);\n\
    style.interaction.tooltip_delay = 0.15;\n\
    // text_styles: see set_fonts() / the playground's typography module.\n\
    ctx.set_style(style);\n\
\n\
    let mut v = Visuals::dark();\n\
    let wr = CornerRadius::same(WIDGET_RADIUS);\n\
    let card = CornerRadius::same(CORNER_RADIUS);\n\
    v.panel_fill = BG;\n\
    v.window_fill = SECTION_BG;\n\
    v.window_stroke = Stroke::new(1.0_f32, BORDER);\n\
    v.window_corner_radius = card;\n\
    v.window_shadow = shadow();\n\
    v.popup_shadow = shadow();\n\
    v.faint_bg_color = ELEVATED_BG;\n\
    v.extreme_bg_color = DARKER_BG;\n\
    v.code_bg_color = DARKER_BG;\n\
    v.hyperlink_color = ACCENT_HOVER;\n\
    v.selection.bg_fill = Color32::from_rgba_unmultiplied({ar}, {ag}, {ab}, 80);\n\
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT_HOVER);\n\
    v.widgets.noninteractive.bg_fill = SECTION_BG;\n\
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, SEPARATOR_BG);\n\
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, FG);\n\
    v.widgets.noninteractive.corner_radius = card;\n\
    v.widgets.inactive.weak_bg_fill = ELEVATED_BG;\n\
    v.widgets.inactive.bg_fill = ELEVATED_BG;\n\
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);\n\
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, FG);\n\
    v.widgets.inactive.corner_radius = wr;\n\
    v.widgets.hovered.weak_bg_fill = HOVER_BG;\n\
    v.widgets.hovered.bg_fill = HOVER_BG;\n\
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);\n\
    v.widgets.hovered.corner_radius = wr;\n\
    v.widgets.active.weak_bg_fill = ACCENT;\n\
    v.widgets.active.bg_fill = ACCENT;\n\
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, ON_ACCENT);\n\
    v.widgets.active.corner_radius = wr;\n\
    ctx.set_visuals(v);\n\
}}\n",
            accent = col(p.accent),
            accent_hover = col(p.accent_hover),
            on_accent = col(p.on_accent),
            bg = col(p.bg),
            elevated_bg = col(p.elevated_bg),
            section_bg = col(p.section_bg),
            darker_bg = col(p.darker_bg),
            hover_bg = col(p.hover_bg),
            separator = col(p.separator),
            border = col(p.border),
            fg = col(p.fg),
            text_muted = col(p.text_muted),
            text_faint = col(p.text_faint),
            ok = col(p.ok_green),
            ko = col(p.ko_red),
            warning = col(p.warning),
            le = col(p.log_error),
            lw = col(p.log_warning),
            li = col(p.log_info),
            ld = col(p.log_debug),
            lev = col(p.log_event),
            wr = p.widget_radius,
            card = p.card_radius,
            fp = p.frame_padding,
            fts = p.frame_text_spacing,
            sdd = p.scrollbar_dot_diameter,
            isx = p.item_spacing.x,
            isy = p.item_spacing.y,
            bpx = p.button_padding.x,
            bpy = p.button_padding.y,
            is = p.interact_size,
            slw = p.slider_width,
            ar = p.accent.r(),
            ag = p.accent.g(),
            ab = p.accent.b(),
        )
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn col(c: Color32) -> String {
    format!("Color32::from_rgb({}, {}, {})", c.r(), c.g(), c.b())
}
