//! Recursive-descent parser for the Cryo language.

use crate::ast::*;
use crate::lexer::{lex, Tok, Token};
use crate::CryoError;
use std::collections::HashMap;

pub fn parse(src: &str) -> Result<Script, CryoError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let script = p.script()?;
    if p.peek() != &Tok::Eof {
        return Err(p.err("expected end of file after the `script` block"));
    }
    Ok(script)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn line(&self) -> usize {
        self.toks[self.pos].line
    }
    fn newline_before(&self) -> bool {
        self.toks[self.pos].newline_before
    }
    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn err(&self, msg: impl Into<String>) -> CryoError {
        CryoError::new(self.line(), msg)
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == t {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: Tok, what: &str) -> Result<(), CryoError> {
        if self.peek() == &t {
            self.next();
            Ok(())
        } else {
            Err(self.err(format!("expected {what}, found {}", describe(self.peek()))))
        }
    }
    fn ident(&mut self, what: &str) -> Result<String, CryoError> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.next();
                Ok(s)
            }
            t => Err(self.err(format!("expected {what}, found {}", describe(&t)))),
        }
    }

    fn ty(&mut self) -> Result<Type, CryoError> {
        let name = self.ident("a type")?;
        Ok(match name.as_str() {
            "num" => Type::Num,
            "bool" => Type::Bool,
            "text" => Type::Text,
            "vec3" => Type::Vec3,
            "entity" => Type::Entity,
            "list" => Type::List,
            "any" => Type::Any,
            _ => {
                return Err(self.err(format!(
                    "unknown type `{name}` (use num, bool, text, vec3, entity, list or any)"
                )))
            }
        })
    }

    fn opt_ty(&mut self) -> Result<Option<Type>, CryoError> {
        if self.eat(&Tok::Colon) {
            Ok(Some(self.ty()?))
        } else {
            Ok(None)
        }
    }

    fn script(&mut self) -> Result<Script, CryoError> {
        self.expect(Tok::Script, "`script`")?;
        let name = self.ident("the script name")?;
        self.expect(Tok::LBrace, "`{`")?;
        let mut s =
            Script { name, fields: vec![], handlers: HashMap::new(), funcs: HashMap::new() };
        loop {
            let line = self.line();
            match self.peek().clone() {
                Tok::RBrace => {
                    self.next();
                    break;
                }
                Tok::Prop | Tok::Var => {
                    let is_prop = self.next() == Tok::Prop;
                    let name = self.ident("the field name")?;
                    if s.fields.iter().any(|f| f.name == name) {
                        return Err(self.err(format!("field `{name}` declared twice")));
                    }
                    let ty = self.opt_ty()?;
                    self.expect(Tok::Assign, "`=` with the initial value")?;
                    let default = self.expr()?;
                    self.eat(&Tok::Semi);
                    s.fields.push(FieldDecl { name, ty, default, is_prop, line });
                }
                Tok::On => {
                    self.next();
                    let f = self.func()?;
                    if s.handlers.contains_key(&f.name) {
                        return Err(CryoError::new(
                            line,
                            format!("event `{}` declared twice", f.name),
                        ));
                    }
                    s.handlers.insert(f.name.clone(), f);
                }
                Tok::Fn => {
                    self.next();
                    let f = self.func()?;
                    if s.funcs.contains_key(&f.name) {
                        return Err(CryoError::new(
                            line,
                            format!("function `{}` declared twice", f.name),
                        ));
                    }
                    s.funcs.insert(f.name.clone(), f);
                }
                Tok::Eof => return Err(self.err("missing `}` to close the script")),
                t => {
                    return Err(self.err(format!(
                        "only `prop`, `var`, `on` and `fn` are allowed inside a script; found {}",
                        describe(&t)
                    )))
                }
            }
        }
        Ok(s)
    }

    fn func(&mut self) -> Result<Func, CryoError> {
        let line = self.line();
        let name = self.ident("a name")?;
        self.expect(Tok::LParen, "`(`")?;
        let mut params = vec![];
        while self.peek() != &Tok::RParen {
            let name = self.ident("the parameter name")?;
            let ty = self.opt_ty()?;
            params.push(Param { name, ty });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        let ret = if self.eat(&Tok::Arrow) { Some(self.ty()?) } else { None };
        let body = self.block()?;
        Ok(Func { name, params, ret, body, line })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, CryoError> {
        self.expect(Tok::LBrace, "`{`")?;
        let mut out = vec![];
        while self.peek() != &Tok::RBrace {
            if self.peek() == &Tok::Eof {
                return Err(self.err("missing `}` to close the block"));
            }
            out.push(self.stmt()?);
            while self.eat(&Tok::Semi) {}
        }
        self.next();
        Ok(out)
    }

    fn stmt(&mut self) -> Result<Stmt, CryoError> {
        let line = self.line();
        let kind = match self.peek().clone() {
            Tok::Let | Tok::Var => {
                let mutable = self.next() == Tok::Var;
                let name = self.ident("the variable name")?;
                let ty = self.opt_ty()?;
                self.expect(Tok::Assign, "`=`")?;
                let value = self.expr()?;
                StmtKind::Decl { name, ty, value, mutable }
            }
            Tok::If => {
                self.next();
                return self.if_rest(line);
            }
            Tok::While => {
                self.next();
                let cond = self.expr()?;
                let body = self.block()?;
                StmtKind::While { cond, body }
            }
            Tok::For => {
                self.next();
                let var = self.ident("the `for` variable")?;
                self.expect(Tok::In, "`in`")?;
                let first = self.expr()?;
                if self.eat(&Tok::DotDot) {
                    let to = self.expr()?;
                    let body = self.block()?;
                    StmtKind::ForRange { var, from: first, to, body }
                } else {
                    let body = self.block()?;
                    StmtKind::ForEach { var, iter: first, body }
                }
            }
            Tok::Return => {
                self.next();
                let has_value = !self.newline_before()
                    && !matches!(self.peek(), Tok::RBrace | Tok::Semi | Tok::Eof);
                StmtKind::Return(if has_value { Some(self.expr()?) } else { None })
            }
            Tok::Break => {
                self.next();
                StmtKind::Break
            }
            Tok::Continue => {
                self.next();
                StmtKind::Continue
            }
            _ => {
                let target = self.expr()?;
                let op = match self.peek() {
                    Tok::Assign => Some(None),
                    Tok::PlusEq => Some(Some(BinOp::Add)),
                    Tok::MinusEq => Some(Some(BinOp::Sub)),
                    Tok::StarEq => Some(Some(BinOp::Mul)),
                    Tok::SlashEq => Some(Some(BinOp::Div)),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        if !matches!(
                            target.kind,
                            ExprKind::Ident(_) | ExprKind::Member(..) | ExprKind::Index(..)
                        ) {
                            return Err(self.err("the left-hand side of `=` cannot be assigned to"));
                        }
                        self.next();
                        let value = self.expr()?;
                        StmtKind::Assign { target, op, value }
                    }
                    None => StmtKind::Expr(target),
                }
            }
        };
        Ok(Stmt { kind, line })
    }

    fn if_rest(&mut self, line: usize) -> Result<Stmt, CryoError> {
        let cond = self.expr()?;
        let then = self.block()?;
        let otherwise = if self.eat(&Tok::Else) {
            if self.peek() == &Tok::If {
                let l = self.line();
                self.next();
                Some(vec![self.if_rest(l)?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt { kind: StmtKind::If { cond, then, otherwise }, line })
    }

    // ---------- expressions ----------

    fn expr(&mut self) -> Result<Expr, CryoError> {
        self.binary(0)
    }

    fn binop(&self) -> Option<(BinOp, u8)> {
        // A `-` at the start of a new line begins another statement.
        Some(match self.peek() {
            Tok::Or => (BinOp::Or, 1),
            Tok::And => (BinOp::And, 2),
            Tok::Eq => (BinOp::Eq, 3),
            Tok::Ne => (BinOp::Ne, 3),
            Tok::Lt => (BinOp::Lt, 4),
            Tok::Le => (BinOp::Le, 4),
            Tok::Gt => (BinOp::Gt, 4),
            Tok::Ge => (BinOp::Ge, 4),
            Tok::Plus => (BinOp::Add, 5),
            Tok::Minus if !self.newline_before() => (BinOp::Sub, 5),
            Tok::Star => (BinOp::Mul, 6),
            Tok::Slash => (BinOp::Div, 6),
            Tok::Percent => (BinOp::Mod, 6),
            _ => return None,
        })
    }

    fn binary(&mut self, min_prec: u8) -> Result<Expr, CryoError> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = self.binop() {
            if prec < min_prec {
                break;
            }
            let line = self.line();
            self.next();
            let rhs = self.binary(prec + 1)?;
            lhs = Expr { kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), line };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, CryoError> {
        let line = self.line();
        match self.peek() {
            Tok::Minus => {
                self.next();
                Ok(Expr { kind: ExprKind::Neg(Box::new(self.unary()?)), line })
            }
            Tok::Not => {
                self.next();
                Ok(Expr { kind: ExprKind::Not(Box::new(self.unary()?)), line })
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, CryoError> {
        let mut e = self.primary()?;
        loop {
            let line = self.line();
            match self.peek() {
                Tok::Dot => {
                    self.next();
                    let name = self.ident("the member name after `.`")?;
                    e = Expr { kind: ExprKind::Member(Box::new(e), name), line };
                }
                Tok::LParen if !self.newline_before() => {
                    self.next();
                    let args = self.args(Tok::RParen)?;
                    e = Expr { kind: ExprKind::Call(Box::new(e), args), line };
                }
                Tok::LBracket if !self.newline_before() => {
                    self.next();
                    let idx = self.expr()?;
                    self.expect(Tok::RBracket, "`]`")?;
                    e = Expr { kind: ExprKind::Index(Box::new(e), Box::new(idx)), line };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn args(&mut self, close: Tok) -> Result<Vec<Expr>, CryoError> {
        let mut args = vec![];
        while self.peek() != &close {
            args.push(self.expr()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        let what = if close == Tok::RParen { "`)`" } else { "`]`" };
        self.expect(close, what)?;
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr, CryoError> {
        let line = self.line();
        let kind = match self.next() {
            Tok::Num(n) => ExprKind::Num(n),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Nil => ExprKind::Nil,
            Tok::Ident(s) => ExprKind::Ident(s),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(e);
            }
            Tok::LBracket => ExprKind::List(self.args(Tok::RBracket)?),
            t => {
                return Err(CryoError::new(
                    line,
                    format!("expected an expression, found {}", describe(&t)),
                ))
            }
        };
        Ok(Expr { kind, line })
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(n) => format!("the number {n}"),
        Tok::Str(_) => "a text".into(),
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Eof => "the end of the file".into(),
        Tok::LBrace => "`{`".into(),
        Tok::RBrace => "`}`".into(),
        Tok::LParen => "`(`".into(),
        Tok::RParen => "`)`".into(),
        Tok::Assign => "`=`".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}
