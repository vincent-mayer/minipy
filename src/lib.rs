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

/// Why a REPL buffer is not ready to run yet — or that it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Continuation {
    /// Run it.
    Complete,
    /// An open `(` or a trailing `\`: the statement continues on the next
    /// line, and runs as soon as it is closed.
    Unclosed,
    /// A `:` header: an indented body follows, and the block runs at the
    /// first blank line.
    BlockHeader,
}

/// Classify a REPL buffer, so the prompt knows whether to keep reading.
pub fn continuation(src: &str) -> Continuation {
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

    // A trailing backslash joins the next line, in code and inside strings.
    let continued = src.trim_end_matches(['\n', '\r']).ends_with('\\');

    if depth > 0 || continued {
        Continuation::Unclosed
    } else if last_significant == Some(':') {
        Continuation::BlockHeader
    } else {
        Continuation::Complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_header_waits_for_its_body() {
        assert_eq!(continuation("if x:"), Continuation::BlockHeader);
        assert_eq!(
            continuation("def f(a):  # start"),
            Continuation::BlockHeader
        );
        assert_eq!(continuation("if x:\n    pass\n"), Continuation::Complete);
    }

    #[test]
    fn an_unclosed_paren_or_backslash_continues_the_statement() {
        assert_eq!(continuation("print("), Continuation::Unclosed);
        assert_eq!(continuation("print()"), Continuation::Complete);
        assert_eq!(continuation("x = 1 + \\\n"), Continuation::Unclosed);
        assert_eq!(continuation("x = 1 + \\\n    2\n"), Continuation::Complete);
    }

    #[test]
    fn a_colon_or_paren_inside_a_string_does_not_open_anything() {
        assert_eq!(continuation("x = 'a:'"), Continuation::Complete);
        assert_eq!(continuation("x = \"(\""), Continuation::Complete);
    }

    #[test]
    fn a_plain_statement_is_ready_to_run() {
        assert_eq!(continuation("x = 1"), Continuation::Complete);
    }
}
