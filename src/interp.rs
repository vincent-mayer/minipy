//! The evaluator: walks the tree and runs it.
//!
//! Two decisions shape this module:
//!
//! * **Output goes through a writer**, not `println!`, so a test can run a
//!   program and read back exactly what it printed.
//! * **Control flow is a value**, [`Flow`], rather than an error variant.
//!   `break`, `continue` and `return` are ordinary results that propagate up
//!   until a loop or a call absorbs them.

use std::io::Write;
use std::rc::Rc;

use crate::ast::{Expr, Stmt, UnOp};
use crate::builtins;
use crate::env::Env;
use crate::error::{MiniPyError, Result};
use crate::value::{self, Function, Value};

/// How far a Rust stack of ~8 MB gets before a deep minipy recursion would
/// overflow it. CPython's own default limit is the same number.
const MAX_DEPTH: usize = 1000;

/// What executing a statement decided to do next.
#[derive(Debug)]
#[must_use = "a Break, Continue or Return must be propagated to its enclosing construct"]
pub enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

pub struct Interpreter<'out> {
    pub globals: Env,
    out: &'out mut dyn Write,
    depth: usize,
}

impl<'out> Interpreter<'out> {
    pub fn new(out: &'out mut dyn Write) -> Self {
        let globals = Env::global();
        builtins::install(&globals);
        Interpreter::with_globals(globals, out)
    }

    /// Reuse an existing global scope — how the REPL keeps state between
    /// entries.
    pub fn with_globals(globals: Env, out: &'out mut dyn Write) -> Self {
        Interpreter {
            globals,
            out,
            depth: 0,
        }
    }

    /// Run a program in the global scope.
    pub fn run(&mut self, program: &[Stmt]) -> Result<()> {
        let env = self.globals.clone();
        match self.exec_block(program, &env)? {
            Flow::Normal => Ok(()),
            // The parser rejects `break`/`continue`/`return` outside their
            // enclosing construct, so nothing else can reach here.
            other => unreachable!("control flow escaped to the top level: {other:?}"),
        }
    }

    pub fn exec_block(&mut self, body: &[Stmt], env: &Env) -> Result<Flow> {
        for stmt in body {
            match self.exec(stmt, env)? {
                Flow::Normal => {}
                flow => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, stmt: &Stmt, env: &Env) -> Result<Flow> {
        match stmt {
            Stmt::Pass => Ok(Flow::Normal),

            Stmt::Expr { value, .. } => {
                self.eval(value, env)?;
                Ok(Flow::Normal)
            }

            Stmt::Assign { target, value, .. } => {
                let value = self.eval(value, env)?;
                env.set(target, value);
                Ok(Flow::Normal)
            }

            Stmt::AugAssign {
                target,
                op,
                value,
                line,
            } => {
                let current = env.get(target).ok_or_else(|| undefined(target, *line))?;
                let rhs = self.eval(value, env)?;
                env.set(target, value::binary(*op, &current, &rhs, *line)?);
                Ok(Flow::Normal)
            }

            Stmt::If {
                branches,
                otherwise,
                ..
            } => {
                for (test, body) in branches {
                    if self.eval(test, env)?.is_truthy() {
                        return self.exec_block(body, env);
                    }
                }
                self.exec_block(otherwise, env)
            }

            Stmt::While { test, body, .. } => {
                while self.eval(test, env)?.is_truthy() {
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Normal | Flow::Continue => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }

            Stmt::For {
                var,
                iter,
                body,
                line,
            } => {
                let iterable = self.eval(iter, env)?;
                for item in iterable.iter(*line)? {
                    env.set(var, item);
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Normal | Flow::Continue => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }

            Stmt::Def {
                name, params, body, ..
            } => {
                let function = Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: Rc::clone(body),
                    // Capturing the defining scope is what makes closures work.
                    closure: env.clone(),
                };
                env.set(name, Value::Func(Rc::new(function)));
                Ok(Flow::Normal)
            }

            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expr) => self.eval(expr, env)?,
                    None => Value::None,
                };
                Ok(Flow::Return(value))
            }

            Stmt::Break { .. } => Ok(Flow::Break),
            Stmt::Continue { .. } => Ok(Flow::Continue),
        }
    }

    pub fn eval(&mut self, expr: &Expr, env: &Env) -> Result<Value> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(x) => Ok(Value::Float(*x)),
            Expr::Str(s) => Ok(Value::Str(Rc::clone(s))),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::None => Ok(Value::None),

