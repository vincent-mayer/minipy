//! Runtime values, and Python's arithmetic and comparison rules.
//!
//! The rules here are Python's, not Rust's — they differ in ways that are easy
//! to get silently wrong, so each one has a test:
//!
//! * `/` always produces a float, even `4 / 2`.
//! * `//` and `%` floor toward −∞ and take the *divisor's* sign, so
//!   `-7 // -2 == 3` where Rust's `div_euclid` says `4`.
//! * `bool` is a subtype of `int`: `True + True == 2`.
//! * Integers are 64-bit rather than arbitrary precision, so overflow is an
//!   `OverflowError` instead of a bigger number.

use std::io::Write;
use std::rc::Rc;

use crate::ast::{BinOp, CmpOp, Stmt};
use crate::env::Env;
use crate::error::{MiniPyError, Result};

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Rc<Vec<Stmt>>,
    /// The scope the `def` was evaluated in — this is the closure.
    pub closure: Env,
}

pub struct Builtin {
    pub name: &'static str,
    pub func: fn(Vec<Value>, &mut dyn Write, usize) -> Result<Value>,
}

impl std::fmt::Debug for Builtin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<built-in function {}>", self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Func(Rc<Function>),
    Builtin(&'static Builtin),
    Range { start: i64, stop: i64, step: i64 },
}

impl Value {
    pub fn str(s: impl AsRef<str>) -> Value {
        Value::Str(Rc::from(s.as_ref()))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "NoneType",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::Func(_) => "function",
            Value::Builtin(_) => "builtin_function_or_method",
            Value::Range { .. } => "range",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(x) => *x != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Range { .. } => self.range_len().is_some_and(|n| n > 0),
            Value::Func(_) | Value::Builtin(_) => true,
        }
    }

    /// What `print` writes — `str()` in Python.
    pub fn display(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            other => other.repr(),
        }
    }

    /// What the REPL echoes — `repr()` in Python.
    pub fn repr(&self) -> String {
        match self {
            Value::None => "None".to_string(),
            Value::Bool(true) => "True".to_string(),
            Value::Bool(false) => "False".to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(x) => format_float(*x),
            Value::Str(s) => quote(s),
            Value::Func(f) => format!("<function {}>", f.name),
            Value::Builtin(b) => format!("<built-in function {}>", b.name),
            Value::Range { start, stop, step } => {
                if *step == 1 {
                    format!("range({start}, {stop})")
                } else {
                    format!("range({start}, {stop}, {step})")
                }
            }
        }
    }

    /// Number of items a `range` yields, or `None` for other types.
    fn range_len(&self) -> Option<i64> {
        let Value::Range { start, stop, step } = self else {
            return None;
        };
        let (start, stop, step) = (*start, *stop, *step);
        let span = if step > 0 { stop - start } else { start - stop };
        let step = step.abs();
        Some(if span <= 0 { 0 } else { (span - 1) / step + 1 })
    }

    pub fn len(&self, line: usize) -> Result<i64> {
        match self {
            Value::Str(s) => Ok(s.chars().count() as i64),
            Value::Range { .. } => Ok(self.range_len().unwrap_or(0)),
            other => Err(MiniPyError::type_(
                format!("object of type '{}' has no len()", other.type_name()),
                line,
            )),
        }
    }

    /// The values a `for` loop walks. Lazy, so `range(10**9)` costs nothing.
    pub fn iter(&self, line: usize) -> Result<ValueIter> {
        match self {
            Value::Range { start, stop, step } => Ok(ValueIter::Range {
                next: *start,
                stop: *stop,
                step: *step,
            }),
            Value::Str(s) => Ok(ValueIter::Chars {
                chars: s.chars().collect(),
                index: 0,
            }),
            other => Err(MiniPyError::type_(
                format!("'{}' object is not iterable", other.type_name()),
                line,
            )),
        }
    }
}

pub enum ValueIter {
    Range { next: i64, stop: i64, step: i64 },
    Chars { chars: Vec<char>, index: usize },
}

impl Iterator for ValueIter {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        match self {
            ValueIter::Range { next, stop, step } => {
                let done = if *step > 0 {
                    *next >= *stop
                } else {
                    *next <= *stop
                };
                if done {
                    return None;
                }
                let current = *next;
                *next = next.checked_add(*step)?;
                Some(Value::Int(current))
            }
            ValueIter::Chars { chars, index } => {
                let ch = chars.get(*index)?;
                *index += 1;
                Some(Value::str(ch.to_string()))
            }
        }
    }
}

