//! Source text → tokens.
//!
//! Two things make a Python lexer more than a `match` over characters:
//!
//! * **Indentation is syntax.** A column stack turns leading whitespace into
//!   explicit `Indent`/`Dedent` tokens, so the parser sees ordinary block
//!   delimiters.
//! * **Newlines are sometimes not statement ends.** Inside `(` … `)` a newline
//!   is just whitespace (implicit line joining), and blank or comment-only
//!   lines never produce a `Newline` at all.

use crate::error::{MiniPyError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // literals and names
    Int(i64),
    Float(f64),
    Str(String),
    Name(String),

    // keywords
    And,
    Break,
    Continue,
    Def,
    Elif,
    Else,
    False,
    For,
    If,
    In,
    None_,
    Not,
    Or,
    Pass,
    Return,
    True,
    While,

    // operators and punctuation
    Plus,
    Minus,
    Star,
    Slash,
    DoubleSlash,
    Percent,
    DoubleStar,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    LParen,
    RParen,
    Comma,
    Colon,
    Semicolon,

    // structure
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl Tok {
    /// How the token is spelled in a syntax error message.
    pub fn describe(&self) -> String {
        match self {
            Tok::Int(n) => n.to_string(),
            Tok::Float(x) => x.to_string(),
            Tok::Str(_) => "string literal".to_string(),
            Tok::Name(n) => n.clone(),
            Tok::Newline => "end of line".to_string(),
            Tok::Indent => "an indented block".to_string(),
            Tok::Dedent => "end of block".to_string(),
            Tok::Eof => "end of input".to_string(),
            other => format!("'{}'", other.spelling()),
        }
    }

    fn spelling(&self) -> &'static str {
        match self {
            Tok::And => "and",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::Def => "def",
            Tok::Elif => "elif",
            Tok::Else => "else",
            Tok::False => "False",
            Tok::For => "for",
            Tok::If => "if",
            Tok::In => "in",
            Tok::None_ => "None",
            Tok::Not => "not",
            Tok::Or => "or",
            Tok::Pass => "pass",
            Tok::Return => "return",
            Tok::True => "True",
            Tok::While => "while",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::DoubleSlash => "//",
            Tok::Percent => "%",
            Tok::DoubleStar => "**",
            Tok::Eq => "==",
            Tok::NotEq => "!=",
            Tok::Lt => "<",
            Tok::LtEq => "<=",
            Tok::Gt => ">",
            Tok::GtEq => ">=",
            Tok::Assign => "=",
            Tok::PlusAssign => "+=",
            Tok::MinusAssign => "-=",
            Tok::StarAssign => "*=",
            Tok::SlashAssign => "/=",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::Comma => ",",
            Tok::Colon => ":",
            Tok::Semicolon => ";",
            _ => "token",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

fn keyword(word: &str) -> Option<Tok> {
    Some(match word {
        "and" => Tok::And,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "def" => Tok::Def,
        "elif" => Tok::Elif,
        "else" => Tok::Else,
        "False" => Tok::False,
        "for" => Tok::For,
        "if" => Tok::If,
        "in" => Tok::In,
        "None" => Tok::None_,
        "not" => Tok::Not,
        "or" => Tok::Or,
        "pass" => Tok::Pass,
        "return" => Tok::Return,
        "True" => Tok::True,
        "while" => Tok::While,
        _ => return None,
    })
}

/// Reserved in Python but not part of the minipy subset — worth a better
/// message than "invalid character".
const UNSUPPORTED: &[&str] = &[
    "class", "import", "from", "try", "except", "finally", "raise", "with", "lambda", "global",
    "nonlocal", "yield", "assert", "del", "async", "await", "is",
];

pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    Lexer::new(src).run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    /// Open `(` count: inside brackets a newline is just whitespace.
    depth: usize,
    indents: Vec<usize>,
    out: Vec<Token>,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            depth: 0,
            indents: vec![0],
            out: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn push(&mut self, tok: Tok) {
        let line = self.line;
        self.out.push(Token { tok, line });
    }

    fn last_is_newline(&self) -> bool {
        matches!(self.out.last().map(|t| &t.tok), None | Some(Tok::Newline))
    }

    fn run(mut self) -> Result<Vec<Token>> {
        while self.pos < self.chars.len() {
            // Start of a logical line: measure indentation, unless a bracket
            // is open (then a newline is just whitespace).
            if self.depth == 0 && self.last_is_newline() {
                if self.handle_line_start()? {
                    continue;
                }
            }
            self.scan_token()?;
        }

        if !self.last_is_newline() {
            self.push(Tok::Newline);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(Tok::Dedent);
        }
        self.push(Tok::Eof);
        Ok(self.out)
    }

    /// Consume leading whitespace and emit Indent/Dedent.
    ///
    /// Returns true when the line turned out to be blank or a comment and the
    /// caller should start over on the next line.
    fn handle_line_start(&mut self) -> Result<bool> {
        let mut column = 0usize;
        loop {
            match self.peek() {
                Some(' ') => {
                    column += 1;
                    self.pos += 1;
                }
                Some('\t') => {
                    return Err(MiniPyError::tab(
                        "inconsistent use of tabs and spaces in indentation \
                         (minipy indents with spaces only)",
                        self.line,
                    ));
                }
                _ => break,
            }
        }

        // Blank line, or nothing but a comment: no tokens, no indent change.
        match self.peek() {
            None => return Ok(true),
            Some('\n') => {
                self.pos += 1;
                self.line += 1;
                return Ok(true);
            }
            Some('#') => {
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    self.pos += 1;
                }
                return Ok(true);
            }
            _ => {}
        }

        let current = *self.indents.last().expect("indent stack is never empty");
        if column > current {
            self.indents.push(column);
            self.push(Tok::Indent);
        } else if column < current {
            while *self.indents.last().unwrap() > column {
                self.indents.pop();
                self.push(Tok::Dedent);
            }
            if *self.indents.last().unwrap() != column {
                return Err(MiniPyError::indentation(
                    "unindent does not match any outer indentation level",
                    self.line,
                ));
            }
        }
        Ok(false)
    }

    fn scan_token(&mut self) -> Result<()> {
        let Some(ch) = self.peek() else { return Ok(()) };

        match ch {
            ' ' | '\r' => {
                self.pos += 1;
            }
            '\t' => {
                // Legal as separating whitespace, just not for indentation.
                self.pos += 1;
            }
            '#' => {
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.pos += 1;
                }
            }
            '\\' if self.peek_at(1) == Some('\n') => {
                // Explicit line joining.
                self.pos += 2;
                self.line += 1;
            }
            '\n' => {
                self.pos += 1;
                if self.depth == 0 && !self.last_is_newline() {
                    self.push(Tok::Newline);
                }
                self.line += 1;
            }
            '0'..='9' => self.number()?,
            '.' if matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) => self.number()?,
            '\'' | '"' => self.string()?,
            c if c.is_alphabetic() || c == '_' => self.name()?,
            _ => self.operator()?,
        }
        Ok(())
    }

    fn number(&mut self) -> Result<()> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '_') {
            self.pos += 1;
        }

        // minipy has no attribute access, so a '.' here is always a decimal
        // point: `4.` is 4.0 and `.5` is 0.5, as in Python.
        let mut is_float = false;
        if self.peek() == Some('.') {
            is_float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '_') {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let mut ahead = 1;
            if matches!(self.peek_at(1), Some('+' | '-')) {
                ahead = 2;
            }
            if matches!(self.peek_at(ahead), Some(c) if c.is_ascii_digit()) {
                is_float = true;
                self.pos += ahead;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }

        let text: String = self.chars[start..self.pos]
            .iter()
            .filter(|c| **c != '_')
            .collect();

        if matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            return Err(MiniPyError::syntax("invalid decimal literal", self.line));
        }

        if is_float {
            let value: f64 = text
                .parse()
                .map_err(|_| MiniPyError::syntax("invalid float literal", self.line))?;
            self.push(Tok::Float(value));
        } else {
            match text.parse::<i64>() {
                Ok(value) => self.push(Tok::Int(value)),
                Err(_) => {
                    return Err(MiniPyError::overflow(
                        "integer literal is too large for minipy's 64-bit ints",
                        self.line,
                    ));
                }
            }
        }
        Ok(())
    }

    fn string(&mut self) -> Result<()> {
        let quote = self.bump().expect("called with a quote available");
        let opened_on = self.line;
        let mut value = String::new();

        loop {
            let Some(ch) = self.bump() else {
                return Err(MiniPyError::syntax(
                    "unterminated string literal",
                    opened_on,
                ));
            };
            match ch {
                c if c == quote => break,
                '\n' => {
                    return Err(MiniPyError::syntax(
                        "unterminated string literal",
                        opened_on,
                    ));
                }
                '\\' => {
                    let Some(esc) = self.bump() else {
                        return Err(MiniPyError::syntax(
                            "unterminated string literal",
                            opened_on,
                        ));
                    };
                    match esc {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        'r' => value.push('\r'),
                        '0' => value.push('\0'),
                        '\\' => value.push('\\'),
                        '\'' => value.push('\''),
                        '"' => value.push('"'),
                        '\n' => self.line += 1, // line continuation inside a string
                        other => {
                            // Python keeps unknown escapes verbatim.
                            value.push('\\');
                            value.push(other);
                        }
                    }
                }
                c => value.push(c),
            }
        }

        self.push(Tok::Str(value));
        Ok(())
    }

    fn name(&mut self) -> Result<()> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();

        if let Some(tok) = keyword(&word) {
            self.push(tok);
        } else if UNSUPPORTED.contains(&word.as_str()) {
            return Err(MiniPyError::syntax(
                format!("'{word}' is not supported by minipy"),
                self.line,
            ));
        } else {
            self.push(Tok::Name(word));
        }
        Ok(())
    }

    fn operator(&mut self) -> Result<()> {
        let ch = self.bump().expect("called with a character available");
        let tok = match ch {
            '+' => {
                if self.eat('=') {
                    Tok::PlusAssign
                } else {
                    Tok::Plus
                }
            }
            '-' => {
                if self.eat('=') {
                    Tok::MinusAssign
                } else {
                    Tok::Minus
                }
            }
            '*' => {
                if self.eat('*') {
                    Tok::DoubleStar
                } else if self.eat('=') {
                    Tok::StarAssign
                } else {
                    Tok::Star
                }
            }
            '/' => {
                if self.eat('/') {
                    Tok::DoubleSlash
                } else if self.eat('=') {
                    Tok::SlashAssign
                } else {
                    Tok::Slash
                }
            }
            '%' => Tok::Percent,
            '=' => {
                if self.eat('=') {
                    Tok::Eq
                } else {
                    Tok::Assign
                }
            }
            '!' => {
                if self.eat('=') {
                    Tok::NotEq
                } else {
                    return Err(MiniPyError::syntax("invalid syntax: '!'", self.line));
                }
            }
            '<' => {
                if self.eat('=') {
                    Tok::LtEq
                } else {
                    Tok::Lt
                }
            }
            '>' => {
                if self.eat('=') {
                    Tok::GtEq
                } else {
                    Tok::Gt
                }
            }
            '(' => {
                self.depth += 1;
                Tok::LParen
            }
            ')' => {
                self.depth = self.depth.saturating_sub(1);
                Tok::RParen
            }
            ',' => Tok::Comma,
            ':' => Tok::Colon,
            ';' => Tok::Semicolon,
            '[' | ']' | '{' | '}' => {
                return Err(MiniPyError::syntax(
                    format!("'{ch}' is not supported by minipy (no lists, dicts or sets)"),
                    self.line,
                ));
            }
            other => {
                return Err(MiniPyError::syntax(
                    format!("invalid character '{other}'"),
                    self.line,
                ));
            }
        };
        self.push(tok);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        tokenize(src)
            .expect("expected the source to lex")
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    fn err(src: &str) -> String {
        tokenize(src).expect_err("expected a lex error").to_string()
    }

    #[test]
    fn simple_assignment() {
        assert_eq!(
            toks("x = 1\n"),
            vec![
                Tok::Name("x".into()),
                Tok::Assign,
                Tok::Int(1),
                Tok::Newline,
                Tok::Eof
            ]
        );
    }

    #[test]
    fn nested_blocks_emit_matching_indent_and_dedent() {
        let src = "if a:\n    if b:\n        c\nd\n";
        assert_eq!(
            toks(src),
            vec![
                Tok::If,
                Tok::Name("a".into()),
                Tok::Colon,
                Tok::Newline,
                Tok::Indent,
                Tok::If,
                Tok::Name("b".into()),
                Tok::Colon,
                Tok::Newline,
                Tok::Indent,
                Tok::Name("c".into()),
                Tok::Newline,
                Tok::Dedent,
                Tok::Dedent,
                Tok::Name("d".into()),
                Tok::Newline,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn trailing_dedents_are_emitted_at_eof() {
        // Also covers a file that does not end in a newline.
        assert_eq!(
            toks("if a:\n    b"),
            vec![
                Tok::If,
                Tok::Name("a".into()),
                Tok::Colon,
                Tok::Newline,
                Tok::Indent,
                Tok::Name("b".into()),
                Tok::Newline,
                Tok::Dedent,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn blank_and_comment_lines_do_not_affect_indentation() {
        let src = "if a:\n\n    # note\n    b\n\n# trailing\n";
        assert_eq!(
            toks(src),
            vec![
                Tok::If,
                Tok::Name("a".into()),
                Tok::Colon,
                Tok::Newline,
                Tok::Indent,
                Tok::Name("b".into()),
                Tok::Newline,
                Tok::Dedent,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn newlines_inside_parens_are_whitespace() {
        assert_eq!(
            toks("f(1,\n  2)\n"),
            vec![
                Tok::Name("f".into()),
                Tok::LParen,
                Tok::Int(1),
                Tok::Comma,
                Tok::Int(2),
                Tok::RParen,
                Tok::Newline,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn line_numbers_track_across_lines() {
        let tokens = tokenize("x = 1\n\ny = 2\n").unwrap();
        let y = tokens
            .iter()
            .find(|t| t.tok == Tok::Name("y".into()))
            .unwrap();
        assert_eq!(y.line, 3);
    }

    #[test]
    fn operators_prefer_the_longest_match() {
        assert_eq!(
            toks("a // b ** c != d <= e += f\n"),
            vec![
                Tok::Name("a".into()),
                Tok::DoubleSlash,
                Tok::Name("b".into()),
                Tok::DoubleStar,
                Tok::Name("c".into()),
                Tok::NotEq,
                Tok::Name("d".into()),
                Tok::LtEq,
                Tok::Name("e".into()),
                Tok::PlusAssign,
                Tok::Name("f".into()),
                Tok::Newline,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            toks("1 2.5 1_000 3e2 4. .5\n")[..6],
            [
                Tok::Int(1),
                Tok::Float(2.5),
                Tok::Int(1000),
                Tok::Float(300.0),
                Tok::Float(4.0),
                Tok::Float(0.5)
            ]
        );
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            toks(r#""a\nb" 'it\'s' "\q""#)[..3],
            [
                Tok::Str("a\nb".into()),
                Tok::Str("it's".into()),
                Tok::Str("\\q".into())
            ]
        );
    }

    #[test]
    fn keywords_are_not_names() {
        assert_eq!(toks("not None\n")[..2], [Tok::Not, Tok::None_]);
        assert_eq!(toks("nothing\n")[0], Tok::Name("nothing".into()));
    }

    #[test]
    fn bad_dedent_is_an_indentation_error() {
        let src = "if a:\n        b\n    c\n";
        assert_eq!(
            err(src),
            "IndentationError: unindent does not match any outer indentation level"
        );
    }

    #[test]
    fn leading_tab_is_a_tab_error() {
        assert!(err("if a:\n\tb\n").starts_with("TabError:"));
    }

    #[test]
    fn unterminated_string_reports_the_opening_line() {
        let e = tokenize("x = 1\ny = 'oops\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.to_string().contains("unterminated string literal"));
    }

    #[test]
    fn unsupported_python_gets_a_specific_message() {
        assert_eq!(
            err("class Foo:\n    pass\n"),
            "SyntaxError: 'class' is not supported by minipy"
        );
        assert!(err("x = [1]\n").contains("no lists, dicts or sets"));
    }

    #[test]
    fn oversized_int_literal_overflows() {
        assert!(err("x = 99999999999999999999\n").starts_with("OverflowError:"));
    }
}
