//! Cryo syntax highlighting for the script editor.

use cryo_lang::lexer::{KEYWORDS, TYPES};
use eframe::egui::{self, text::LayoutJob, Color32, FontId, TextFormat};

const ENGINE: &[&str] = &["self", "input", "scene", "time", "audio", "anim", "qte", "global", "hud", "emit", "after", "every", "cancel", "PI"];

pub fn highlight(src: &str, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = Color32::from_rgb(212, 212, 212);
    let comment = Color32::from_rgb(106, 153, 85);
    let string = Color32::from_rgb(206, 145, 120);
    let number = Color32::from_rgb(181, 206, 168);
    let keyword = Color32::from_rgb(86, 156, 214);
    let ty = Color32::from_rgb(78, 201, 176);
    let builtin = Color32::from_rgb(220, 220, 170);
    let engine = Color32::from_rgb(156, 220, 254);

    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let n = chars.len();
    let at = |i: usize| if i < n { chars[i].0 } else { src.len() };
    let mut push = |a: usize, b: usize, color: Color32| {
        if b > a {
            job.append(&src[at(a)..at(b)], 0.0, TextFormat::simple(font.clone(), color));
        }
    };

    let mut i = 0;
    while i < n {
        let c = chars[i].1;
        let start = i;
        if c == '/' && i + 1 < n && chars[i + 1].1 == '/' {
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            push(start, i, comment);
        } else if c == '"' {
            i += 1;
            while i < n && chars[i].1 != '"' && chars[i].1 != '\n' {
                if chars[i].1 == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            push(start, i, string);
        } else if c.is_ascii_digit() {
            while i < n && (chars[i].1.is_ascii_digit()) {
                i += 1;
            }
            if i + 1 < n && chars[i].1 == '.' && chars[i + 1].1.is_ascii_digit() {
                i += 1;
                while i < n && chars[i].1.is_ascii_digit() {
                    i += 1;
                }
            }
            push(start, i, number);
        } else if c.is_alphabetic() || c == '_' {
            while i < n && (chars[i].1.is_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let word = &src[at(start)..at(i)];
            let color = if KEYWORDS.contains(&word) {
                keyword
            } else if TYPES.contains(&word) {
                ty
            } else if ENGINE.contains(&word) {
                engine
            } else if cryo_lang::BUILTINS.contains(&word) {
                builtin
            } else {
                plain
            };
            push(start, i, color);
        } else {
            while i < n {
                let c = chars[i].1;
                if c.is_alphanumeric() || c == '_' || c == '"' || c == '/' {
                    break;
                }
                i += 1;
            }
            if i == start {
                i += 1; // a lone `/`
            }
            push(start, i, plain);
        }
    }
    let _ = egui::Color32::WHITE;
    job
}