/// `repr` of a string: prefer single quotes, as Python does.
fn quote(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python's float repr: shortest round-trip, always with a `.0` or an
/// exponent, switching to scientific notation below 1e-4 and at 1e16.
pub fn format_float(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.to_string();
    }

    // `{:e}` normalizes the mantissa to [1, 10), so its exponent is exact.
    let scientific = format!("{x:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("{:e} always emits an 'e'");
    let exponent: i32 = exponent
        .parse()
        .expect("{:e} always emits an integer exponent");

    if exponent < -4 || exponent >= 16 {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    } else {
        let plain = format!("{x}");
        if plain.contains('.') {
            plain
        } else {
            format!("{plain}.0")
        }
    }
}

/// `int` and `bool` as integers; `float` as a float. Everything else is not a
/// number, which is what the arithmetic paths need to know.
#[derive(Clone, Copy)]
enum Num {
    Int(i64),
    Float(f64),
}

fn as_num(v: &Value) -> Option<Num> {
    match v {
        Value::Bool(b) => Some(Num::Int(*b as i64)),
        Value::Int(n) => Some(Num::Int(*n)),
        Value::Float(x) => Some(Num::Float(*x)),
        _ => None,
    }
}

fn to_f64(n: Num) -> f64 {
    match n {
        Num::Int(i) => i as f64,
        Num::Float(f) => f,
    }
}

fn unsupported(op: BinOp, lhs: &Value, rhs: &Value, line: usize) -> MiniPyError {
    MiniPyError::type_(
        format!(
            "unsupported operand type(s) for {}: '{}' and '{}'",
            op.symbol(),
            lhs.type_name(),
            rhs.type_name()
        ),
        line,
    )
}

fn overflowed(line: usize) -> MiniPyError {
    MiniPyError::overflow("integer result is too large for minipy's 64-bit ints", line)
}

pub fn binary(op: BinOp, lhs: &Value, rhs: &Value, line: usize) -> Result<Value> {
    // The two operations that also accept strings.
    match (op, lhs, rhs) {
        (BinOp::Add, Value::Str(a), Value::Str(b)) => {
            return Ok(Value::str(format!("{a}{b}")));
        }
        (BinOp::Mul, Value::Str(s), other) | (BinOp::Mul, other, Value::Str(s)) => {
            let count = match other {
                Value::Int(n) => *n,
                Value::Bool(b) => *b as i64,
                _ => return Err(unsupported(op, lhs, rhs, line)),
            };
            let count = count.max(0) as usize;
            let total = s.len().checked_mul(count).ok_or_else(|| overflowed(line))?;
            if total > 64 * 1024 * 1024 {
                return Err(MiniPyError::overflow("repeated string is too long", line));
            }
            return Ok(Value::str(s.repeat(count)));
        }
        _ => {}
    }

    let (Some(a), Some(b)) = (as_num(lhs), as_num(rhs)) else {
        return Err(unsupported(op, lhs, rhs, line));
    };

    match (op, a, b) {
        // Integer arithmetic stays integral, and overflow is an error rather
        // than a wrap-around, since minipy has no bignums.
        (BinOp::Add, Num::Int(x), Num::Int(y)) => x
            .checked_add(y)
            .map(Value::Int)
            .ok_or_else(|| overflowed(line)),
        (BinOp::Sub, Num::Int(x), Num::Int(y)) => x
            .checked_sub(y)
            .map(Value::Int)
            .ok_or_else(|| overflowed(line)),
        (BinOp::Mul, Num::Int(x), Num::Int(y)) => x
            .checked_mul(y)
            .map(Value::Int)
            .ok_or_else(|| overflowed(line)),
        (BinOp::FloorDiv, Num::Int(x), Num::Int(y)) => {
            if y == 0 {
                return Err(MiniPyError::zero_division("division by zero", line));
            }
            Ok(Value::Int(floor_div(x, y).ok_or_else(|| overflowed(line))?))
        }
        (BinOp::Mod, Num::Int(x), Num::Int(y)) => {
            if y == 0 {
                return Err(MiniPyError::zero_division("division by zero", line));
            }
            Ok(Value::Int(floor_mod(x, y)))
        }
        (BinOp::Pow, Num::Int(x), Num::Int(y)) => {
            if y >= 0 {
                let exp = u32::try_from(y).map_err(|_| overflowed(line))?;
                x.checked_pow(exp)
                    .map(Value::Int)
                    .ok_or_else(|| overflowed(line))
            } else {
                // A negative exponent produces a float, as in Python.
                Ok(Value::Float((x as f64).powf(y as f64)))
            }
        }

        // Anything involving a float, plus `/`, which is always float division.
        (op, a, b) => {
            let (x, y) = (to_f64(a), to_f64(b));
            let result = match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => {
                    if y == 0.0 {
                        return Err(MiniPyError::zero_division("division by zero", line));
                    }
                    x / y
                }
                BinOp::FloorDiv => {
                    if y == 0.0 {
                        return Err(MiniPyError::zero_division("division by zero", line));
                    }
                    (x / y).floor()
                }
                BinOp::Mod => {
                    if y == 0.0 {
                        return Err(MiniPyError::zero_division("division by zero", line));
                    }
                    x - y * (x / y).floor()
                }
                BinOp::Pow => x.powf(y),
            };
            Ok(Value::Float(result))
        }
    }
}

