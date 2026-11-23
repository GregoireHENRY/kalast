//! The maths a number field reads when typed into, as Blender's do: "2*pi",
//! "90/4", or "*2" to double what the field holds.
//!
//! Blender hands the text to Python. kalast reads a small language of its
//! own -- Python's arithmetic and nothing more: its operators with their
//! precedence, the functions of `math` a number wants and `abs`, `round`,
//! `min` and `max`, `pi`, `tau` and `e` -- as the UI app runs without
//! Python, and a number field has no business running code. Two things
//! Python does not read: a number written against a name or a parenthesis
//! multiplies it ("2pi", "3(1+1)"), and an operator first takes the field's
//! value as its left side ("*2").

use std::f64::consts::{E, PI, TAU};

/// How deep parentheses and signs may nest: far more than anyone types, and
/// a pasted wall of them is refused rather than overflowing the stack.
const MAX_DEPTH: usize = 100;

/// The value `text` stands for, in a field holding `current`; `None` when
/// it is not an expression this reads, or comes to something not finite.
pub fn eval(text: &str, current: f64) -> Option<f64> {
    let mut tokens = lex(text)?;
    // "*2" doubles the value, "+1" adds one to it; "-1" is minus one.
    if let Some(Token::Op('+' | '*' | '/' | '%' | '^')) = tokens.first() {
        tokens.insert(0, Token::Num(current));
    }
    let mut parser = Parser { tokens, at: 0, depth: 0 };
    let value = parser.sum()?;
    (parser.at == parser.tokens.len()).then_some(value).and_then(finite)
}

/// What the text is cut into: numbers, lower-cased names, and operators,
/// parentheses and commas, a character each -- `**` is read as `^`.
#[derive(Clone, PartialEq)]
enum Token {
    Num(f64),
    Name(String),
    Op(char),
}

/// `text` as tokens, spaces only parting them; `None` at a character this
/// does not read.
fn lex(text: &str) -> Option<Vec<Token>> {
    let b = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == b'.' {
            while b.get(i).is_some_and(|&d| d.is_ascii_digit() || d == b'.') {
                i += 1;
            }
            // An exponent only when digits follow: "2e" is 2 times e, as "2pi"
            // is 2 times pi.
            if matches!(b.get(i), Some(b'e' | b'E')) {
                let sign = usize::from(matches!(b.get(i + 1), Some(b'+' | b'-')));
                if b.get(i + 1 + sign).is_some_and(u8::is_ascii_digit) {
                    i += 1 + sign;
                    while b.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1;
                    }
                }
            }
            tokens.push(Token::Num(text[start..i].parse().ok()?));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while b.get(i).is_some_and(|&d| d.is_ascii_alphanumeric() || d == b'_') {
                i += 1;
            }
            tokens.push(Token::Name(text[start..i].to_ascii_lowercase()));
        } else if c == b'*' && b.get(i + 1) == Some(&b'*') {
            tokens.push(Token::Op('^'));
            i += 2;
        } else if b"+-*/%^(),".contains(&c) {
            tokens.push(Token::Op(char::from(c)));
            i += 1;
        } else {
            return None;
        }
    }
    Some(tokens)
}

/// A recursive descent, one method to each of Python's levels of
/// precedence, the loosest first.
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser {
    /// Steps over `op` if it comes next.
    fn eat(&mut self, op: char) -> bool {
        let next = self.tokens.get(self.at) == Some(&Token::Op(op));
        self.at += usize::from(next);
        next
    }

    /// `a + b - c`.
    fn sum(&mut self) -> Option<f64> {
        let mut x = self.product()?;
        loop {
            x = finite(if self.eat('+') {
                x + self.product()?
            } else if self.eat('-') {
                x - self.product()?
            } else {
                return Some(x);
            })?;
        }
    }

    /// `a * b / c % d`, from left to right.
    fn product(&mut self) -> Option<f64> {
        let mut x = self.unary()?;
        loop {
            x = finite(if self.eat('*') {
                x * self.unary()?
            } else if self.eat('/') {
                x / nonzero(self.unary()?)?
            } else if self.eat('%') {
                modulo(x, nonzero(self.unary()?)?)
            } else {
                return Some(x);
            })?;
        }
    }

    /// `-a`, `+a`: looser than a power, so that -2**2 is -4, as in Python.
    fn unary(&mut self) -> Option<f64> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        let x = if self.eat('-') {
            -self.unary()?
        } else if self.eat('+') {
            self.unary()?
        } else {
            self.implied()?
        };
        self.depth -= 1;
        Some(x)
    }

    /// A number or a closed parenthesis written against a name or an open
    /// one: a product with no `*`. Tighter than `*` and `/`, so that it
    /// reads as one quantity, as on paper -- 1/2pi is 1/(2*pi) -- and
    /// looser than a power: 2pi^2 is 2*pi^2.
    fn implied(&mut self) -> Option<f64> {
        let mut x = self.power()?;
        while matches!(self.tokens.get(self.at - 1), Some(Token::Num(_) | Token::Op(')')))
            && matches!(self.tokens.get(self.at), Some(Token::Name(_) | Token::Op('(')))
        {
            x = finite(x * self.power()?)?;
        }
        Some(x)
    }

    /// `a ** b`, or `a ^ b`: from right to left, the exponent signed or not,
    /// so that 2^3^2 is 2^9 and 2**-1 a half.
    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        if self.eat('^') { finite(base.powf(self.unary()?)) } else { Some(base) }
    }

    /// A number, a constant, a function's call, or a parenthesis.
    fn atom(&mut self) -> Option<f64> {
        let token = self.tokens.get(self.at)?.clone();
        self.at += 1;
        match token {
            Token::Num(x) => finite(x),
            Token::Op('(') => {
                let x = self.sum()?;
                self.eat(')').then_some(x)
            }
            Token::Name(name) if self.eat('(') => {
                let mut args = vec![self.sum()?];
                while self.eat(',') {
                    args.push(self.sum()?);
                }
                if self.eat(')') { call(&name, &args) } else { None }
            }
            Token::Name(name) => match name.as_str() {
                "pi" => Some(PI),
                "tau" => Some(TAU),
                "e" => Some(E),
                _ => None,
            },
            Token::Op(_) => None,
        }
    }
}

