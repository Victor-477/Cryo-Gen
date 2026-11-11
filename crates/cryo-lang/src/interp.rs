//! Tree-walking interpreter for the Cryo language.

use crate::ast::*;
use crate::CryoError;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Runtime value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Num(f64),
    Bool(bool),
    Text(Rc<str>),
    Vec3([f64; 3]),
    /// Reference to a scene entity (id).
    Entity(u64),
    List(Rc<RefCell<Vec<Value>>>),
    /// Object provided by the engine (`input`, `scene`, `time`...).
    Object(Rc<str>),
}

impl Value {
    pub fn text(s: impl AsRef<str>) -> Value {
        Value::Text(Rc::from(s.as_ref()))
    }
    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(v)))
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Num(_) => "num",
            Value::Bool(_) => "bool",
            Value::Text(_) => "text",
            Value::Vec3(_) => "vec3",
            Value::Entity(_) => "entity",
            Value::List(_) => "list",
            Value::Object(_) => "object",
        }
    }
    pub fn matches(&self, ty: Type) -> bool {
        matches!(
            (ty, self),
            (Type::Any, _)
                | (Type::Num, Value::Num(_))
                | (Type::Bool, Value::Bool(_))
                | (Type::Text, Value::Text(_))
                | (Type::Vec3, Value::Vec3(_))
                | (Type::Entity, Value::Entity(_) | Value::Nil)
                | (Type::List, Value::List(_))
        )
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Num(n) => write!(f, "{}", fmt_num(*n)),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Text(s) => write!(f, "{s}"),
            Value::Vec3(v) => {
                write!(f, "({}, {}, {})", fmt_num(v[0]), fmt_num(v[1]), fmt_num(v[2]))
            }
            Value::Entity(id) => write!(f, "entity#{id}"),
            Value::List(l) => {
                write!(f, "[")?;
                for (i, v) in l.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Object(name) => write!(f, "<{name}>"),
        }
    }
}

/// Bridge between the script and the engine. Everything Cryo does not know
/// (entities, `input`, `scene`, `log`...) is resolved here.
pub trait Host {
    /// Global names such as `self`, `input`, `scene`, `time`.
    fn global(&mut self, name: &str) -> Option<Value>;
    fn get_member(&mut self, target: &Value, name: &str) -> Result<Value, String>;
    fn set_member(&mut self, target: &Value, name: &str, value: Value) -> Result<(), String>;
    /// `target = None` for global functions (`log(...)`).
    fn call(&mut self, target: Option<&Value>, name: &str, args: &[Value])
        -> Result<Value, String>;
}

/// Empty host: useful for tests and for evaluating `prop` defaults.
pub struct NullHost {
    pub logs: Vec<String>,
}

impl NullHost {
    pub fn new() -> Self {
        NullHost { logs: vec![] }
    }
}

impl Default for NullHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for NullHost {
    fn global(&mut self, _name: &str) -> Option<Value> {
        None
    }
    fn get_member(&mut self, target: &Value, name: &str) -> Result<Value, String> {
        Err(format!("`{}` has no member `{name}`", target.type_name()))
    }
    fn set_member(&mut self, target: &Value, name: &str, _v: Value) -> Result<(), String> {
        Err(format!("`{}` has no member `{name}`", target.type_name()))
    }
    fn call(&mut self, target: Option<&Value>, name: &str, args: &[Value]) -> Result<Value, String> {
        if target.is_none() && name == "log" {
            let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            self.logs.push(parts.join(" "));
            return Ok(Value::Nil);
        }
        Err(format!("unknown function `{name}`"))
    }
}

/// Built-in functions (also used by the editor's syntax highlighting).
pub const BUILTINS: &[&str] = &[
    "vec3", "abs", "sign", "sin", "cos", "tan", "atan2", "sqrt", "pow", "floor", "ceil", "round",
    "min", "max", "clamp", "lerp", "random", "length", "normalize", "dot", "cross", "distance",
    "str", "num", "len", "log",
];

