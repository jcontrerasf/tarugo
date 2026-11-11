//! Arithmetic expressions for dimensions: numbers, parameter names, `+ - * /`, parentheses and
//! the functions `min`, `max`, `abs`, `round`, `floor`, `ceil`, `sqrt`.
//!
//! Outside a function call a comma between digits is a decimal separator (`12,5`), the way
//! measures are typed in Spanish; inside a call it separates arguments (`;` always does).

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Var(String),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Min,
    Max,
    Abs,
    Round,
    Floor,
    Ceil,
    Sqrt,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprError {
    Empty,
    /// Something unexpected at this byte offset.
    Syntax(usize),
    UnknownName(String),
    UnknownFunction(String),
    /// A function called with the wrong number of arguments.
    Arity(String),
    DivisionByZero,
    /// The result is not a finite number.
    NotFinite,
    /// A parameter that depends on itself.
    Cycle(String),
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprError::Empty => write!(f, "empty"),
            ExprError::Syntax(at) => write!(f, "syntax error at {at}"),
            ExprError::UnknownName(n) => write!(f, "unknown name '{n}'"),
            ExprError::UnknownFunction(n) => write!(f, "unknown function '{n}'"),
            ExprError::Arity(n) => write!(f, "wrong number of arguments for '{n}'"),
            ExprError::DivisionByZero => write!(f, "division by zero"),
            ExprError::NotFinite => write!(f, "not a finite number"),
            ExprError::Cycle(n) => write!(f, "'{n}' depends on itself"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A number as typed.
    Num(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Open,
    Close,
    Comma,
    Semi,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Tokens with their byte offset.
fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let mut out = Vec::new();
    let mut chars = src.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let simple = match c {
            '+' => Some(Tok::Plus),
            '-' => Some(Tok::Minus),
            '*' => Some(Tok::Star),
            '/' => Some(Tok::Slash),
            '(' => Some(Tok::Open),
            ')' => Some(Tok::Close),
            ',' => Some(Tok::Comma),
            ';' => Some(Tok::Semi),
            _ => None,
        };
        if let Some(tok) = simple {
            chars.next();
            out.push((tok, start));
            continue;
        }
        let accept: fn(char) -> bool = if c.is_ascii_digit() || c == '.' {
            |c| c.is_ascii_digit() || c == '.'
        } else if is_ident_start(c) {
            is_ident_char
        } else {
            return Err(ExprError::Syntax(start));
        };
        let mut end = start;
        while let Some(&(i, ch)) = chars.peek() {
            if !accept(ch) {
                break;
            }
            end = i + ch.len_utf8();
            chars.next();
        }
        let text = src[start..end].to_string();
        out.push((
            if is_ident_start(c) {
                Tok::Ident(text)
            } else {
                Tok::Num(text)
            },
            start,
        ));
    }
    Ok(out)
}

/// Is `name` usable as a parameter name?
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_ident_start)
        && chars.all(is_ident_char)
        && func_by_name(name).is_none()
}

fn func_by_name(name: &str) -> Option<Func> {
    Some(match name {
        "min" => Func::Min,
        "max" => Func::Max,
        "abs" => Func::Abs,
        "round" => Func::Round,
        "floor" => Func::Floor,
        "ceil" => Func::Ceil,
        "sqrt" => Func::Sqrt,
        _ => return None,
    })
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    end: usize,
    /// Inside the arguments of a call, where a comma separates them.
    in_call: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn at(&self) -> usize {
        self.toks.get(self.pos).map_or(self.end, |t| t.1)
    }

    fn sum(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.product()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => Op::Add,
                Some(Tok::Minus) => Op::Sub,
                _ => return Ok(left),
            };
            self.pos += 1;
            let right = self.product()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn product(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => Op::Mul,
                Some(Tok::Slash) => Op::Div,
                _ => return Ok(left),
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        let at = self.at();
        let tok = self.peek().cloned().ok_or(ExprError::Syntax(at))?;
        self.pos += 1;
        match tok {
            Tok::Num(mut text) => {
                // `12,5` outside a call: decimal comma.
                if !self.in_call
                    && !text.contains('.')
                    && self.peek() == Some(&Tok::Comma)
                    && let Some((Tok::Num(frac), _)) = self.toks.get(self.pos + 1)
                    && !frac.contains('.')
                {
                    text = format!("{text}.{frac}");
                    self.pos += 2;
                }
                text.parse()
                    .map(Expr::Num)
                    .map_err(|_| ExprError::Syntax(at))
            }
            Tok::Ident(name) => {
                if self.peek() != Some(&Tok::Open) {
                    return Ok(Expr::Var(name));
                }
                let func =
                    func_by_name(&name).ok_or_else(|| ExprError::UnknownFunction(name.clone()))?;
                self.pos += 1;
                let outer = std::mem::replace(&mut self.in_call, true);
                let mut args = vec![self.sum()?];
                while matches!(self.peek(), Some(Tok::Comma | Tok::Semi)) {
                    self.pos += 1;
                    args.push(self.sum()?);
                }
                self.in_call = outer;
                self.expect_close()?;
                let ok = match func {
                    Func::Min | Func::Max => true,
                    _ => args.len() == 1,
                };
                if !ok {
                    return Err(ExprError::Arity(name));
                }
                Ok(Expr::Call(func, args))
            }
            Tok::Open => {
                let inner = self.sum()?;
                self.expect_close()?;
                Ok(inner)
            }
            _ => Err(ExprError::Syntax(at)),
        }
    }

    fn expect_close(&mut self) -> Result<(), ExprError> {
        if self.peek() == Some(&Tok::Close) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ExprError::Syntax(self.at()))
        }
    }
}

pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err(ExprError::Empty);
    }
    let mut p = Parser {
        toks,
        pos: 0,
        end: src.len(),
        in_call: false,
    };
    let expr = p.sum()?;
    if p.pos < p.toks.len() {
        return Err(ExprError::Syntax(p.at()));
    }
    Ok(expr)
}

impl Expr {
    /// Value of the expression; `lookup` gives the value of a name.
    pub fn eval(
        &self,
        lookup: &mut dyn FnMut(&str) -> Result<f64, ExprError>,
    ) -> Result<f64, ExprError> {
        let v = match self {
            Expr::Num(n) => *n,
            Expr::Var(name) => lookup(name)?,
            Expr::Neg(e) => -e.eval(lookup)?,
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(lookup)?, b.eval(lookup)?);
                match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                    Op::Div => {
                        if b == 0.0 {
                            return Err(ExprError::DivisionByZero);
                        }
                        a / b
                    }
                }
            }
            Expr::Call(func, args) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(a.eval(lookup)?);
                }
                let first = values[0];
                match func {
                    Func::Min => values.iter().copied().fold(first, f64::min),
                    Func::Max => values.iter().copied().fold(first, f64::max),
                    Func::Abs => first.abs(),
                    Func::Round => first.round(),
                    Func::Floor => first.floor(),
                    Func::Ceil => first.ceil(),
                    Func::Sqrt => first.sqrt(),
                }
            }
        };
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ExprError::NotFinite)
        }
    }

    /// Names the expression uses.
    pub fn names(&self, out: &mut Vec<String>) {
        match self {
            Expr::Num(_) => {}
            Expr::Var(n) => {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
            Expr::Neg(e) => e.names(out),
            Expr::Bin(_, a, b) => {
                a.names(out);
                b.names(out);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.names(out)),
        }
    }
}

/// Rewrites `src` with every use of the name `from` replaced by `to`. Text that does not
/// tokenize is returned unchanged.
pub fn rename(src: &str, from: &str, to: &str) -> String {
    let Ok(toks) = tokenize(src) else {
        return src.to_string();
    };
    let mut out = String::with_capacity(src.len());
    let mut last = 0;
    for (i, (tok, at)) in toks.iter().enumerate() {
        let is_call = matches!(toks.get(i + 1), Some((Tok::Open, _)));
        if let Tok::Ident(name) = tok
            && name == from
            && !is_call
        {
            out.push_str(&src[last..*at]);
            out.push_str(to);
            last = at + name.len();
        }
    }
    out.push_str(&src[last..]);
    out
}

/// A number the way it is shown in a dimension field: up to two decimals, no trailing zeros.
pub fn format_number(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let mut s = format!("{rounded:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str) -> Result<f64, ExprError> {
        parse(src)?.eval(&mut |name| match name {
            "W" => Ok(800.0),
            "t" => Ok(18.0),
            other => Err(ExprError::UnknownName(other.to_string())),
        })
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(eval("1 + 2 * 3"), Ok(7.0));
        assert_eq!(eval("(1 + 2) * 3"), Ok(9.0));
        assert_eq!(eval("-4 + 10 / 4"), Ok(-1.5));
        assert_eq!(eval("2 - 3 - 4"), Ok(-5.0));
    }

    #[test]
    fn names_and_functions() {
        assert_eq!(eval("(W - 2*t) / 2"), Ok(382.0));
        assert_eq!(eval("max(W, t; 900)"), Ok(900.0));
        assert_eq!(eval("round(10 / 3)"), Ok(3.0));
        assert_eq!(eval("x"), Err(ExprError::UnknownName("x".into())));
        assert_eq!(
            eval("foo(1)"),
            Err(ExprError::UnknownFunction("foo".into()))
        );
        assert_eq!(eval("abs(1, 2)"), Err(ExprError::Arity("abs".into())));
    }

    #[test]
    fn decimal_comma_outside_calls() {
        assert_eq!(eval("12,5"), Ok(12.5));
        assert_eq!(eval("1,05 * 2"), Ok(2.1));
        assert_eq!(eval("(12,5)"), Ok(12.5));
        assert_eq!(eval("min(12,5)"), Ok(5.0));
        assert_eq!(eval("min(12.5; 20)"), Ok(12.5));
    }

    #[test]
    fn errors() {
        assert_eq!(eval(""), Err(ExprError::Empty));
        assert_eq!(eval("1 +"), Err(ExprError::Syntax(3)));
        assert_eq!(eval("1 / 0"), Err(ExprError::DivisionByZero));
        assert_eq!(eval("2 $ 3"), Err(ExprError::Syntax(2)));
        assert_eq!(eval("(1"), Err(ExprError::Syntax(2)));
    }

    #[test]
    fn renaming_keeps_the_rest() {
        assert_eq!(rename("(W - 2*t)/W", "W", "width"), "(width - 2*t)/width");
        assert_eq!(rename("max(max, 2)", "max", "m"), "max(m, 2)");
        assert_eq!(rename("Wide + W", "W", "x"), "Wide + x");
    }

    #[test]
    fn number_format() {
        assert_eq!(format_number(18.0), "18");
        assert_eq!(format_number(12.5), "12.5");
        assert_eq!(format_number(0.004), "0");
        assert_eq!(format_number(-0.001), "0");
        assert_eq!(format_number(33.333333), "33.33");
    }

    #[test]
    fn valid_names() {
        assert!(is_valid_name("ancho_1"));
        assert!(is_valid_name("año"));
        assert!(!is_valid_name("1a"));
        assert!(!is_valid_name("max"));
        assert!(!is_valid_name("a b"));
    }
}
