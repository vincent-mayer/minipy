//! The builtin functions, and the scope they live in.
//!
//! Each takes its arguments by value plus the output writer (only `print`
//! uses it) and the call site's line, so failures point at the caller.

use std::io::Write;

use crate::env::Env;
use crate::error::{MiniPyError, Result};
use crate::interp::write_failed;
use crate::value::{Builtin, Value};

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "print",
        func: print,
    },
    Builtin {
        name: "len",
        func: len,
    },
    Builtin {
        name: "range",
        func: range,
    },
    Builtin {
        name: "str",
        func: to_str,
    },
    Builtin {
        name: "int",
        func: to_int,
    },
    Builtin {
        name: "float",
        func: to_float,
    },
    Builtin {
        name: "bool",
        func: to_bool,
    },
    Builtin {
        name: "abs",
        func: abs,
    },
    Builtin {
        name: "min",
        func: min,
    },
    Builtin {
        name: "max",
        func: max,
    },
];

/// Bind every builtin in `env`. They are ordinary bindings, so a program may
/// shadow one — `print = 3` is legal Python, and legal here.
pub fn install(env: &Env) {
    for builtin in BUILTINS {
        env.set(builtin.name, Value::Builtin(builtin));
    }
}

fn arity(name: &str, args: &[Value], min: usize, max: usize, line: usize) -> Result<()> {
    if args.len() < min || args.len() > max {
        let expected = if min == max {
            format!("exactly {min} argument{}", plural(min))
        } else {
            format!("{min} to {max} arguments")
        };
        return Err(MiniPyError::type_(
            format!("{name}() takes {expected} ({} given)", args.len()),
            line,
        ));
    }
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn print(args: Vec<Value>, out: &mut dyn Write, _line: usize) -> Result<Value> {
    let rendered: Vec<String> = args.iter().map(|v| v.display()).collect();
    writeln!(out, "{}", rendered.join(" ")).map_err(|e| write_failed(&e))?;
    Ok(Value::None)
}

fn len(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("len", &args, 1, 1, line)?;
    Ok(Value::Int(args[0].len(line)?))
}

fn range(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("range", &args, 1, 3, line)?;

    let mut bounds = Vec::with_capacity(args.len());
    for arg in &args {
        bounds.push(match arg {
            Value::Int(n) => *n,
            Value::Bool(b) => *b as i64,
            other => {
                return Err(MiniPyError::type_(
                    format!(
                        "'{}' object cannot be interpreted as an integer",
                        other.type_name()
                    ),
                    line,
                ));
            }
        });
    }

    let (start, stop, step) = match bounds[..] {
        [stop] => (0, stop, 1),
        [start, stop] => (start, stop, 1),
        [start, stop, step] => (start, stop, step),
        _ => unreachable!("arity was checked"),
    };
    if step == 0 {
        return Err(MiniPyError::value("range() arg 3 must not be zero", line));
    }
    Ok(Value::Range { start, stop, step })
}

fn to_str(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("str", &args, 0, 1, line)?;
    Ok(match args.first() {
        Some(value) => Value::str(value.display()),
        None => Value::str(""),
    })
}

fn to_int(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("int", &args, 0, 1, line)?;
    let Some(value) = args.first() else {
        return Ok(Value::Int(0));
    };
    match value {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Bool(b) => Ok(Value::Int(*b as i64)),
        Value::Float(x) => {
            if !x.is_finite() {
                return Err(MiniPyError::value(
                    format!(
                        "cannot convert {} to an integer",
                        if x.is_nan() {
                            "float NaN"
                        } else {
                            "float infinity"
                        }
                    ),
                    line,
                ));
            }
            // Python truncates toward zero.
            Ok(Value::Int(x.trunc() as i64))
        }
        Value::Str(s) => s.trim().parse::<i64>().map(Value::Int).map_err(|_| {
            MiniPyError::value(
                format!("invalid literal for int() with base 10: '{s}'"),
                line,
            )
        }),
        other => Err(MiniPyError::type_(
            format!(
                "int() argument must be a string or a number, not '{}'",
                other.type_name()
            ),
            line,
        )),
    }
}

fn to_float(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("float", &args, 0, 1, line)?;
    let Some(value) = args.first() else {
        return Ok(Value::Float(0.0));
    };
    match value {
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Bool(b) => Ok(Value::Float(*b as i64 as f64)),
        Value::Float(x) => Ok(Value::Float(*x)),
        Value::Str(s) => s.trim().parse::<f64>().map(Value::Float).map_err(|_| {
            MiniPyError::value(format!("could not convert string to float: '{s}'"), line)
        }),
        other => Err(MiniPyError::type_(
            format!(
                "float() argument must be a string or a number, not '{}'",
                other.type_name()
            ),
            line,
        )),
    }
}

fn to_bool(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("bool", &args, 0, 1, line)?;
    Ok(Value::Bool(args.first().is_some_and(|v| v.is_truthy())))
}

fn abs(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    arity("abs", &args, 1, 1, line)?;
    match &args[0] {
        Value::Int(n) => n.checked_abs().map(Value::Int).ok_or_else(|| {
            MiniPyError::overflow("integer result is too large for minipy's 64-bit ints", line)
        }),
        Value::Bool(b) => Ok(Value::Int(*b as i64)),
        Value::Float(x) => Ok(Value::Float(x.abs())),
        other => Err(MiniPyError::type_(
            format!("bad operand type for abs(): '{}'", other.type_name()),
            line,
        )),
    }
}

fn min(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    extremum("min", args, line, crate::ast::CmpOp::Lt)
}

fn max(args: Vec<Value>, _out: &mut dyn Write, line: usize) -> Result<Value> {
    extremum("max", args, line, crate::ast::CmpOp::Gt)
}

/// `min`/`max` over their arguments, or over a single iterable argument.
fn extremum(name: &str, args: Vec<Value>, line: usize, keep: crate::ast::CmpOp) -> Result<Value> {
    let candidates: Vec<Value> = match args.len() {
        0 => {
            return Err(MiniPyError::type_(
                format!("{name}() expected at least 1 argument, got 0"),
                line,
            ));
        }
        1 => args[0].iter(line)?.collect(),
        _ => args,
    };

    let mut best: Option<Value> = None;
    for candidate in candidates {
        best = Some(match best {
            None => candidate,
            Some(current) => {
                if crate::value::compare(keep, &candidate, &current, line)? {
                    candidate
                } else {
                    current
                }
            }
        });
    }
    best.ok_or_else(|| MiniPyError::value(format!("{name}() arg is an empty sequence"), line))
}