            Expr::Name { name, line } => env.get(name).ok_or_else(|| undefined(name, *line)),

            Expr::Unary { op, operand, line } => {
                let value = self.eval(operand, env)?;
                match op {
                    UnOp::Neg => value::unary_neg(&value, *line),
                    UnOp::Pos => value::unary_pos(&value, *line),
                    UnOp::Not => Ok(Value::Bool(!value.is_truthy())),
                }
            }

            Expr::Binary { op, lhs, rhs, line } => {
                let lhs = self.eval(lhs, env)?;
                let rhs = self.eval(rhs, env)?;
                value::binary(*op, &lhs, &rhs, *line)
            }

            // `a < b < c` evaluates `b` once and stops at the first false link.
            Expr::Compare { first, rest, line } => {
                let mut lhs = self.eval(first, env)?;
                for (op, operand) in rest {
                    let rhs = self.eval(operand, env)?;
                    if !value::compare(*op, &lhs, &rhs, *line)? {
                        return Ok(Value::Bool(false));
                    }
                    lhs = rhs;
                }
                Ok(Value::Bool(true))
            }

            // `and` and `or` yield an operand, not a bool: `1 or 2` is 1.
            Expr::And { lhs, rhs } => {
                let lhs = self.eval(lhs, env)?;
                if lhs.is_truthy() {
                    self.eval(rhs, env)
                } else {
                    Ok(lhs)
                }
            }
            Expr::Or { lhs, rhs } => {
                let lhs = self.eval(lhs, env)?;
                if lhs.is_truthy() {
                    Ok(lhs)
                } else {
                    self.eval(rhs, env)
                }
            }

            Expr::Call { callee, args, line } => {
                let callee = self.eval(callee, env)?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, env)?);
                }
                self.call(&callee, values, *line)
            }
        }
    }

    pub fn call(&mut self, callee: &Value, args: Vec<Value>, line: usize) -> Result<Value> {
        match callee {
            Value::Builtin(builtin) => (builtin.func)(args, self.out, line),

            Value::Func(function) => {
                if args.len() != function.params.len() {
                    return Err(MiniPyError::type_(
                        format!(
                            "{}() takes {} positional argument{} but {} {} given",
                            function.name,
                            function.params.len(),
                            if function.params.len() == 1 { "" } else { "s" },
                            args.len(),
                            if args.len() == 1 { "was" } else { "were" },
                        ),
                        line,
                    ));
                }

                // Without this the Rust stack, not the program, decides how
                // deep a recursion may go — and it fails by crashing.
                if self.depth >= MAX_DEPTH {
                    return Err(MiniPyError::recursion(
                        "maximum recursion depth exceeded",
                        line,
                    ));
                }

                let frame = function.closure.child();
                for (param, arg) in function.params.iter().zip(args) {
                    frame.set(param, arg);
                }

                self.depth += 1;
                let flow = self.exec_block(&function.body, &frame);
                self.depth -= 1;

                Ok(match flow? {
                    Flow::Return(value) => value,
                    // Falling off the end of a function returns None.
                    _ => Value::None,
                })
            }

            other => Err(MiniPyError::type_(
                format!("'{}' object is not callable", other.type_name()),
                line,
            )),
        }
    }

    /// Run one REPL entry, echoing the value of a bare expression.
    pub fn run_interactive(&mut self, program: &[Stmt]) -> Result<()> {
        let env = self.globals.clone();
        for stmt in program {
            if let Stmt::Expr { value, .. } = stmt {
                let value = self.eval(value, &env)?;
                if !matches!(value, Value::None) {
                    writeln!(self.out, "{}", value.repr()).map_err(|e| write_failed(&e))?;
                }
                continue;
            }
            // As in `run`: the parser has already ruled out control flow
            // reaching the top level.
            match self.exec(stmt, &env)? {
                Flow::Normal => {}
                other => unreachable!("control flow escaped to the top level: {other:?}"),
            }
        }
        Ok(())
    }
}

fn undefined(name: &str, line: usize) -> MiniPyError {
    MiniPyError::name(format!("name '{name}' is not defined"), line)
}

pub fn write_failed(err: &std::io::Error) -> MiniPyError {
    MiniPyError::value(format!("could not write output: {err}"), 0)
}
