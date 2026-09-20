//! minipy — a tree-walking interpreter for a small subset of Python 3.
//!
//! The public surface is deliberately tiny: [`run_source`] executes a whole
//! script, [`Repl`] executes statements one at a time against a persistent
//! global scope, and every output byte goes through a caller-supplied
//! [`Write`] so tests can capture it without spawning a process.

pub mod ast;
pub mod builtins;
pub mod env;
pub mod error;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod value;

pub use error::{ErrorKind, MiniPyError, Result};

use std::io::Write;

use crate::env::Env;
use crate::interp::Interpreter;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stack for the thread the interpreter runs on.
///
/// One minipy call costs several nested Rust frames, so the [recursion
/// limit](interp) of 1000 calls needs far more stack than a thread gets by
/// default — without this the native stack overflows (and aborts) before the
/// interpreter can report `RecursionError`.
pub const STACK_SIZE: usize = 64 * 1024 * 1024;

/// Run `f` on a thread with a stack deep enough for the recursion limit.
pub fn on_interpreter_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let worker = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(f)
        .expect("could not spawn the interpreter thread");

    match worker.join() {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Execute a complete source file, writing program output to `out`.
pub fn run_source(src: &str, out: &mut dyn Write) -> Result<()> {
    let program = parser::parse(src)?;
    Interpreter::new(out).run(&program)
}

/// Run a program and return what it printed — the shape the tests want.
pub fn capture(src: &str) -> Result<String> {
    let mut buffer = Vec::new();
    run_source(src, &mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// An interactive session: statements share one persistent global scope.
pub struct Repl {
    globals: Env,
}

impl Default for Repl {
    fn default() -> Self {
        Repl::new()
    }
}

impl Repl {
    pub fn new() -> Self {
        let globals = Env::global();
        builtins::install(&globals);
        Repl { globals }
    }

    /// Execute one statement buffer. A bare expression echoes its `repr`.
    pub fn feed(&mut self, src: &str, out: &mut dyn Write) -> Result<()> {
        let program = parser::parse(src)?;
        Interpreter::with_globals(self.globals.clone(), out).run_interactive(&program)
    }
}

/// Whether a REPL buffer is still open and needs another line.
///
/// True while a block header awaits its body or a `(` is unclosed.
pub fn is_incomplete(src: &str) -> bool {
    let mut depth = 0i32;
    let mut last_significant = None;

    // Comments run to end of line, so scan line by line rather than in one pass.
    for line in src.lines() {
        let mut in_str: Option<char> = None;
        let mut escaped = false;

        for ch in line.chars() {
            if let Some(quote) = in_str {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote {
                    in_str = None;
                }
                continue;
            }
            match ch {
                '#' => break,
                '\'' | '"' => in_str = Some(ch),
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if !ch.is_whitespace() {
                last_significant = Some(ch);
            }
        }
    }

    depth > 0 || last_significant == Some(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_block_is_incomplete() {
        assert!(is_incomplete("if x:"));
        assert!(is_incomplete("def f(a):  # start"));
        assert!(!is_incomplete("x = 1"));
    }

    #[test]
    fn open_paren_is_incomplete() {
        assert!(is_incomplete("print("));
        assert!(!is_incomplete("print()"));
    }

    #[test]
    fn colon_inside_a_string_does_not_open_a_block() {
        assert!(!is_incomplete("x = 'a:'"));
        assert!(!is_incomplete("x = \"(\""));
    }
}