/// The function `name` on `args`, as Python's `math` and builtins have it,
/// and `ln`; `None` for a name or a number of arguments it does not take,
/// or a result out of the reals -- where Python would raise, as for
/// sqrt(-1) or log(0).
fn call(name: &str, args: &[f64]) -> Option<f64> {
    finite(match (name, args) {
        ("sin", [x]) => x.sin(),
        ("cos", [x]) => x.cos(),
        ("tan", [x]) => x.tan(),
        ("asin", [x]) => x.asin(),
        ("acos", [x]) => x.acos(),
        ("atan", [x]) => x.atan(),
        ("atan2", [y, x]) => y.atan2(*x),
        ("sqrt", [x]) => x.sqrt(),
        ("exp", [x]) => x.exp(),
        ("log" | "ln", [x]) => x.ln(),
        ("log", [x, base]) => x.ln() / finite(base.ln())?,
        ("log10", [x]) => x.log10(),
        ("log2", [x]) => x.log2(),
        ("abs", [x]) => x.abs(),
        ("floor", [x]) => x.floor(),
        ("ceil", [x]) => x.ceil(),
        // Half to even, as Python rounds: round(2.5) is 2.
        ("round", [x]) => x.round_ties_even(),
        ("min", [_, ..]) => args.iter().copied().fold(f64::INFINITY, f64::min),
        ("max", [_, ..]) => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ("pow", [x, y]) => x.powf(*y),
        ("hypot", [x, y]) => x.hypot(*y),
        ("radians", [x]) => x.to_radians(),
        ("degrees", [x]) => x.to_degrees(),
        _ => return None,
    })
}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

/// A divisor, refused when zero: Python raises rather than give infinity,
/// which a later step could turn back into a number.
fn nonzero(x: f64) -> Option<f64> {
    (x != 0.0).then_some(x)
}

