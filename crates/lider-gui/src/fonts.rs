//! Fonts (decision G1): egui draws text itself and its bundled fonts have no CJK, so LID loads the
//! system's own UI font, monospace font and fallbacks for CJK and symbols; egui's fonts stay last.
//!
//! The download list has a family of its own, [`LIST`]: the font chosen in View → Font (`list_font`), found
//! with fontconfig, then the UI fonts; without a choice it is the UI font.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// The file fontconfig picks for a pattern ("sans", "monospace", "sans:lang=ja").
fn fc_match(pattern: &str) -> Option<PathBuf> {
    let out = Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

/// The family of the download list.
pub const LIST: &str = "list";

/// The file of a font family by its name, when fontconfig has that family (not a substitute).
fn family_file(family: &str) -> Option<PathBuf> {
    if cfg!(windows) || family.trim().is_empty() {
        return None;
    }
    let out = Command::new("fc-match")
        .args(["-f", "%{family}\n%{file}", family.trim()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (found, file) = text.split_once('\n')?;
    let wanted = family.trim().to_lowercase();
    found
        .split(',')
        .any(|f| f.trim().to_lowercase() == wanted)
        .then(|| PathBuf::from(file.trim()))
        .filter(|p| p.is_file())
}

/// Font files for (proportional, monospace, fallbacks), first hit first.
fn system_fonts() -> (Vec<PathBuf>, Vec<PathBuf>) {
    if cfg!(windows) {
        let dir = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("Fonts");
        let has = |names: &[&str]| {
            names
                .iter()
                .map(|n| dir.join(n))
                .filter(|p| p.is_file())
                .collect::<Vec<_>>()
        };
        return (
            has(&[
                "segoeui.ttf",
                "msyh.ttc",
                "meiryo.ttc",
                "malgun.ttf",
                "seguisym.ttf",
            ]),
            has(&["consola.ttf"]),
        );
    }
    let mut ui: Vec<PathBuf> = [
        "sans",
        "sans:lang=ja",
        "sans:lang=zh-cn",
        "sans:lang=ko",
        "sans:lang=ar",
        "emoji",
    ]
    .iter()
    .filter_map(|p| fc_match(p))
    .collect();
    ui.dedup();
    (ui, fc_match("monospace").into_iter().collect())
}

pub fn install(ctx: &egui::Context) {
    install_with_list(ctx, "");
}

/// The fonts, with `list_family` (a family name, "" for the UI font) first in the list's family.
pub fn install_with_list(ctx: &egui::Context, list_family: &str) {
    let mut defs = FontDefinitions::default();
    let (ui, mono) = system_fonts();
    let add = |family: FontFamily, files: &[PathBuf], defs: &mut FontDefinitions| {
        let mut names = vec![];
        for f in files {
            // Font collections (.ttc) and variable fonts load with their first face.
            let Ok(bytes) = std::fs::read(f) else {
                continue;
            };
            let name = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            defs.font_data
                .entry(name.clone())
                .or_insert_with(|| Arc::new(FontData::from_owned(bytes)));
            names.push(name);
        }
        let list = defs.families.entry(family).or_default();
        for (i, n) in names.into_iter().enumerate() {
            list.insert(i, n);
        }
    };
    add(FontFamily::Proportional, &ui, &mut defs);
    add(FontFamily::Monospace, &mono, &mut defs);
    // CJK and symbols in monospace text too (file names in the list).
    add(
        FontFamily::Monospace,
        &ui.iter().skip(1).cloned().collect::<Vec<_>>(),
        &mut defs,
    );
    // The list: the chosen family, then everything the UI text falls back to.
    let proportional = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    defs.families
        .insert(FontFamily::Name(LIST.into()), proportional);
    if let Some(file) = family_file(list_family) {
        add(FontFamily::Name(LIST.into()), &[file], &mut defs);
    }
    ctx.set_fonts(defs);
}