/// Python's `//`: round toward −∞, unlike Rust's truncating `/`.
fn floor_div(x: i64, y: i64) -> Option<i64> {
    let q = x.checked_div(y)?;
    let r = x % y;
    Some(if r != 0 && ((r < 0) != (y < 0)) {
        q - 1
    } else {
        q
    })
}

/// Python's `%`: the result takes the sign of the divisor.
fn floor_mod(x: i64, y: i64) -> i64 {
    let r = x % y;
    if r != 0 && ((r < 0) != (y < 0)) {
        r + y
    } else {
        r
    }
}

pub fn unary_neg(v: &Value, line: usize) -> Result<Value> {
    match as_num(v) {
        Some(Num::Int(n)) => n
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| overflowed(line)),
        Some(Num::Float(x)) => Ok(Value::Float(-x)),
        None => Err(MiniPyError::type_(
            format!("bad operand type for unary -: '{}'", v.type_name()),
            line,
        )),
    }
}

pub fn unary_pos(v: &Value, line: usize) -> Result<Value> {
    match as_num(v) {
        Some(Num::Int(n)) => Ok(Value::Int(n)),
        Some(Num::Float(x)) => Ok(Value::Float(x)),
        None => Err(MiniPyError::type_(
            format!("bad operand type for unary +: '{}'", v.type_name()),
            line,
        )),
    }
}

/// `==` never fails: values of unrelated types are simply unequal.
pub fn equal(lhs: &Value, rhs: &Value) -> bool {
    match (lhs, rhs) {
        (Value::None, Value::None) => true,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Func(a), Value::Func(b)) => Rc::ptr_eq(a, b),
        (Value::Builtin(a), Value::Builtin(b)) => std::ptr::eq(*a, *b),
        (
            Value::Range {
                start: a,
                stop: b,
                step: c,
            },
            Value::Range {
                start: d,
                stop: e,
                step: f,
            },
        ) => (a, b, c) == (d, e, f),
        _ => match (as_num(lhs), as_num(rhs)) {
            (Some(Num::Int(a)), Some(Num::Int(b))) => a == b,
            (Some(a), Some(b)) => to_f64(a) == to_f64(b),
            _ => false,
        },
    }
}

