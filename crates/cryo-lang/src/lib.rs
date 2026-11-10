//! # Cryo
//!
//! Linguagem de script da Cryo Engine. Simples, estável e previsível:
//!
//! ```text
//! script Jogador {
//!     prop velocidade: num = 5
//!     var pontos = 0
//!
//!     on update(dt) {
//!         let dir = vec3(input.axis("horizontal"), 0, -input.axis("vertical"))
//!         self.position += dir * velocidade * dt
//!     }
//! }
//! ```

pub mod ast;
pub mod interp;
pub mod lexer;
pub mod parser;

pub use ast::{Script, Type};
pub use interp::{default_props, Host, Instance, NullHost, Value, BUILTINS};
pub use parser::parse;

use std::fmt;

/// Erro de compilação ou de execução, sempre com a linha do script.
#[derive(Debug, Clone, PartialEq)]
pub struct CryoError {
    pub line: usize,
    pub message: String,
}

impl CryoError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        CryoError { line, message: message.into() }
    }
}

impl fmt::Display for CryoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "linha {}: {}", self.line, self.message)
    }
}

impl std::error::Error for CryoError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn run(body: &str) -> Result<Vec<String>, CryoError> {
        let src = format!("script T {{\n{body}\n}}");
        let script = Rc::new(parse(&src)?);
        let mut host = NullHost::new();
        let mut inst = Instance::new(script, &mut host)?;
        inst.call_event("start", &[], &mut host)?;
        Ok(host.logs)
    }

    fn ok(body: &str) -> Vec<String> {
        run(body).unwrap_or_else(|e| panic!("erro inesperado: {e}"))
    }

    fn fail(body: &str) -> String {
        run(body).expect_err("deveria falhar").message
    }

    #[test]
    fn aritmetica_e_precedencia() {
        assert_eq!(ok("on start() { log(1 + 2 * 3, (1 + 2) * 3, 7 % 3, -2 * 2) }"), ["7 9 1 -4"]);
        assert_eq!(ok("on start() { log(10 / 4, 3 > 2 and not (1 == 2)) }"), ["2.5 true"]);
    }

    #[test]
    fn campos_e_funcoes() {
        let out = ok("prop vel: num = 2\nvar total = 0\n\
            fn dobro(x: num) -> num { return x * 2 }\n\
            on start() { total += dobro(vel) \n total += 1 \n log(total) }");
        assert_eq!(out, ["5"]);
    }

    #[test]
    fn vetores() {
        let out = ok("on start() {\n var p = vec3(1, 2, 3)\n p.y = 10\n p += vec3(1) * 2\n log(p, length(vec3(3, 4, 0))) }");
        assert_eq!(out, ["(3, 12, 5) 5"]);
    }

    #[test]
    fn lacos_e_listas() {
        let out = ok("on start() {\n var s = 0\n for i in 0..5 { if i == 3 { continue } \n s += i }\n\
            let l = [1, 2, 3]\n l.push(4)\n for x in l { s += x }\n log(s, len(l), l[0]) }");
        assert_eq!(out, ["17 4 1"]);
    }

    #[test]
    fn if_else_e_while() {
        let out = ok("on start() {\n var n = 0\n while true { n += 1 \n if n >= 4 { break } }\n\
            if n < 2 { log(\"a\") } else if n == 4 { log(\"b\") } else { log(\"c\") } }");
        assert_eq!(out, ["b"]);
    }

    #[test]
    fn texto() {
        assert_eq!(ok("on start() { log(\"pontos: \" + 3, \"abc\".upper()) }"), ["pontos: 3 ABC"]);
    }

    #[test]
    fn menos_em_nova_linha_e_outra_instrucao() {
        assert_eq!(ok("on start() {\n let a = 5\n -a\n log(a) }"), ["5"]);
    }

    #[test]
    fn let_e_imutavel() {
        assert!(fail("on start() { let a = 1 \n a = 2 }").contains("imutável"));
    }

    #[test]
    fn tipos_sao_verificados() {
        assert!(fail("prop v: num = \"oi\"").contains("espera `num`"));
        assert!(fail("fn f(x: num) { }\n on start() { f(true) }").contains("espera `num`"));
        assert!(fail("on start() { if 1 { } }").contains("bool"));
    }

    #[test]
    fn erros_seguros() {
        assert!(fail("on start() { log(1 / 0) }").contains("divisão por zero"));
        assert!(fail("on start() { let l = [1] \n log(l[1]) }").contains("fora da lista"));
        assert!(fail("on start() { while true { } }").contains("longa demais"));
        assert!(fail("on start() { log(x) }").contains("não foi declarado"));
        assert!(fail("fn f() { f() }\n on start() { f() }").contains("recursão"));
    }

    #[test]
    fn erro_de_sintaxe_tem_linha() {
        let e = parse("script A {\n on start() {\n let = 3\n }\n}").unwrap_err();
        assert_eq!(e.line, 3);
    }

    #[test]
    fn props_padrao() {
        let s = Rc::new(parse("script A { prop a: num = 2 * 3\n var b = 1\n prop c = vec3(1) }").unwrap());
        let p = default_props(&s);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].2, Value::Num(6.0));
        assert_eq!(p[1].2, Value::Vec3([1.0; 3]));
    }
}