/// Statement limit per event call: stops an infinite `while` from freezing the editor.
const BUDGET: u64 = 2_000_000;
const MAX_DEPTH: usize = 120;

/// A script instance attached to an entity.
pub struct Instance {
    pub script: Rc<Script>,
    pub fields: HashMap<String, Value>,
    rng: u64,
}

impl Instance {
    /// Creates the instance by evaluating the initial values of `prop` and `var`.
    pub fn new(script: Rc<Script>, host: &mut dyn Host) -> Result<Self, CryoError> {
        let mut inst = Instance { script: script.clone(), fields: HashMap::new(), rng: 0x9E3779B97F4A7C15 };
        for f in &script.fields {
            let v = {
                let mut ex = Exec::new(&mut inst, host);
                ex.eval(&f.default)?
            };
            check_type(f.ty, &v, f.line, &format!("field `{}`", f.name))?;
            inst.fields.insert(f.name.clone(), v);
        }
        Ok(inst)
    }

    pub fn has_handler(&self, event: &str) -> bool {
        self.script.handlers.contains_key(event)
    }

    /// Sets the value of a `prop` (from the Inspector), respecting the declared type.
    pub fn set_prop(&mut self, name: &str, value: Value) -> Result<(), CryoError> {
        let Some(f) = self.script.fields.iter().find(|f| f.name == name) else {
            return Ok(()); // old prop that no longer exists in the script
        };
        check_type(f.ty, &value, f.line, &format!("field `{}`", f.name))?;
        self.fields.insert(name.to_string(), value);
        Ok(())
    }

    /// Fires an event (`start`, `update`, ...). Undeclared events are ignored.
    pub fn call_event(
        &mut self,
        event: &str,
        args: &[Value],
        host: &mut dyn Host,
    ) -> Result<Value, CryoError> {
        let script = self.script.clone();
        let Some(func) = script.handlers.get(event) else {
            return Ok(Value::Nil);
        };
        // Handlers may be declared with fewer parameters than the engine sends;
        // the extra arguments are simply dropped.
        let n = func.params.len().min(args.len());
        let mut ex = Exec::new(self, host);
        ex.call_func(func, &args[..n], func.line, true)
    }

    /// Calls a script `fn` by name.
    pub fn call_fn(
        &mut self,
        name: &str,
        args: &[Value],
        host: &mut dyn Host,
    ) -> Result<Value, CryoError> {
        let script = self.script.clone();
        let Some(func) = script.funcs.get(name) else {
            return Err(CryoError::new(0, format!("function `{name}` does not exist in the script")));
        };
        let mut ex = Exec::new(self, host);
        ex.call_func(func, args, func.line, false)
    }
}

/// Evaluates the default values of `prop` fields without needing the engine.
pub fn default_props(script: &Rc<Script>) -> Vec<(String, Option<Type>, Value)> {
    let mut host = NullHost::new();
    let mut inst = Instance { script: script.clone(), fields: HashMap::new(), rng: 1 };
    let mut out = vec![];
    for f in &script.fields {
        let v = {
            let mut ex = Exec::new(&mut inst, &mut host);
            ex.eval(&f.default).unwrap_or(Value::Nil)
        };
        inst.fields.insert(f.name.clone(), v.clone());
        if f.is_prop {
            out.push((f.name.clone(), f.ty, v));
        }
    }
    out
}

fn check_type(ty: Option<Type>, v: &Value, line: usize, what: &str) -> Result<(), CryoError> {
    match ty {
        Some(t) if !v.matches(t) => Err(CryoError::new(
            line,
            format!("{what} expects `{}`, but got `{}`", t.name(), v.type_name()),
        )),
        _ => Ok(()),
    }
}

enum Flow {
    Next,
    Break,
    Continue,
    Return(Value),
}

struct Local {
    value: Value,
    mutable: bool,
    ty: Option<Type>,
}

struct Exec<'a> {
    inst: &'a mut Instance,
    host: &'a mut dyn Host,
    scopes: Vec<HashMap<String, Local>>,
    budget: u64,
    depth: usize,
}

