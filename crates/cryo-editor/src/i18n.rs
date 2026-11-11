//! Interface language. English is the primary language; Portuguese is available as a secondary one.
//!
//! Every user-facing string goes through the [`tr!`] macro, which carries both versions:
//! `tr!("Scene saved: {}", "Cena salva: {}", path)`.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Pt,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Pt];

    /// Name of the language, written in that language.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Pt => "Português",
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Pt => "pt",
        }
    }
    pub fn from_code(code: &str) -> Lang {
        if code.to_lowercase().starts_with("pt") {
            Lang::Pt
        } else {
            Lang::En
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::Pt
    } else {
        Lang::En
    }
}

pub fn set_lang(lang: Lang) {
    LANG.store(matches!(lang, Lang::Pt) as u8, Ordering::Relaxed);
}

/// Picks the English or the Portuguese text (both are `format!`-style templates).
#[macro_export]
macro_rules! tr {
    ($en:literal, $pt:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::lang() == $crate::i18n::Lang::Pt {
            format!($pt $(, $arg)*)
        } else {
            format!($en $(, $arg)*)
        }
    };
}
