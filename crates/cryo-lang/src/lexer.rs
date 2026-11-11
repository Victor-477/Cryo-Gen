//! Lexer for the Cryo language.

use crate::CryoError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    // keywords
    Script,
    Prop,
    Var,
    Let,
    On,
    Fn,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Break,
    Continue,
    True,
    False,
    Nil,
    And,
    Or,
    Not,
    // symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    DotDot,
    Colon,
    Semi,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// There is a line break between the previous token and this one.
    pub newline_before: bool,
}

pub const KEYWORDS: &[&str] = &[
    "script", "prop", "var", "let", "on", "fn", "if", "else", "while", "for", "in", "return",
    "break", "continue", "true", "false", "nil", "and", "or", "not",
];

pub const TYPES: &[&str] = &["num", "bool", "text", "vec3", "entity", "list", "any"];

fn keyword(s: &str) -> Option<Tok> {
    Some(match s {
        "script" => Tok::Script,
        "prop" => Tok::Prop,
        "var" => Tok::Var,
        "let" => Tok::Let,
        "on" => Tok::On,
        "fn" => Tok::Fn,
        "if" => Tok::If,
        "else" => Tok::Else,
        "while" => Tok::While,
        "for" => Tok::For,
        "in" => Tok::In,
        "return" => Tok::Return,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "true" => Tok::True,
        "false" => Tok::False,
        "nil" => Tok::Nil,
        "and" => Tok::And,
        "or" => Tok::Or,
        "not" => Tok::Not,
        _ => return None,
    })
}

pub fn lex(src: &str) -> Result<Vec<Token>, CryoError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut newline = false;
    let mut out = Vec::new();

    macro_rules! push {
        ($t:expr) => {{
            out.push(Token { tok: $t, line, newline_before: newline });
            newline = false;
        }};
    }

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');
        match c {
            '\n' => {
                line += 1;
                newline = true;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            '/' if next == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '0'..='9' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                // decimal part (but not the `..` operator)
                if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let s: String = chars[start..i].iter().collect();
                let n = s
                    .parse::<f64>()
                    .map_err(|_| CryoError::new(line, format!("invalid number: {s}")))?;
                push!(Tok::Num(n));
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() || chars[i] == '\n' {
                        return Err(CryoError::new(line, "text is missing its closing quote"));
                    }
                    match chars[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' => {
                            i += 1;
                            match chars.get(i) {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(&c) => s.push(c),
                                None => {}
                            }
                            i += 1;
                        }
                        c => {
                            s.push(c);
                            i += 1;
                        }
                    }
                }
                push!(Tok::Str(s));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                push!(keyword(&s).unwrap_or(Tok::Ident(s)));
            }
            _ => {
                let (tok, len) = match (c, next) {
                    ('.', '.') => (Tok::DotDot, 2),
                    ('-', '>') => (Tok::Arrow, 2),
                    ('+', '=') => (Tok::PlusEq, 2),
                    ('-', '=') => (Tok::MinusEq, 2),
                    ('*', '=') => (Tok::StarEq, 2),
                    ('/', '=') => (Tok::SlashEq, 2),
                    ('=', '=') => (Tok::Eq, 2),
                    ('!', '=') => (Tok::Ne, 2),
                    ('<', '=') => (Tok::Le, 2),
                    ('>', '=') => (Tok::Ge, 2),
                    ('(', _) => (Tok::LParen, 1),
                    (')', _) => (Tok::RParen, 1),
                    ('{', _) => (Tok::LBrace, 1),
                    ('}', _) => (Tok::RBrace, 1),
                    ('[', _) => (Tok::LBracket, 1),
                    (']', _) => (Tok::RBracket, 1),
                    (',', _) => (Tok::Comma, 1),
                    ('.', _) => (Tok::Dot, 1),
                    (':', _) => (Tok::Colon, 1),
                    (';', _) => (Tok::Semi, 1),
                    ('+', _) => (Tok::Plus, 1),
                    ('-', _) => (Tok::Minus, 1),
                    ('*', _) => (Tok::Star, 1),
                    ('/', _) => (Tok::Slash, 1),
                    ('%', _) => (Tok::Percent, 1),
                    ('=', _) => (Tok::Assign, 1),
                    ('<', _) => (Tok::Lt, 1),
                    ('>', _) => (Tok::Gt, 1),
                    _ => {
                        return Err(CryoError::new(line, format!("unexpected character: '{c}'")))
                    }
                };
                push!(tok);
                i += len;
            }
        }
    }
    out.push(Token { tok: Tok::Eof, line, newline_before: true });
    Ok(out)
}
