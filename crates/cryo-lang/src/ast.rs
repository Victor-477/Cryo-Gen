//! Syntax tree of the Cryo language.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Type {
    Num,
    Bool,
    Text,
    Vec3,
    Entity,
    List,
    Map,
    Any,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::Num => "num",
            Type::Bool => "bool",
            Type::Text => "text",
            Type::Vec3 => "vec3",
            Type::Entity => "entity",
            Type::List => "list",
            Type::Map => "map",
            Type::Any => "any",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Num(f64),
    Str(String),
    Bool(bool),
    Nil,
    Ident(String),
    List(Vec<Expr>),
    /// `{ key: value, "other key": value }`
    Map(Vec<(Expr, Expr)>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `let` (immutable) or `var` (mutable)
    Decl { name: String, ty: Option<Type>, value: Expr, mutable: bool },
    Assign { target: Expr, op: Option<BinOp>, value: Expr },
    If { cond: Expr, then: Vec<Stmt>, otherwise: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    /// `for i in a..b`
    ForRange { var: String, from: Expr, to: Expr, body: Vec<Stmt> },
    /// `for item in list`
    ForEach { var: String, iter: Expr, body: Vec<Stmt> },
    /// `match value { a, b => { ... } else => { ... } }`
    Match { value: Expr, arms: Vec<(Vec<Expr>, Vec<Stmt>)>, otherwise: Option<Vec<Stmt>> },
    /// `goto name`: switch the script to another `state`
    Goto(String),
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
}

#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Expr,
    /// `prop` shows up in the editor's Inspector; `var` is internal state.
    pub is_prop: bool,
    pub line: usize,
}

/// `state name { on enter() {} on update(dt) {} on exit() {} }`
#[derive(Debug, Clone)]
pub struct StateDecl {
    pub name: String,
    pub handlers: HashMap<String, Func>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Script {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// Events: `on start()`, `on update(dt)`, `on collide(other)`, ...
    pub handlers: HashMap<String, Func>,
    pub funcs: HashMap<String, Func>,
    /// State machine; the first state declared is the initial one.
    pub states: Vec<StateDecl>,
}
