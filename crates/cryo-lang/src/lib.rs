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
    fn maps() {
        let out = ok(r#"on start() {
            var inv = { coins: 2, "red key": true }
            inv.coins += 3
            inv["gems"] = 1
            log(inv.coins, inv["red key"], inv.missing, inv.len())
            for k in inv { log(k) }
            inv.remove("gems")
            log(inv.has("gems"), inv)
        }"#);
        assert_eq!(out, ["5 true nil 3", "coins", "gems", "red key", "false {coins: 5, red key: true}"]);
    }

    #[test]
    fn match_statement() {
        let out = ok(r#"
        fn kind(x) -> text {
            match x {
                1, 2 => return "low"
                "a" => { return "letter" }
                else => return "other"
            }
            return "never"
        }
        on start() { log(kind(2), kind("a"), kind(9)) }"#);
        assert_eq!(out, ["low letter other"]);
    }

    #[test]
    fn state_machines() {
        let src = r#"script Door {
            var state = "a variable may still be called state"
            on start() { log("start") }
            on open() { goto opened }
            state closed {
                on enter() { log("enter closed") }
                on update(dt) { log("closed", state_time()) }
                on exit() { log("exit closed") }
            }
            state opened {
                on enter() { log("enter opened") }
                on update(dt) { log("opened", current_state()); goto closed; log("unreachable") }
            }
        }"#;
        let script = Rc::new(parse(src).unwrap());
        let mut host = NullHost::new();
        let mut inst = Instance::new(script, &mut host).unwrap();
        inst.call_event("start", &[], &mut host).unwrap();
        inst.call_event("update", &[Value::Num(0.5)], &mut host).unwrap();
        inst.call_event("open", &[], &mut host).unwrap();
        inst.call_event("update", &[Value::Num(0.5)], &mut host).unwrap();
        assert_eq!(
            host.logs,
            [
                "start", "enter closed", "closed 0.5", "exit closed", "enter opened",
                "opened opened", "enter closed"
            ]
        );
        assert_eq!(inst.state_name(), "closed");
        assert!(fail("on start() { goto nowhere }").contains("no state `nowhere`"));
    }

    #[test]
    fn text_and_list_helpers() {
        let out = ok(r#"on start() {
            var l = [3, 1, 2]
            l.sort()
            l.insert(0, 0)
            log(l, l.index_of(2), l.join("-"), range(3))
            log("a,b,c".split(","), " hi ".trim(), "frost".sub(1, 3), "frost"[0])
            log(format("{} of {}", 2, 5), type(l), move_toward(0, 10, 4), wrap(370, 0, 360))
        }"#);
        assert_eq!(out, ["[0, 1, 2, 3] 2 0-1-2-3 [0, 1, 2]", "[a, b, c] hi ros f", "2 of 5 list 4 10"]);
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
