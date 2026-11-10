//! Interpretador (tree-walking) da linguagem Cryo.

use crate::ast::*;
use crate::CryoError;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Valor em tempo de execução.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Num(f64),
    Bool(bool),
    Text(Rc<str>),
    Vec3([f64; 3]),
    /// Referência a uma entidade da cena (id).
    Entity(u64),
    List(Rc<RefCell<Vec<Value>>>),
    /// Objeto fornecido pela engine (`input`, `scene`, `time`...).
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
            Value::Object(_) => "objeto",
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

/// Ponte entre o script e a engine. Tudo que o Cryo não conhece
/// (entidades, `input`, `scene`, `log`...) é resolvido aqui.
pub trait Host {
    /// Nomes globais como `self`, `input`, `scene`, `time`.
    fn global(&mut self, name: &str) -> Option<Value>;
    fn get_member(&mut self, target: &Value, name: &str) -> Result<Value, String>;
    fn set_member(&mut self, target: &Value, name: &str, value: Value) -> Result<(), String>;
    /// `target = None` para funções globais (`log(...)`).
    fn call(&mut self, target: Option<&Value>, name: &str, args: &[Value])
        -> Result<Value, String>;
}

/// Host vazio: útil para testes e para avaliar valores padrão de `prop`.
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
        Err(format!("`{}` não tem o membro `{name}`", target.type_name()))
    }
    fn set_member(&mut self, target: &Value, name: &str, _v: Value) -> Result<(), String> {
        Err(format!("`{}` não tem o membro `{name}`", target.type_name()))
    }
    fn call(&mut self, target: Option<&Value>, name: &str, args: &[Value]) -> Result<Value, String> {
        if target.is_none() && name == "log" {
            let parts: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            self.logs.push(parts.join(" "));
            return Ok(Value::Nil);
        }
        Err(format!("função desconhecida `{name}`"))
    }
}

/// Funções embutidas (usado também pelo realce de sintaxe do editor).
pub const BUILTINS: &[&str] = &[
    "vec3", "abs", "sign", "sin", "cos", "tan", "atan2", "sqrt", "pow", "floor", "ceil", "round",
    "min", "max", "clamp", "lerp", "random", "length", "normalize", "dot", "cross", "distance",
    "str", "num", "len", "log",
];

/// Limite de instruções por chamada de evento: impede que um `while` infinito trave o editor.
const BUDGET: u64 = 2_000_000;
const MAX_DEPTH: usize = 120;

/// Uma instância de script anexada a uma entidade.
pub struct Instance {
    pub script: Rc<Script>,
    pub fields: HashMap<String, Value>,
    rng: u64,
}

impl Instance {
    /// Cria a instância avaliando os valores iniciais de `prop` e `var`.
    pub fn new(script: Rc<Script>, host: &mut dyn Host) -> Result<Self, CryoError> {
        let mut inst = Instance { script: script.clone(), fields: HashMap::new(), rng: 0x9E3779B97F4A7C15 };
        for f in &script.fields {
            let v = {
                let mut ex = Exec::new(&mut inst, host);
                ex.eval(&f.default)?
            };
            check_type(f.ty, &v, f.line, &format!("campo `{}`", f.name))?;
            inst.fields.insert(f.name.clone(), v);
        }
        Ok(inst)
    }

    pub fn has_handler(&self, event: &str) -> bool {
        self.script.handlers.contains_key(event)
    }

    /// Define o valor de um `prop` (vindo do Inspector), respeitando o tipo declarado.
    pub fn set_prop(&mut self, name: &str, value: Value) -> Result<(), CryoError> {
        let Some(f) = self.script.fields.iter().find(|f| f.name == name) else {
            return Ok(()); // prop antigo que não existe mais no script
        };
        check_type(f.ty, &value, f.line, &format!("campo `{}`", f.name))?;
        self.fields.insert(name.to_string(), value);
        Ok(())
    }

    /// Dispara um evento (`start`, `update`, ...). Eventos não declarados são ignorados.
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
        // Eventos aceitam menos argumentos do que os declarados? Não: mas aceitam
        // ser declarados com menos parâmetros do que a engine envia.
        let n = func.params.len().min(args.len());
        let mut ex = Exec::new(self, host);
        ex.call_func(func, &args[..n], func.line, true)
    }

    /// Chama uma função `fn` do script pelo nome.
    pub fn call_fn(
        &mut self,
        name: &str,
        args: &[Value],
        host: &mut dyn Host,
    ) -> Result<Value, CryoError> {
        let script = self.script.clone();
        let Some(func) = script.funcs.get(name) else {
            return Err(CryoError::new(0, format!("função `{name}` não existe no script")));
        };
        let mut ex = Exec::new(self, host);
        ex.call_func(func, args, func.line, false)
    }
}