type R<T> = Result<T, CryoError>;

impl<'a> Exec<'a> {
    fn new(inst: &'a mut Instance, host: &'a mut dyn Host) -> Self {
        Exec { inst, host, scopes: vec![HashMap::new()], budget: BUDGET, depth: 0 }
    }

    fn call_func(&mut self, f: &Func, args: &[Value], line: usize, lenient: bool) -> R<Value> {
        if !lenient && args.len() != f.params.len() {
            return Err(CryoError::new(
                line,
                format!(
                    "`{}` expects {} argument(s), but got {}",
                    f.name,
                    f.params.len(),
                    args.len()
                ),
            ));
        }
        if self.depth >= MAX_DEPTH {
            return Err(CryoError::new(line, "recursion too deep"));
        }
        let mut scope = HashMap::new();
        for (p, a) in f.params.iter().zip(args) {
            check_type(p.ty, a, line, &format!("parameter `{}` of `{}`", p.name, f.name))?;
            scope.insert(p.name.clone(), Local { value: a.clone(), mutable: false, ty: p.ty });
        }
        let saved = std::mem::replace(&mut self.scopes, vec![scope]);
        self.depth += 1;
        let result = self.block_inner(&f.body);
        self.depth -= 1;
        self.scopes = saved;
        let v = match result? {
            Flow::Return(v) => v,
            _ => Value::Nil,
        };
        if f.ret.is_some() {
            check_type(f.ret, &v, f.line, &format!("return value of `{}`", f.name))?;
        }
        Ok(v)
    }

    fn block(&mut self, stmts: &[Stmt]) -> R<Flow> {
        self.scopes.push(HashMap::new());
        let r = self.block_inner(stmts);
        self.scopes.pop();
        r
    }

