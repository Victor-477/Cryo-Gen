//! # Cryo
//!
//! Scripting language of the Cryo Engine. Simple, stable and predictable:
//!
//! ```text
//! script Player {
//!     prop speed: num = 5
//!     var points = 0
//!
//!     on update(dt) {
//!         let dir = vec3(input.axis("horizontal"), 0, -input.axis("vertical"))
//!         self.position += dir * speed * dt
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

/// Compile-time or runtime error, always with the script line.
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
        write!(f, "line {}: {}", self.line, self.message)
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
        run(body).unwrap_or_else(|e| panic!("unexpected error: {e}"))
    }

    fn fail(body: &str) -> String {
        run(body).expect_err("should fail").message
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ok("on start() { log(1 + 2 * 3, (1 + 2) * 3, 7 % 3, -2 * 2) }"), ["7 9 1 -4"]);
        assert_eq!(ok("on start() { log(10 / 4, 3 > 2 and not (1 == 2)) }"), ["2.5 true"]);
    }

    #[test]
    fn fields_and_functions() {
        let out = ok("prop speed: num = 2\nvar total = 0\n\
            fn double(x: num) -> num { return x * 2 }\n\
            on start() { total += double(speed) \n total += 1 \n log(total) }");
        assert_eq!(out, ["5"]);
    }

    #[test]
    fn vectors() {
        let out = ok("on start() {\n var p = vec3(1, 2, 3)\n p.y = 10\n p += vec3(1) * 2\n log(p, length(vec3(3, 4, 0))) }");
        assert_eq!(out, ["(3, 12, 5) 5"]);
    }

    #[test]
    fn loops_and_lists() {
        let out = ok("on start() {\n var s = 0\n for i in 0..5 { if i == 3 { continue } \n s += i }\n\
            let l = [1, 2, 3]\n l.push(4)\n for x in l { s += x }\n log(s, len(l), l[0]) }");
        assert_eq!(out, ["17 4 1"]);
    }

    #[test]
    fn if_else_and_while() {
        let out = ok("on start() {\n var n = 0\n while true { n += 1 \n if n >= 4 { break } }\n\
            if n < 2 { log(\"a\") } else if n == 4 { log(\"b\") } else { log(\"c\") } }");
        assert_eq!(out, ["b"]);
    }

    #[test]
    fn text() {
        assert_eq!(ok("on start() { log(\"points: \" + 3, \"abc\".upper()) }"), ["points: 3 ABC"]);
    }

    #[test]
    fn minus_on_new_line_is_another_statement() {
        assert_eq!(ok("on start() {\n let a = 5\n -a\n log(a) }"), ["5"]);
    }

    #[test]
    fn let_is_immutable() {
        assert!(fail("on start() { let a = 1 \n a = 2 }").contains("immutable"));
    }

    #[test]
    fn types_are_checked() {
        assert!(fail("prop v: num = \"hi\"").contains("expects `num`"));
        assert!(fail("fn f(x: num) { }\n on start() { f(true) }").contains("expects `num`"));
        assert!(fail("on start() { if 1 { } }").contains("bool"));
    }

    #[test]
    fn safe_errors() {
        assert!(fail("on start() { log(1 / 0) }").contains("division by zero"));
        assert!(fail("on start() { let l = [1] \n log(l[1]) }").contains("out of range"));
        assert!(fail("on start() { while true { } }").contains("too long"));
        assert!(fail("on start() { log(x) }").contains("was not declared"));
        assert!(fail("fn f() { f() }\n on start() { f() }").contains("recursion"));
    }

    #[test]
    fn syntax_error_has_line() {
        let e = parse("script A {\n on start() {\n let = 3\n }\n}").unwrap_err();
        assert_eq!(e.line, 3);
    }

    #[test]
    fn default_props_are_evaluated() {
        let s = Rc::new(parse("script A { prop a: num = 2 * 3\n var b = 1\n prop c = vec3(1) }").unwrap());
        let p = default_props(&s);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].2, Value::Num(6.0));
        assert_eq!(p[1].2, Value::Vec3([1.0; 3]));
    }
}