/// Python's `%`: the remainder takes the divisor's sign, so -7 % 3 is 2
/// where Rust's is -1, and a zero is signed as the divisor is.
fn modulo(x: f64, y: f64) -> f64 {
    let r = x % y;
    if r == 0.0 {
        0f64.copysign(y)
    } else if (r < 0.0) != (y < 0.0) {
        r + y
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn reads(text: &str, current: f64, expected: f64) {
        let value = eval(text, current).unwrap_or_else(|| panic!("{text:?} not read"));
        assert!((value - expected).abs() <= 1e-12 * expected.abs().max(1.0), "{text:?} is {value}, not {expected}");
    }

    #[test]
    fn arithmetic_has_pythons_precedence() {
        reads("2*pi", 0.0, 2.0 * PI);
        reads("90/4", 0.0, 22.5);
        reads("1+2*3", 0.0, 7.0);
        reads("(1+2)*3", 0.0, 9.0);
        reads("-1", 0.0, -1.0);
        reads("2*-3", 0.0, -6.0);
        reads("--1", 0.0, 1.0);
        reads("-2**2", 0.0, -4.0);
        reads("2^3^2", 0.0, 512.0);
        reads("2**3**2", 0.0, 512.0);
        reads("2**-1", 0.0, 0.5);
        reads("-7 % 3", 0.0, 2.0);
        reads("7 % -3", 0.0, -2.0);
        reads("7.5 % 2", 0.0, 1.5);
        assert!(eval("-7 % 7", 0.0).is_some_and(f64::is_sign_positive), "a zero signed as the divisor");
    }

    #[test]
    fn numbers_are_written_as_in_python() {
        reads("1e3", 0.0, 1000.0);
        reads("1e-3", 0.0, 0.001);
        reads("2.5E+4", 0.0, 25000.0);
        reads(".5", 0.0, 0.5);
        reads("5.", 0.0, 5.0);
        reads("  42  ", 0.0, 42.0);
        reads(" 1 + 2 ", 0.0, 3.0);
    }

    #[test]
    fn an_operator_first_takes_the_current_value() {
        reads("*2", 3.0, 6.0);
        reads("/2", 5.0, 2.5);
        reads("+1", 1.0, 2.0);
        reads("^2", 3.0, 9.0);
        reads("**2", -3.0, 9.0);
        reads("%3", 7.0, 1.0);
        reads("*2+1", 3.0, 7.0);
        reads("-1", 5.0, -1.0);
    }

    #[test]
    fn functions_and_constants_are_pythons() {
        reads("sqrt(2)", 0.0, 2f64.sqrt());
        reads("radians(180)", 0.0, PI);
        reads("degrees(pi/2)", 0.0, 90.0);
        reads("max(1, 5, 3)", 0.0, 5.0);
        reads("min(4)", 0.0, 4.0);
        reads("log(8, 2)", 0.0, 3.0);
        reads("log(e)", 0.0, 1.0);
        reads("ln(e**2)", 0.0, 2.0);
        reads("log10(1000)", 0.0, 3.0);
        reads("log2(8)", 0.0, 3.0);
        reads("exp(0)", 0.0, 1.0);
        reads("sin(pi/2) + cos(0) + tan(0)", 0.0, 2.0);
        reads("asin(1) + acos(1) + atan(1)", 0.0, 0.75 * PI);
        reads("atan2(1, -1)", 0.0, 0.75 * PI);
        reads("hypot(3, 4)", 0.0, 5.0);
        reads("pow(2, 10)", 0.0, 1024.0);
        reads("abs(-3) + floor(-1.5) + ceil(1.2)", 0.0, 3.0);
        reads("tau", 0.0, TAU);
        reads("SQRT(4) * Pi", 0.0, 2.0 * PI);
    }

    /// Half to even, as Python's `round`, not half away from zero.
    #[test]
    fn round_takes_halves_to_even() {
        reads("round(2.5)", 0.0, 2.0);
        reads("round(3.5)", 0.0, 4.0);
        reads("round(-2.5)", 0.0, -2.0);
        reads("round(2.6)", 0.0, 3.0);
    }

    #[test]
    fn a_number_against_a_name_or_parenthesis_multiplies() {
        reads("2pi", 0.0, 2.0 * PI);
        reads("2 pi", 0.0, 2.0 * PI);
        reads("3(1+1)", 0.0, 6.0);
        reads("2sqrt(4)", 0.0, 4.0);
        reads("(1+1)(2+2)", 0.0, 8.0);
        reads("2e", 0.0, 2.0 * E);
        reads("2e3", 0.0, 2000.0);
        reads("1/2pi", 0.0, 0.5 / PI);
        reads("2pi^2", 0.0, 2.0 * PI * PI);
        reads("-2pi", 0.0, -2.0 * PI);
        reads("*2pi", 1.5, 3.0 * PI);
    }

    #[test]
    fn what_is_not_an_expression_is_none() {
        let texts = ["", "   ", "abc", "(1+2", "1+2)", "90/", "*", "2 3", "1..2", ".", "1,5", "2 = 2", "1 // 2", "×2"];
        let calls = ["sqrt", "sqrt()", "sqrt(1, 2)", "pi(2)", "nope(1)"];
        for text in texts.into_iter().chain(calls) {
            assert_eq!(eval(text, 1.0), None, "{text:?}");
        }
    }

    #[test]
    fn what_is_not_a_finite_number_is_none() {
        let raising = ["1/0", "1 % 0", "1/(1/0)", "0**-1", "sqrt(-1)", "log(0)", "log(2, 0)", "log(1, 1)", "asin(2)"];
        for text in raising.into_iter().chain(["1e999", "10**400", "exp(1000)"]) {
            assert_eq!(eval(text, 1.0), None, "{text:?}");
        }
        assert_eq!(eval("(-8)**(1/3)", 0.0), None, "a complex root");
        assert_eq!(eval("+1", f64::NAN), None, "a field holding no number");
        assert_eq!(eval("*2", f64::INFINITY), None);
    }

    #[test]
    fn deep_nesting_is_refused_not_overflowed() {
        reads(&format!("{}1{}", "(".repeat(50), ")".repeat(50)), 0.0, 1.0);
        assert_eq!(eval(&format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000)), 0.0), None);
        assert_eq!(eval(&format!("{}1", "-".repeat(100_000)), 0.0), None);
    }
}
