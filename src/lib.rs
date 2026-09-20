//! minipy — a tree-walking interpreter for a small subset of Python 3.
//!
//! The public surface is deliberately tiny: [`run_source`] executes a whole
//! script, [`Repl`] executes statements one at a time against a persistent
//! global scope, and every output byte goes through a caller-supplied
//! [`Write`] so tests can capture it without spawning a process.

pub mod error;

pub use error::{ErrorKind, MiniPyError, Result};

use std::io::Write;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Execute a complete source file, writing program output to `out`.
pub fn run_source(_src: &str, _out: &mut dyn Write) -> Result<()> {
    Err(MiniPyError::syntax(
        "the front end lands in the next commit",
        1,
    ))
}

/// An interactive session: statements share one persistent global scope.
#[derive(Default)]
pub struct Repl {
    _private: (),
}

impl Repl {
    pub fn new() -> Self {
        Repl::default()
    }

    /// Execute one statement buffer. A bare expression echoes its `repr`.
    pub fn feed(&mut self, src: &str, out: &mut dyn Write) -> Result<()> {
        run_source(src, out)
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
