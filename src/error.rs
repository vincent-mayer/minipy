//! A single error type for every stage of the interpreter.
//!
//! minipy reports errors the way CPython does, so that a program that fails
//! here fails recognizably there too:
//!
//! ```text
//!   File "examples/fib.py", line 4
//! TypeError: unsupported operand type(s) for +: 'int' and 'str'
//! ```

use std::fmt;

/// The Python exception name an error is reported under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Syntax,
    Indentation,
    Tab,
    Name,
    UnboundLocal,
    Type,
    Value,
    ZeroDivision,
    Overflow,
    Recursion,
    /// Not a program error: the output stream failed.
    Io,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Syntax => "SyntaxError",
            ErrorKind::Indentation => "IndentationError",
            ErrorKind::Tab => "TabError",
            ErrorKind::Name => "NameError",
            ErrorKind::UnboundLocal => "UnboundLocalError",
            ErrorKind::Type => "TypeError",
            ErrorKind::Value => "ValueError",
            ErrorKind::ZeroDivision => "ZeroDivisionError",
            ErrorKind::Overflow => "OverflowError",
            ErrorKind::Recursion => "RecursionError",
            ErrorKind::Io => "OSError",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An error carrying the source line it occurred on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniPyError {
    pub kind: ErrorKind,
    pub msg: String,
    pub line: usize,
    /// Set only for [`ErrorKind::Io`]: which I/O failure it was. A broken
    /// pipe (`minipy script.py | head`) is normal and should exit quietly.
    pub io: Option<std::io::ErrorKind>,
}

impl MiniPyError {
    pub fn new(kind: ErrorKind, msg: impl Into<String>, line: usize) -> Self {
        MiniPyError {
            kind,
            msg: msg.into(),
            line,
            io: None,
        }
    }

    /// The output stream failed. Carries no source line, because no line of
    /// the program is at fault.
    pub fn io(err: &std::io::Error) -> Self {
        MiniPyError {
            kind: ErrorKind::Io,
            msg: format!("could not write output: {err}"),
            line: 0,
            io: Some(err.kind()),
        }
    }

    /// True when output stopped because the reader went away — `| head`.
    pub fn is_broken_pipe(&self) -> bool {
        self.io == Some(std::io::ErrorKind::BrokenPipe)
    }

    /// Render with the `File "…", line N` header CPython prints.
    pub fn report(&self, filename: &str) -> String {
        format!("  File \"{}\", line {}\n{}", filename, self.line, self)
    }
}

macro_rules! ctor {
    ($($name:ident => $kind:ident),* $(,)?) => {
        impl MiniPyError {
            $(
                pub fn $name(msg: impl Into<String>, line: usize) -> Self {
                    MiniPyError::new(ErrorKind::$kind, msg, line)
                }
            )*
        }
    };
}

ctor! {
    syntax => Syntax,
    indentation => Indentation,
    tab => Tab,
    name => Name,
    unbound_local => UnboundLocal,
    type_ => Type,
    value => Value,
    zero_division => ZeroDivision,
    overflow => Overflow,
    recursion => Recursion,
}

impl fmt::Display for MiniPyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.msg)
    }
}

impl std::error::Error for MiniPyError {}

pub type Result<T> = std::result::Result<T, MiniPyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_exception_name_and_message() {
        let e = MiniPyError::name("name 'x' is not defined", 3);
        assert_eq!(e.to_string(), "NameError: name 'x' is not defined");
    }

    #[test]
    fn a_broken_pipe_is_recognised() {
        let err = MiniPyError::io(&std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Broken pipe",
        ));
        assert!(err.is_broken_pipe());
        assert!(!MiniPyError::name("x", 1).is_broken_pipe());
    }

    #[test]
    fn report_matches_cpython_layout() {
        let e = MiniPyError::type_("bad operand", 4);
        assert_eq!(
            e.report("examples/fib.py"),
            "  File \"examples/fib.py\", line 4\nTypeError: bad operand"
        );
    }
}