pub fn compare(op: CmpOp, lhs: &Value, rhs: &Value, line: usize) -> Result<bool> {
    if let CmpOp::Eq | CmpOp::NotEq = op {
        let eq = equal(lhs, rhs);
        return Ok(if op == CmpOp::Eq { eq } else { !eq });
    }

    let ordering = match (lhs, rhs) {
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        _ => match (as_num(lhs), as_num(rhs)) {
            (Some(Num::Int(a)), Some(Num::Int(b))) => a.cmp(&b),
            (Some(a), Some(b)) => match to_f64(a).partial_cmp(&to_f64(b)) {
                Some(ordering) => ordering,
                // A NaN is involved: every ordering comparison is false.
                None => return Ok(false),
            },
            _ => {
                return Err(MiniPyError::type_(
                    format!(
                        "'{}' not supported between instances of '{}' and '{}'",
                        op.symbol(),
                        lhs.type_name(),
                        rhs.type_name()
                    ),
                    line,
                ));
            }
        },
    };

    use std::cmp::Ordering::*;
    Ok(match op {
        CmpOp::Lt => ordering == Less,
        CmpOp::LtEq => ordering != Greater,
        CmpOp::Gt => ordering == Greater,
        CmpOp::GtEq => ordering != Less,
        CmpOp::Eq | CmpOp::NotEq => unreachable!("handled above"),
    })
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        // Distinguish 1 from True here, unlike Python's `==`, so that tests
        // can assert on exact values.
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Bool(_), _) | (_, Value::Bool(_)) => false,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Int(_), _) | (_, Value::Int(_)) => false,
            (Value::Float(a), Value::Float(b)) => a == b,
            _ => equal(self, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Int(n)
    }

    fn eval(op: BinOp, a: Value, b: Value) -> Result<Value> {
        binary(op, &a, &b, 1)
    }

    fn ok(op: BinOp, a: Value, b: Value) -> Value {
        eval(op, a, b).expect("expected the operation to succeed")
    }

    #[test]
    fn division_always_produces_a_float() {
        assert_eq!(ok(BinOp::Div, int(4), int(2)), Value::Float(2.0));
        assert_eq!(ok(BinOp::Div, int(1), int(2)), Value::Float(0.5));
    }

    #[test]
    fn floor_division_rounds_toward_negative_infinity() {
        // Rust's `/` truncates and `div_euclid` disagrees on negative
        // divisors; these are CPython's answers.
        assert_eq!(ok(BinOp::FloorDiv, int(-7), int(2)), int(-4));
        assert_eq!(ok(BinOp::FloorDiv, int(-7), int(-2)), int(3));
        assert_eq!(ok(BinOp::FloorDiv, int(7), int(-2)), int(-4));
        assert_eq!(ok(BinOp::FloorDiv, int(7), int(2)), int(3));
    }

    #[test]
    fn modulo_takes_the_sign_of_the_divisor() {
        assert_eq!(ok(BinOp::Mod, int(-7), int(2)), int(1));
        assert_eq!(ok(BinOp::Mod, int(-7), int(-2)), int(-1));
        assert_eq!(ok(BinOp::Mod, int(7), int(-2)), int(-1));
        assert_eq!(ok(BinOp::Mod, int(7), int(2)), int(1));
    }

    #[test]
    fn power_stays_integral_unless_the_exponent_is_negative() {
        assert_eq!(ok(BinOp::Pow, int(2), int(10)), int(1024));
        assert_eq!(ok(BinOp::Pow, int(2), int(-1)), Value::Float(0.5));
        assert_eq!(
            ok(BinOp::Pow, int(2), Value::Float(0.5)),
            Value::Float(std::f64::consts::SQRT_2)
        );
    }

    #[test]
    fn bool_is_a_subtype_of_int() {
        assert_eq!(ok(BinOp::Add, Value::Bool(true), Value::Bool(true)), int(2));
        assert_eq!(ok(BinOp::Mul, Value::Bool(true), int(5)), int(5));
    }

    #[test]
    fn strings_concatenate_and_repeat() {
        assert_eq!(
            ok(BinOp::Add, Value::str("ab"), Value::str("c")),
            Value::str("abc")
        );
        assert_eq!(ok(BinOp::Mul, Value::str("a"), int(3)), Value::str("aaa"));
        assert_eq!(ok(BinOp::Mul, int(3), Value::str("a")), Value::str("aaa"));
        assert_eq!(ok(BinOp::Mul, Value::str("a"), int(-1)), Value::str(""));
    }

    #[test]
    fn division_by_zero_is_reported() {
        for op in [BinOp::Div, BinOp::FloorDiv, BinOp::Mod] {
            let err = eval(op, int(1), int(0)).unwrap_err();
            assert_eq!(err.to_string(), "ZeroDivisionError: division by zero");
        }
        let err = eval(BinOp::Div, Value::Float(1.0), int(0)).unwrap_err();
        assert_eq!(err.to_string(), "ZeroDivisionError: division by zero");
    }

    #[test]
    fn mismatched_operands_name_both_types() {
        let err = eval(BinOp::Add, int(1), Value::str("a")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "TypeError: unsupported operand type(s) for +: 'int' and 'str'"
        );
        let err = eval(BinOp::Add, Value::None, int(1)).unwrap_err();
        assert!(err.to_string().contains("'NoneType' and 'int'"));
    }

    #[test]
    fn integer_overflow_is_an_error_not_a_wraparound() {
        let err = eval(BinOp::Add, int(i64::MAX), int(1)).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::Overflow);
    }

    #[test]
    fn ordering_needs_comparable_types() {
        assert!(compare(CmpOp::Lt, &int(1), &int(2), 1).unwrap());
        assert!(compare(CmpOp::Lt, &Value::str("a"), &Value::str("b"), 1).unwrap());
        assert!(compare(CmpOp::LtEq, &Value::Float(1.0), &int(1), 1).unwrap());

        let err = compare(CmpOp::Lt, &int(1), &Value::str("a"), 1).unwrap_err();
        assert_eq!(
            err.to_string(),
            "TypeError: '<' not supported between instances of 'int' and 'str'"
        );
    }

    #[test]
    fn equality_across_unrelated_types_is_false_not_an_error() {
        assert!(!compare(CmpOp::Eq, &int(1), &Value::str("1"), 1).unwrap());
        assert!(compare(CmpOp::NotEq, &Value::None, &int(0), 1).unwrap());
        assert!(compare(CmpOp::Eq, &Value::Bool(true), &int(1), 1).unwrap());
        assert!(compare(CmpOp::Eq, &Value::Float(1.0), &int(1), 1).unwrap());
    }

    #[test]
    fn truthiness_follows_python() {
        assert!(!Value::None.is_truthy());
        assert!(!int(0).is_truthy());
        assert!(!Value::Float(0.0).is_truthy());
        assert!(!Value::str("").is_truthy());
        assert!(int(-1).is_truthy());
        assert!(Value::str("0").is_truthy());
        assert!(
            !Value::Range {
                start: 0,
                stop: 0,
                step: 1
            }
            .is_truthy()
        );
        assert!(
            Value::Range {
                start: 0,
                stop: 1,
                step: 1
            }
            .is_truthy()
        );
    }

    #[test]
    fn range_length_and_iteration() {
        let r = Value::Range {
            start: 0,
            stop: 10,
            step: 3,
        };
        assert_eq!(r.len(1).unwrap(), 4);
        let items: Vec<_> = r.iter(1).unwrap().collect();
        assert_eq!(items, vec![int(0), int(3), int(6), int(9)]);

        let down = Value::Range {
            start: 3,
            stop: 0,
            step: -1,
        };
        assert_eq!(down.len(1).unwrap(), 3);
        let items: Vec<_> = down.iter(1).unwrap().collect();
        assert_eq!(items, vec![int(3), int(2), int(1)]);
    }

    #[test]
    fn strings_iterate_by_character() {
        let items: Vec<_> = Value::str("héllo").iter(1).unwrap().collect();
        assert_eq!(items.len(), 5);
        assert_eq!(items[1], Value::str("é"));
        assert_eq!(Value::str("héllo").len(1).unwrap(), 5);
    }

    #[test]
    fn repr_and_display_differ_for_strings_only() {
        assert_eq!(Value::str("hi").repr(), "'hi'");
        assert_eq!(Value::str("hi").display(), "hi");
        assert_eq!(Value::str("it's").repr(), "\"it's\"");
        assert_eq!(Value::str("a\nb").repr(), "'a\\nb'");
        assert_eq!(Value::Bool(true).display(), "True");
        assert_eq!(Value::None.display(), "None");
        assert_eq!(
            Value::Range {
                start: 0,
                stop: 3,
                step: 1
            }
            .repr(),
            "range(0, 3)"
        );
        assert_eq!(
            Value::Range {
                start: 0,
                stop: 3,
                step: 2
            }
            .repr(),
            "range(0, 3, 2)"
        );
    }

    #[test]
    fn float_repr_matches_cpython() {
        // Expectations produced by `python3 -c 'print(repr(x))'`.
        let cases = [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1e100, "1e+100"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (-0.0, "-0.0"),
            (300.0, "300.0"),
            (1.0 / 3.0, "0.3333333333333333"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::NAN, "nan"),
        ];
        for (value, expected) in cases {
            assert_eq!(format_float(value), expected, "repr of {value}");
        }
    }
}