/// Avalia os valores padrão dos campos `prop` sem precisar da engine.
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
            format!("{what} espera `{}`, mas recebeu `{}`", t.name(), v.type_name()),
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
                    "`{}` espera {} argumento(s), mas recebeu {}",
                    f.name,
                    f.params.len(),
                    args.len()
                ),
            ));
        }
        if self.depth >= MAX_DEPTH {
            return Err(CryoError::new(line, "recursão profunda demais"));
        }
        let mut scope = HashMap::new();
        for (p, a) in f.params.iter().zip(args) {
            check_type(p.ty, a, line, &format!("parâmetro `{}` de `{}`", p.name, f.name))?;
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
            check_type(f.ret, &v, f.line, &format!("retorno de `{}`", f.name))?;
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
                "execução longa demais (laço infinito?) — o evento foi interrompido",
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
                format!("a condição precisa ser `bool`, mas é `{}`", v.type_name()),
            )),
        }
    }

    fn stmt(&mut self, s: &Stmt) -> R<Flow> {
        self.tick(s.line)?;
        match &s.kind {
            StmtKind::Decl { name, ty, value, mutable } => {
                let v = self.eval(value)?;
                check_type(*ty, &v, s.line, &format!("variável `{name}`"))?;
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
                            format!("`for ... in` espera uma lista, mas recebeu `{}`", v.type_name()),
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
                format!("esperado `num`, mas recebeu `{}`", v.type_name()),
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
                                format!("`{name}` é imutável (`let`); declare com `var` para poder alterar"),
                            ));
                        }
                        check_type(l.ty, &v, line, &format!("variável `{name}`"))?;
                        l.value = v;
                        return Ok(());
                    }
                }
                if let Some(f) = self.inst.script.fields.iter().find(|f| &f.name == name) {
                    check_type(f.ty, &v, line, &format!("campo `{name}`"))?;
                    self.inst.fields.insert(name.clone(), v);
                    return Ok(());
                }
                Err(CryoError::new(line, format!("variável `{name}` não foi declarada")))
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
                                    format!("`vec3` não tem o membro `{name}` (use x, y ou z)"),
                                ))
                            }
                        };
                        let Value::Num(n) = v else {
                            return Err(CryoError::new(
                                line,
                                format!("`{name}` de um vec3 precisa ser `num`"),
                            ));
                        };
                        a[i] = n;
                        // escreve o vetor modificado de volta (ex.: self.position.y = 2)
                        self.assign(obj, Value::Vec3(a))
                    }
                    Value::Entity(_) | Value::Object(_) => self
                        .host
                        .set_member(&base, name, v)
                        .map_err(|m| CryoError::new(line, m)),
                    other => Err(CryoError::new(
                        line,
                        format!("não é possível alterar `{name}` em `{}`", other.type_name()),
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
                        format!("`{}` não pode ser indexado", other.type_name()),
                    )),
                }
            }
            _ => Err(CryoError::new(line, "destino de atribuição inválido")),
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
        Err(CryoError::new(line, format!("`{name}` não foi declarado")))
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
                        format!("não é possível negar `{}`", v.type_name()),
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
                            format!("`{}` não pode ser indexado", other.type_name()),
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
                    _ => return Err(CryoError::new(line, "isto não é uma função")),
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
                format!("`vec3` não tem o membro `{name}` (use x, y ou z)"),
            )),
            (Value::Entity(_) | Value::Object(_), _) => {
                self.host.get_member(base, name).map_err(|m| CryoError::new(line, m))
            }
            (Value::Nil, _) => Err(CryoError::new(
                line,
                format!("tentou ler `{name}` de `nil` (a entidade existe?)"),
            )),
            (other, _) => Err(CryoError::new(
                line,
                format!("`{}` não tem o membro `{name}`", other.type_name()),
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
                _ => Err(err(format!("`list` não tem o método `{name}` com esses argumentos"))),
            },
            Value::Text(s) => match (name, args) {
                ("len", []) => Ok(Value::Num(s.chars().count() as f64)),
                ("upper", []) => Ok(Value::text(s.to_uppercase())),
                ("lower", []) => Ok(Value::text(s.to_lowercase())),
                ("contains", [Value::Text(t)]) => Ok(Value::Bool(s.contains(&**t))),
                _ => Err(err(format!("`text` não tem o método `{name}` com esses argumentos"))),
            },
            Value::Vec3(v) => match (name, args) {
                ("length", []) => Ok(Value::Num(vlen(*v))),
                ("normalized", []) => Ok(Value::Vec3(vnorm(*v))),
                _ => Err(err(format!("`vec3` não tem o método `{name}`"))),
            },
            Value::Entity(_) | Value::Object(_) => {
                self.host.call(Some(base), name, args).map_err(err)
            }
            Value::Nil => Err(err(format!("tentou chamar `{name}` em `nil` (a entidade existe?)"))),
            other => Err(err(format!("`{}` não tem o método `{name}`", other.type_name()))),
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
        // 1) funções do próprio script
        let script = self.inst.script.clone();
        if let Some(f) = script.funcs.get(name) {
            return self.call_func(f, args, line, false);
        }
        // 2) funções embutidas
        use Value::*;
        let bad = || {
            let tys: Vec<&str> = args.iter().map(|a| a.type_name()).collect();
            CryoError::new(line, format!("`{name}` não aceita os argumentos ({})", tys.join(", ")))
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
                return Err(CryoError::new(line, "raiz quadrada de número negativo"))
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
            // 3) funções da engine
            _ => return self.host.call(None, name, args).map_err(|m| CryoError::new(line, m)),
        };
        Ok(v)
    }
}

fn index(i: f64, len: usize, line: usize) -> R<usize> {
    if i.fract() != 0.0 || i < 0.0 || i as usize >= len {
        return Err(CryoError::new(
            line,
            format!("índice {} fora da lista (tamanho {len})", fmt_num(i)),
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
            return Err(CryoError::new(line, "divisão por zero"))
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
            return Err(CryoError::new(line, "divisão por zero"))
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
                format!("operador `{sym}` não funciona entre `{}` e `{}`", a.type_name(), b.type_name()),
            ));
        }
    };
    Ok(v)
}
