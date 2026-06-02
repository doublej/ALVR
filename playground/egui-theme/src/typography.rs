//! Font installation + the type scale.
//!
//! Three families carry the "engineering instrument" voice:
//! - **Space Grotesk Medium** — display family (headings, wordmark, eyebrows)
//! - **IBM Plex Sans** (Regular / SemiBold) — body + strong
//! - **IBM Plex Mono** (Regular / Medium) — every number, log line, code block
//!
//! `install_fonts` runs once at startup (rebuilds the atlas — never per frame).
//! `apply_text_styles` is cheap and runs each frame so the scale stays live.

use egui::{Context, FontData, FontDefinitions, FontFamily, FontId, Style, TextStyle};
use std::sync::Arc;

pub const DISPLAY: &str = "display";
pub const STRONG: &str = "strong";
pub const MONO_STRONG: &str = "mono_strong";

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    register(&mut fonts, "body", include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"));
    register(&mut fonts, "strong", include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"));
    register(&mut fonts, "display", include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"));
    register(&mut fonts, "mono", include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"));
    register(&mut fonts, "mono_strong", include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"));

    // Keep egui's bundled fonts as fallback so emoji/symbol glyphs (⚠ ❓ ●) still render.
    prepend(&mut fonts, FontFamily::Proportional, "body");
    prepend(&mut fonts, FontFamily::Monospace, "mono");
    named(&mut fonts, DISPLAY, &["display", "body"]);
    named(&mut fonts, STRONG, &["strong", "body"]);
    named(&mut fonts, MONO_STRONG, &["mono_strong", "mono"]);

    ctx.set_fonts(fonts);
}

fn register(fonts: &mut FontDefinitions, key: &str, bytes: &'static [u8]) {
    fonts
        .font_data
        .insert(key.to_owned(), Arc::new(FontData::from_static(bytes)));
}

fn prepend(fonts: &mut FontDefinitions, family: FontFamily, key: &str) {
    fonts.families.entry(family).or_default().insert(0, key.to_owned());
}

fn named(fonts: &mut FontDefinitions, name: &str, keys: &[&str]) {
    let stack = keys.iter().map(|k| k.to_string()).collect();
    fonts.families.insert(FontFamily::Name(name.into()), stack);
}

#[derive(Clone)]
pub struct TypeScale {
    pub display: f32,
    pub heading: f32,
    pub body: f32,
    pub small: f32,
    pub mono: f32,
    pub button: f32,
    pub eyebrow: f32,
}

impl Default for TypeScale {
    fn default() -> Self {
        Self {
            display: 23.0,
            heading: 15.0,
            body: 14.0,
            small: 12.0,
            mono: 13.0,
            button: 13.5,
            eyebrow: 11.0,
        }
    }
}

pub fn apply_text_styles(style: &mut Style, s: &TypeScale) {
    let prop = FontFamily::Proportional;
    let mono = FontFamily::Monospace;
    let display = FontFamily::Name(DISPLAY.into());
    let strong = FontFamily::Name(STRONG.into());
    let mono_strong = FontFamily::Name(MONO_STRONG.into());

    let m = &mut style.text_styles;
    m.clear();
    m.insert(TextStyle::Heading, FontId::new(s.heading, display.clone()));
    m.insert(TextStyle::Body, FontId::new(s.body, prop.clone()));
    m.insert(TextStyle::Button, FontId::new(s.button, strong));
    m.insert(TextStyle::Small, FontId::new(s.small, prop));
    m.insert(TextStyle::Monospace, FontId::new(s.mono, mono));
    m.insert(ts_display(), FontId::new(s.display, display));
    m.insert(ts_eyebrow(), FontId::new(s.eyebrow, FontFamily::Name(DISPLAY.into())));
    m.insert(ts_mono_strong(), FontId::new(s.mono, mono_strong));
}

pub fn ts_display() -> TextStyle {
    TextStyle::Name("display".into())
}

pub fn ts_eyebrow() -> TextStyle {
    TextStyle::Name("eyebrow".into())
}

pub fn ts_mono_strong() -> TextStyle {
    TextStyle::Name("mono_strong".into())
}