    fn block_inner(&mut self, stmts: &[Stmt]) -> R<Flow> {
        for s in stmts {
            match self.stmt(s)? {
                Flow::Next => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }

    fn tick(&mut self, line: usize) -> R<()> {
        if self.budget == 0 {
            return Err(CryoError::new(
                line,
                "execution took too long (infinite loop?) — the event was stopped",
            ));
        }
        self.budget -= 1;
        Ok(())
    }

    fn cond(&mut self, e: &Expr) -> R<bool> {
        match self.eval(e)? {
            Value::Bool(b) => Ok(b),
            v => Err(CryoError::new(
                e.line,
                format!("the condition must be `bool`, but it is `{}`", v.type_name()),
            )),
        }
    }

    fn stmt(&mut self, s: &Stmt) -> R<Flow> {
        self.tick(s.line)?;
        match &s.kind {
            StmtKind::Decl { name, ty, value, mutable } => {
                let v = self.eval(value)?;
                check_type(*ty, &v, s.line, &format!("variable `{name}`"))?;
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), Local { value: v, mutable: *mutable, ty: *ty });
            }
            StmtKind::Assign { target, op, value } => {
                let mut v = self.eval(value)?;
                if let Some(op) = op {
                    let cur = self.eval(target)?;
                    v = binary(*op, cur, v, s.line)?;
                }
                self.assign(target, v)?;
            }
            StmtKind::If { cond, then, otherwise } => {
                if self.cond(cond)? {
                    return self.block(then);
                } else if let Some(o) = otherwise {
                    return self.block(o);
                }
            }
            StmtKind::While { cond, body } => {
                while self.cond(cond)? {
                    self.tick(s.line)?;
                    match self.block(body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::ForRange { var, from, to, body } => {
                let a = self.num(from)?.round() as i64;
                let b = self.num(to)?.round() as i64;
                for i in a..b {
                    self.tick(s.line)?;
                    match self.loop_body(var, Value::Num(i as f64), body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::ForEach { var, iter, body } => {
                let items = match self.eval(iter)? {
                    Value::List(l) => l.borrow().clone(),
                    v => {
                        return Err(CryoError::new(
                            iter.line,
                            format!("`for ... in` expects a list, but got `{}`", v.type_name()),
                        ))
                    }
                };
                for item in items {
                    self.tick(s.line)?;
                    match self.loop_body(var, item, body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(v));
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Expr(e) => {
                self.eval(e)?;
            }
        }
        Ok(Flow::Next)
    }

    fn loop_body(&mut self, var: &str, value: Value, body: &[Stmt]) -> R<Flow> {
        let mut scope = HashMap::new();
        scope.insert(var.to_string(), Local { value, mutable: false, ty: None });
        self.scopes.push(scope);
        let r = self.block_inner(body);
        self.scopes.pop();
        r
    }

    fn num(&mut self, e: &Expr) -> R<f64> {
        match self.eval(e)? {
            Value::Num(n) => Ok(n),
            v => Err(CryoError::new(
                e.line,
                format!("expected `num`, but got `{}`", v.type_name()),
            )),
        }
    }

    fn assign(&mut self, target: &Expr, v: Value) -> R<()> {
        let line = target.line;
        match &target.kind {
            ExprKind::Ident(name) => {
                for scope in self.scopes.iter_mut().rev() {
                    if let Some(l) = scope.get_mut(name) {
                        if !l.mutable {
                            return Err(CryoError::new(
                                line,
                                format!("`{name}` is immutable (`let`); declare it with `var` to change it"),
                            ));
                        }
                        check_type(l.ty, &v, line, &format!("variable `{name}`"))?;
                        l.value = v;
                        return Ok(());
                    }
                }
                if let Some(f) = self.inst.script.fields.iter().find(|f| &f.name == name) {
                    check_type(f.ty, &v, line, &format!("field `{name}`"))?;
                    self.inst.fields.insert(name.clone(), v);
                    return Ok(());
                }
                Err(CryoError::new(line, format!("variable `{name}` was not declared")))
            }
            ExprKind::Member(obj, name) => {
                let base = self.eval(obj)?;
                match base {
                    Value::Vec3(mut a) => {
                        let i = match name.as_str() {
                            "x" => 0,
                            "y" => 1,
                            "z" => 2,
                            _ => {
                                return Err(CryoError::new(
                                    line,
                                    format!("`vec3` has no member `{name}` (use x, y or z)"),
                                ))
                            }
                        };
                        let Value::Num(n) = v else {
                            return Err(CryoError::new(
                                line,
                                format!("`{name}` of a vec3 must be `num`"),
                            ));
                        };
                        a[i] = n;
                        // writes the modified vector back (e.g. self.position.y = 2)
                        self.assign(obj, Value::Vec3(a))
                    }
                    Value::Entity(_) | Value::Object(_) => self
                        .host
                        .set_member(&base, name, v)
                        .map_err(|m| CryoError::new(line, m)),
                    other => Err(CryoError::new(
                        line,
                        format!("cannot set `{name}` on `{}`", other.type_name()),
                    )),
                }
            }
            ExprKind::Index(obj, idx) => {
                let base = self.eval(obj)?;
                let i = self.num(idx)?;
                match base {
                    Value::List(l) => {
                        let mut l = l.borrow_mut();
                        let n = l.len();
                        let slot = index(i, n, line)?;
                        l[slot] = v;
                        Ok(())
                    }
                    other => Err(CryoError::new(
                        line,
                        format!("`{}` cannot be indexed", other.type_name()),
                    )),
                }
            }
            _ => Err(CryoError::new(line, "invalid assignment target")),
        }
    }

    fn lookup(&mut self, name: &str, line: usize) -> R<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(l) = scope.get(name) {
                return Ok(l.value.clone());
            }
        }
        if let Some(v) = self.inst.fields.get(name) {
            return Ok(v.clone());
        }
        match name {
            "PI" => return Ok(Value::Num(std::f64::consts::PI)),
            _ => {}
        }
        if let Some(v) = self.host.global(name) {
            return Ok(v);
        }
        Err(CryoError::new(line, format!("`{name}` was not declared")))
    }

    fn eval(&mut self, e: &Expr) -> R<Value> {
        let line = e.line;
        Ok(match &e.kind {
            ExprKind::Num(n) => Value::Num(*n),
            ExprKind::Str(s) => Value::text(s),
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Nil => Value::Nil,
            ExprKind::Ident(name) => self.lookup(name, line)?,
            ExprKind::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for i in items {
                    out.push(self.eval(i)?);
                }
                Value::list(out)
            }
            ExprKind::Neg(inner) => match self.eval(inner)? {
                Value::Num(n) => Value::Num(-n),
                Value::Vec3(v) => Value::Vec3([-v[0], -v[1], -v[2]]),
                v => {
                    return Err(CryoError::new(
                        line,
                        format!("cannot negate `{}`", v.type_name()),
                    ))
                }
            },
            ExprKind::Not(inner) => Value::Bool(!self.cond(inner)?),
            ExprKind::Binary(BinOp::And, a, b) => Value::Bool(self.cond(a)? && self.cond(b)?),
            ExprKind::Binary(BinOp::Or, a, b) => Value::Bool(self.cond(a)? || self.cond(b)?),
            ExprKind::Binary(op, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                binary(*op, a, b, line)?
            }
            ExprKind::Member(obj, name) => {
                let base = self.eval(obj)?;
                self.member(&base, name, line)?
            }
            ExprKind::Index(obj, idx) => {
                let base = self.eval(obj)?;
                let i = self.num(idx)?;
                match base {
                    Value::List(l) => {
                        let l = l.borrow();
                        l[index(i, l.len(), line)?].clone()
                    }
                    other => {
                        return Err(CryoError::new(
                            line,
                            format!("`{}` cannot be indexed", other.type_name()),
                        ))
                    }
                }
            }
            ExprKind::Call(callee, args) => {
                let mut argv = Vec::with_capacity(args.len());
                for a in args {
                    argv.push(self.eval(a)?);
                }
                match &callee.kind {
                    ExprKind::Ident(name) => self.call_global(name, &argv, line)?,
                    ExprKind::Member(obj, name) => {
                        let base = self.eval(obj)?;
                        self.call_method(&base, name, &argv, line)?
                    }
                    _ => return Err(CryoError::new(line, "this is not a function")),
                }
            }
        })
    }

    fn member(&mut self, base: &Value, name: &str, line: usize) -> R<Value> {
        match (base, name) {
            (Value::Vec3(v), "x") => Ok(Value::Num(v[0])),
            (Value::Vec3(v), "y") => Ok(Value::Num(v[1])),
            (Value::Vec3(v), "z") => Ok(Value::Num(v[2])),
            (Value::Vec3(_), _) => Err(CryoError::new(
                line,
                format!("`vec3` has no member `{name}` (use x, y or z)"),
            )),
            (Value::Entity(_) | Value::Object(_), _) => {
                self.host.get_member(base, name).map_err(|m| CryoError::new(line, m))
            }
            (Value::Nil, _) => Err(CryoError::new(
                line,
                format!("tried to read `{name}` of `nil` (does the entity exist?)"),
            )),
            (other, _) => Err(CryoError::new(
                line,
                format!("`{}` has no member `{name}`", other.type_name()),
            )),
        }
    }

    fn call_method(&mut self, base: &Value, name: &str, args: &[Value], line: usize) -> R<Value> {
        let err = |m: String| CryoError::new(line, m);
        match base {
            Value::List(l) => match (name, args) {
                ("push", [v]) => {
                    l.borrow_mut().push(v.clone());
                    Ok(Value::Nil)
                }
                ("pop", []) => Ok(l.borrow_mut().pop().unwrap_or(Value::Nil)),
                ("len", []) => Ok(Value::Num(l.borrow().len() as f64)),
                ("clear", []) => {
                    l.borrow_mut().clear();
                    Ok(Value::Nil)
                }
                ("contains", [v]) => Ok(Value::Bool(l.borrow().contains(v))),
                ("remove", [Value::Num(i)]) => {
                    let mut l = l.borrow_mut();
                    let n = l.len();
                    Ok(l.remove(index(*i, n, line)?))
                }
                _ => Err(err(format!("`list` has no method `{name}` with these arguments"))),
            },
            Value::Text(s) => match (name, args) {
                ("len", []) => Ok(Value::Num(s.chars().count() as f64)),
                ("upper", []) => Ok(Value::text(s.to_uppercase())),
                ("lower", []) => Ok(Value::text(s.to_lowercase())),
                ("contains", [Value::Text(t)]) => Ok(Value::Bool(s.contains(&**t))),
                _ => Err(err(format!("`text` has no method `{name}` with these arguments"))),
            },
            Value::Vec3(v) => match (name, args) {
                ("length", []) => Ok(Value::Num(vlen(*v))),
                ("normalized", []) => Ok(Value::Vec3(vnorm(*v))),
                _ => Err(err(format!("`vec3` has no method `{name}`"))),
            },
            Value::Entity(_) | Value::Object(_) => {
                self.host.call(Some(base), name, args).map_err(err)
            }
            Value::Nil => Err(err(format!("tried to call `{name}` on `nil` (does the entity exist?)"))),
            other => Err(err(format!("`{}` has no method `{name}`", other.type_name()))),
        }
    }

    fn random(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.inst.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.inst.rng = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn call_global(&mut self, name: &str, args: &[Value], line: usize) -> R<Value> {
        // 1) the script's own functions
        let script = self.inst.script.clone();
        if let Some(f) = script.funcs.get(name) {
            return self.call_func(f, args, line, false);
        }
        // 2) built-in functions
        use Value::*;
        let bad = || {
            let tys: Vec<&str> = args.iter().map(|a| a.type_name()).collect();
            CryoError::new(line, format!("`{name}` does not accept the arguments ({})", tys.join(", ")))
        };
        let v = match (name, args) {
            ("vec3", [Num(x), Num(y), Num(z)]) => Vec3([*x, *y, *z]),
            ("vec3", [Num(x)]) => Vec3([*x, *x, *x]),
            ("vec3", []) => Vec3([0.0; 3]),
            ("abs", [Num(x)]) => Num(x.abs()),
            ("sign", [Num(x)]) => Num(if *x > 0.0 {
                1.0
            } else if *x < 0.0 {
                -1.0
            } else {
                0.0
            }),
            ("sin", [Num(x)]) => Num(x.sin()),
            ("cos", [Num(x)]) => Num(x.cos()),
            ("tan", [Num(x)]) => Num(x.tan()),
            ("atan2", [Num(y), Num(x)]) => Num(y.atan2(*x)),
            ("sqrt", [Num(x)]) if *x >= 0.0 => Num(x.sqrt()),
            ("sqrt", [Num(_)]) => {
                return Err(CryoError::new(line, "square root of a negative number"))
            }
            ("pow", [Num(a), Num(b)]) => Num(a.powf(*b)),
            ("floor", [Num(x)]) => Num(x.floor()),
            ("ceil", [Num(x)]) => Num(x.ceil()),
            ("round", [Num(x)]) => Num(x.round()),
            ("min", [Num(a), Num(b)]) => Num(a.min(*b)),
            ("max", [Num(a), Num(b)]) => Num(a.max(*b)),
            ("clamp", [Num(x), Num(lo), Num(hi)]) => Num(x.max(*lo).min(*hi)),
            ("lerp", [Num(a), Num(b), Num(t)]) => Num(a + (b - a) * t),
            ("lerp", [Vec3(a), Vec3(b), Num(t)]) => Vec3([
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]),
            ("random", []) => Num(self.random()),
            ("random", [Num(a), Num(b)]) => Num(a + (b - a) * self.random()),
            ("length", [Vec3(v)]) => Num(vlen(*v)),
            ("normalize", [Vec3(v)]) => Vec3(vnorm(*v)),
            ("dot", [Vec3(a), Vec3(b)]) => Num(a[0] * b[0] + a[1] * b[1] + a[2] * b[2]),
            ("cross", [Vec3(a), Vec3(b)]) => Vec3([
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]),
            ("distance", [Vec3(a), Vec3(b)]) => Num(vlen([a[0] - b[0], a[1] - b[1], a[2] - b[2]])),
            ("str", [v]) => Value::text(v.to_string()),
            ("num", [Text(s)]) => match s.trim().parse::<f64>() {
                Ok(n) => Num(n),
                Err(_) => Nil,
            },
            ("num", [Num(n)]) => Num(*n),
            ("len", [List(l)]) => Num(l.borrow().len() as f64),
            ("len", [Text(s)]) => Num(s.chars().count() as f64),
            _ if BUILTINS.contains(&name) && name != "log" => return Err(bad()),
            // 3) engine functions
            _ => return self.host.call(None, name, args).map_err(|m| CryoError::new(line, m)),
        };
        Ok(v)
    }
}

fn index(i: f64, len: usize, line: usize) -> R<usize> {
    if i.fract() != 0.0 || i < 0.0 || i as usize >= len {
        return Err(CryoError::new(
            line,
            format!("index {} out of range (list length {len})", fmt_num(i)),
        ));
    }
    Ok(i as usize)
}

fn vlen(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn vnorm(v: [f64; 3]) -> [f64; 3] {
    let l = vlen(v);
    if l < 1e-12 {
        [0.0; 3]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

fn binary(op: BinOp, a: Value, b: Value, line: usize) -> R<Value> {
    use BinOp::*;
    use Value::*;
    let v = match (op, &a, &b) {
        (Eq, _, _) => Bool(a == b),
        (Ne, _, _) => Bool(a != b),
        (Add, Num(x), Num(y)) => Num(x + y),
        (Sub, Num(x), Num(y)) => Num(x - y),
        (Mul, Num(x), Num(y)) => Num(x * y),
        (Div | Mod, Num(_), Num(y)) if *y == 0.0 => {
            return Err(CryoError::new(line, "division by zero"))
        }
        (Div, Num(x), Num(y)) => Num(x / y),
        (Mod, Num(x), Num(y)) => Num(x.rem_euclid(*y)),
        (Lt, Num(x), Num(y)) => Bool(x < y),
        (Le, Num(x), Num(y)) => Bool(x <= y),
        (Gt, Num(x), Num(y)) => Bool(x > y),
        (Ge, Num(x), Num(y)) => Bool(x >= y),
        (Add, Text(_), _) | (Add, _, Text(_)) => Value::text(format!("{a}{b}")),
        (Add, Vec3(x), Vec3(y)) => Vec3([x[0] + y[0], x[1] + y[1], x[2] + y[2]]),
        (Sub, Vec3(x), Vec3(y)) => Vec3([x[0] - y[0], x[1] - y[1], x[2] - y[2]]),
        (Mul, Vec3(x), Vec3(y)) => Vec3([x[0] * y[0], x[1] * y[1], x[2] * y[2]]),
        (Mul, Vec3(x), Num(s)) | (Mul, Num(s), Vec3(x)) => Vec3([x[0] * s, x[1] * s, x[2] * s]),
        (Div, Vec3(_), Num(s)) if *s == 0.0 => {
            return Err(CryoError::new(line, "division by zero"))
        }
        (Div, Vec3(x), Num(s)) => Vec3([x[0] / s, x[1] / s, x[2] / s]),
        _ => {
            let sym = match op {
                Add => "+",
                Sub => "-",
                Mul => "*",
                Div => "/",
                Mod => "%",
                Lt => "<",
                Le => "<=",
                Gt => ">",
                Ge => ">=",
                _ => "?",
            };
            return Err(CryoError::new(
                line,
                format!("operator `{sym}` does not work between `{}` and `{}`", a.type_name(), b.type_name()),
            ));
        }
    };
    Ok(v)
}
