//! The three palettes of LID (theme.py in 1.x, tokens.ts in the Tauri window) and the egui style made
//! from them.

use egui::{Color32, CornerRadius, Stroke, Visuals};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Ember,
    EmberDark,
    Dark,
}

impl ThemeName {
    pub fn parse(s: &str) -> ThemeName {
        match s {
            "ember-dark" => ThemeName::EmberDark,
            "dark" => ThemeName::Dark,
            _ => ThemeName::Ember,
        }
    }
    fn index(self) -> usize {
        self as usize
    }
    pub fn is_dark(self) -> bool {
        self != ThemeName::Ember
    }
}

macro_rules! palette {
    ($($name:ident: [$a:literal, $b:literal, $c:literal],)*) => {
        /// One palette: the colours by their 1.x token names.
        #[derive(Debug, Clone, Copy)]
        #[allow(dead_code)]
        pub struct Palette { $(pub $name: Color32,)* }
        impl Palette {
            pub fn of(t: ThemeName) -> Palette {
                Palette { $($name: hex([$a, $b, $c][t.index()]),)* }
            }
        }
    };
}

palette! {
    win: ["#F7F5F2", "#1A1714", "#1B1E24"],
    pane: ["#FFFFFF", "#141210", "#15181D"],
    alt: ["#FBFAF8", "#181512", "#181B21"],
    hdr: ["#F9F7F4", "#1D1915", "#1D2027"],
    line: ["#E7E2DA", "#2E2925", "#2B3039"],
    line2: ["#D8D1C6", "#40382F", "#3A404B"],
    tx: ["#2A2420", "#EDE6DF", "#E2E8F0"],
    mu: ["#6B625A", "#AEA399", "#A3ADBD"],
    fa: ["#9A9087", "#7C7168", "#6E7787"],
    ac: ["#EA580C", "#F97316", "#F97316"],
    acsel: ["#FDEEE4", "#3A2417", "#33261F"],
    actx: ["#C2410C", "#FDBA74", "#FDBA74"],
    ok: ["#0D9488", "#2DD4BF", "#2DD4BF"],
    bad: ["#DC2626", "#F87171", "#F87171"],
    track: ["#EFEAE3", "#2B2520", "#2A2F38"],
    input: ["#FFFFFF", "#100E0C", "#111318"],
    btn: ["#FFFFFF", "#221E1A", "#252932"],
    ink: ["#1E293B", "#E2E8F0", "#E2E8F0"],
}

pub fn hex(s: &str) -> Color32 {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
}

/// `#RRGGBB` for SVG recolouring.
pub fn hex_text(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// A colour at `alpha` (0–1) over nothing.
pub fn alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha * 255.0).round() as u8)
}

/// The egui look for a palette: window and panel fills, text, selection, widget states.
pub fn apply(ctx: &egui::Context, t: ThemeName) {
    let p = Palette::of(t);
    let mut v = if t.is_dark() {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.override_text_color = Some(p.tx);
    v.panel_fill = p.win;
    v.window_fill = p.win;
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.alt;
    v.window_stroke = Stroke::new(1.0, p.line2);
    v.selection.bg_fill = p.acsel;
    v.selection.stroke = Stroke::new(1.0, p.actx);
    v.hyperlink_color = p.actx;
    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
        w.fg_stroke = Stroke::new(1.0, p.tx);
    }
    v.widgets.noninteractive.bg_fill = p.win;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v.widgets.inactive.bg_fill = p.btn;
    v.widgets.inactive.weak_bg_fill = p.btn;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line2);
    v.widgets.hovered.bg_fill = p.btn;
    v.widgets.hovered.weak_bg_fill = p.btn;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.ac);
    v.widgets.active.bg_fill = p.acsel;
    v.widgets.active.weak_bg_fill = p.acsel;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.ac);
    v.widgets.open.bg_fill = p.acsel;
    v.widgets.open.weak_bg_fill = p.acsel;
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 5.0);
        s.spacing.interact_size.y = 26.0;
    });
}
