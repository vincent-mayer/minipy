//! Tokens → syntax tree.
//!
//! Statements are parsed by straightforward recursive descent; expressions by
//! precedence climbing, one function per precedence level, lowest first.
//!
//! The parser also enforces the placement rules that are grammatical rather
//! than semantic — `break` outside a loop, `return` outside a function — so
//! the interpreter never has to handle control flow escaping the top level.

use std::rc::Rc;

use crate::ast::{BinOp, CmpOp, Expr, Stmt, UnOp, bound_names};
use crate::error::{MiniPyError, Result};
use crate::lexer::{Tok, Token, tokenize};

pub fn parse(src: &str) -> Result<Vec<Stmt>> {
    Parser::new(tokenize(src)?).program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Depth of enclosing loops, reset inside a `def`: `break` may not cross a
    /// function boundary.
    loops: usize,
    functions: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            loops: 0,
            functions: 0,
        }
    }

    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].tok
    }

    fn line(&self) -> usize {
        self.tokens[self.pos.min(self.tokens.len() - 1)].line
    }

    fn at(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn advance(&mut self) -> Tok {
        let tok = self.tokens[self.pos.min(self.tokens.len() - 1)].tok.clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.at(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok) -> Result<()> {
        if self.eat(tok) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("expected {}", tok.describe())))
        }
    }

    fn unexpected(&self, what: &str) -> MiniPyError {
        MiniPyError::syntax(
            format!("invalid syntax: {what}, found {}", self.peek().describe()),
            self.line(),
        )
    }

    fn program(&mut self) -> Result<Vec<Stmt>> {
        let mut body = Vec::new();
        while !self.at(&Tok::Eof) {
            // A stray Newline at top level is harmless.
            if self.eat(&Tok::Newline) {
                continue;
            }
            body.extend(self.statement()?);
        }
        Ok(body)
    }

    /// One statement — or several, when simple statements are `;`-separated.
    fn statement(&mut self) -> Result<Vec<Stmt>> {
        match self.peek() {
            Tok::If => Ok(vec![self.if_stmt()?]),
            Tok::While => Ok(vec![self.while_stmt()?]),
            Tok::For => Ok(vec![self.for_stmt()?]),
            Tok::Def => Ok(vec![self.def_stmt()?]),
            Tok::Indent => Err(MiniPyError::indentation("unexpected indent", self.line())),
            _ => self.simple_line(),
        }
    }

    /// The body of a compound statement: either an indented block, or simple
    /// statements on the same line (`if x: return 1`).
    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(&Tok::Colon)?;

        if !self.eat(&Tok::Newline) {
            return self.simple_line();
        }

        if !self.eat(&Tok::Indent) {
            return Err(MiniPyError::indentation(
                "expected an indented block",
                self.line(),
            ));
        }
        let mut body = Vec::new();
        while !self.at(&Tok::Dedent) && !self.at(&Tok::Eof) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            body.extend(self.statement()?);
        }
        self.expect(&Tok::Dedent)?;
        Ok(body)
    }

    fn simple_line(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = vec![self.simple_stmt()?];
        while self.eat(&Tok::Semicolon) {
            if self.at(&Tok::Newline) || self.at(&Tok::Eof) {
                break;
            }
            stmts.push(self.simple_stmt()?);
        }
        if !self.eat(&Tok::Newline) && !self.at(&Tok::Eof) && !self.at(&Tok::Dedent) {
            return Err(self.unexpected("expected end of line"));
        }
        Ok(stmts)
    }

    fn simple_stmt(&mut self) -> Result<Stmt> {
        let line = self.line();
        match self.peek() {
            Tok::Pass => {
                self.advance();
                Ok(Stmt::Pass)
            }
            Tok::Break => {
                self.advance();
                if self.loops == 0 {
                    return Err(MiniPyError::syntax("'break' outside loop", line));
                }
                Ok(Stmt::Break { line })
            }
            Tok::Continue => {
                self.advance();
                if self.loops == 0 {
                    return Err(MiniPyError::syntax("'continue' not properly in loop", line));
                }
                Ok(Stmt::Continue { line })
            }
            Tok::Return => {
                self.advance();
                if self.functions == 0 {
                    return Err(MiniPyError::syntax("'return' outside function", line));
                }
                let value =
                    if self.at(&Tok::Newline) || self.at(&Tok::Semicolon) || self.at(&Tok::Eof) {
                        None
                    } else {
                        Some(self.expression()?)
                    };
                Ok(Stmt::Return { value, line })
            }
            _ => self.expr_or_assignment(line),
        }
    }

    fn expr_or_assignment(&mut self, line: usize) -> Result<Stmt> {
        let target = self.expression()?;

        let op = match self.peek() {
            Tok::Assign => None,
            Tok::PlusAssign => Some(BinOp::Add),
            Tok::MinusAssign => Some(BinOp::Sub),
            Tok::StarAssign => Some(BinOp::Mul),
            Tok::SlashAssign => Some(BinOp::Div),
            _ => {
                return Ok(Stmt::Expr {
                    value: target,
                    line,
                });
            }
        };
        let is_plain = self.at(&Tok::Assign);
        self.advance();

        let Expr::Name { name, .. } = target else {
            let what = match target {
                Expr::Call { .. } => "function call",
                Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::None => {
                    "literal"
                }
                _ => "expression",
            };
            return Err(MiniPyError::syntax(
                format!("cannot assign to {what} here"),
                line,
            ));
        };

        let value = self.expression()?;
        Ok(if is_plain {
            Stmt::Assign {
                target: name,
                value,
                line,
            }
        } else {
            Stmt::AugAssign {
                target: name,
                op: op.expect("augmented assignment has an op"),
                value,
                line,
            }
        })
    }

    fn if_stmt(&mut self) -> Result<Stmt> {
        let line = self.line();
        self.expect(&Tok::If)?;

        let mut branches = vec![(self.expression()?, self.block()?)];
        loop {
            if self.eat(&Tok::Elif) {
                branches.push((self.expression()?, self.block()?));
            } else {
                break;
            }
        }
        let otherwise = if self.eat(&Tok::Else) {
            self.block()?
        } else {
            Vec::new()
        };

        Ok(Stmt::If {
            branches,
            otherwise,
            line,
        })
    }

    fn while_stmt(&mut self) -> Result<Stmt> {
        let line = self.line();
        self.expect(&Tok::While)?;
        let test = self.expression()?;

        self.loops += 1;
        let body = self.block();
        self.loops -= 1;

        Ok(Stmt::While {
            test,
            body: body?,
            line,
        })
    }

    fn for_stmt(&mut self) -> Result<Stmt> {
        let line = self.line();
        self.expect(&Tok::For)?;

        let Tok::Name(var) = self.advance() else {
            return Err(MiniPyError::syntax(
                "expected a loop variable after 'for'",
                line,
            ));
        };
        self.expect(&Tok::In)?;
        let iter = self.expression()?;

        self.loops += 1;
        let body = self.block();
        self.loops -= 1;

        Ok(Stmt::For {
            var,
            iter,
            body: body?,
            line,
        })
    }

    fn def_stmt(&mut self) -> Result<Stmt> {
        let line = self.line();
        self.expect(&Tok::Def)?;

        let Tok::Name(name) = self.advance() else {
            return Err(MiniPyError::syntax(
                "expected a function name after 'def'",
                line,
            ));
        };
        self.expect(&Tok::LParen)?;

        let mut params: Vec<String> = Vec::new();
        while !self.at(&Tok::RParen) {
            let Tok::Name(param) = self.advance() else {
                return Err(self.unexpected("expected a parameter name"));
            };
            if params.contains(&param) {
                return Err(MiniPyError::syntax(
                    format!("duplicate argument '{param}' in function definition"),
                    line,
                ));
            }
            params.push(param);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen)?;

        // A loop outside the function does not enclose its body.
        let enclosing_loops = std::mem::take(&mut self.loops);
        self.functions += 1;
        let body = self.block();
        self.functions -= 1;
        self.loops = enclosing_loops;

        let body = body?;
        // Python treats every name a function binds as local to the whole
        // function, so work out that set once, here.
        let mut locals: std::collections::HashSet<String> = params.iter().cloned().collect();
        bound_names(&body, &mut locals);

        Ok(Stmt::Def {
            name,
            params,
            body: Rc::new(body),
            locals: Rc::new(locals),
            line,
        })
    }

    // ----- expressions, lowest precedence first -----

    fn expression(&mut self) -> Result<Expr> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.and_expr()?;
        while self.eat(&Tok::Or) {
            let rhs = self.and_expr()?;
            lhs = Expr::Or {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.not_expr()?;
        while self.eat(&Tok::And) {
            let rhs = self.not_expr()?;
            lhs = Expr::And {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn not_expr(&mut self) -> Result<Expr> {
        let line = self.line();
        if self.eat(&Tok::Not) {
            let operand = self.not_expr()?;
            return Ok(Expr::Unary {
                op: UnOp::Not,
                operand: Box::new(operand),
                line,
            });
        }
        self.comparison()
    }

    /// `a < b <= c` becomes one node so that `b` is evaluated once.
    fn comparison(&mut self) -> Result<Expr> {
        let line = self.line();
        let first = self.additive()?;

        let mut rest = Vec::new();
        loop {
            let op = match self.peek() {
                Tok::Eq => CmpOp::Eq,
                Tok::NotEq => CmpOp::NotEq,
                Tok::Lt => CmpOp::Lt,
                Tok::LtEq => CmpOp::LtEq,
                Tok::Gt => CmpOp::Gt,
                Tok::GtEq => CmpOp::GtEq,
                _ => break,
            };
            self.advance();
            rest.push((op, self.additive()?));
        }

        Ok(if rest.is_empty() {
            first
        } else {
            Expr::Compare {
                first: Box::new(first),
                rest,
                line,
            }
        })
    }

    fn additive(&mut self) -> Result<Expr> {
        let mut lhs = self.multiplicative()?;
        loop {
            let line = self.line();
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.multiplicative()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                line,
            };
        }
        Ok(lhs)
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let line = self.line();
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::DoubleSlash => BinOp::FloorDiv,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.unary()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                line,
            };
        }
        Ok(lhs)
    }

    /// Unary `-`/`+` bind looser than `**`, so `-2 ** 2` is `-(2 ** 2)`.
    fn unary(&mut self) -> Result<Expr> {
        let line = self.line();
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Plus => UnOp::Pos,
            _ => return self.power(),
        };
        self.advance();
        let operand = self.unary()?;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
            line,
        })
    }

    /// Right-associative: `2 ** 3 ** 2` is `2 ** (3 ** 2)`.
    fn power(&mut self) -> Result<Expr> {
        let base = self.call_expr()?;
        let line = self.line();
        if self.eat(&Tok::DoubleStar) {
            let exponent = self.unary()?;
            return Ok(Expr::Binary {
                op: BinOp::Pow,
                lhs: Box::new(base),
                rhs: Box::new(exponent),
                line,
            });
        }
        Ok(base)
    }

    fn call_expr(&mut self) -> Result<Expr> {
        let mut expr = self.primary()?;
        loop {
            let line = self.line();
            if !self.eat(&Tok::LParen) {
                break;
            }
            let mut args = Vec::new();
            while !self.at(&Tok::RParen) {
                args.push(self.expression()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
            expr = Expr::Call {
                callee: Box::new(expr),
                args,
                line,
            };
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr> {
        let line = self.line();
        match self.advance() {
            Tok::Int(n) => Ok(Expr::Int(n)),
            Tok::Float(x) => Ok(Expr::Float(x)),
            Tok::Str(s) => Ok(Expr::Str(Rc::from(s.as_str()))),
            Tok::True => Ok(Expr::Bool(true)),
            Tok::False => Ok(Expr::Bool(false)),
            Tok::None_ => Ok(Expr::None),
            Tok::Name(name) => Ok(Expr::Name { name, line }),
            Tok::LParen => {
                let inner = self.expression()?;
                self.expect(&Tok::RParen)?;
                Ok(inner)
            }
            other => {
                self.pos = self.pos.saturating_sub(1);
                Err(MiniPyError::syntax(
                    format!("invalid syntax: unexpected {}", other.describe()),
                    line,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render an expression as an s-expression, so precedence and
    /// associativity are visible in one line.
    fn sexpr(e: &Expr) -> String {
        match e {
            Expr::Int(n) => n.to_string(),
            Expr::Float(x) => x.to_string(),
            Expr::Str(s) => format!("{s:?}"),
            Expr::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Expr::None => "None".to_string(),
            Expr::Name { name, .. } => name.clone(),
            Expr::Unary { op, operand, .. } => {
                let op = match op {
                    UnOp::Neg => "-",
                    UnOp::Pos => "+",
                    UnOp::Not => "not",
                };
                format!("({op} {})", sexpr(operand))
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                format!("({} {} {})", op.symbol(), sexpr(lhs), sexpr(rhs))
            }
            Expr::Compare { first, rest, .. } => {
                let mut out = format!("(cmp {}", sexpr(first));
                for (op, operand) in rest {
                    out.push_str(&format!(" {} {}", op.symbol(), sexpr(operand)));
                }
                out + ")"
            }
            Expr::And { lhs, rhs } => format!("(and {} {})", sexpr(lhs), sexpr(rhs)),
            Expr::Or { lhs, rhs } => format!("(or {} {})", sexpr(lhs), sexpr(rhs)),
            Expr::Call { callee, args, .. } => {
                let args: Vec<_> = args.iter().map(sexpr).collect();
                format!("(call {} [{}])", sexpr(callee), args.join(" "))
            }
        }
    }

    fn expr(src: &str) -> String {
        let stmts = parse(&format!("{src}\n")).expect("expected the expression to parse");
        match &stmts[0] {
            Stmt::Expr { value, .. } => sexpr(value),
            other => panic!("expected an expression statement, got {other:?}"),
        }
    }

    fn err(src: &str) -> String {
        parse(src).expect_err("expected a parse error").to_string()
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(expr("1 + 2 * 3"), "(+ 1 (* 2 3))");
        assert_eq!(expr("(1 + 2) * 3"), "(* (+ 1 2) 3)");
        assert_eq!(expr("1 - 2 - 3"), "(- (- 1 2) 3)");
        assert_eq!(expr("1 + 2 % 3 // 4"), "(+ 1 (// (% 2 3) 4))");
    }

    #[test]
    fn power_binds_tighter_than_unary_minus_and_is_right_associative() {
        assert_eq!(expr("-2 ** 2"), "(- (** 2 2))");
        assert_eq!(expr("2 ** 3 ** 2"), "(** 2 (** 3 2))");
        assert_eq!(expr("2 ** -1"), "(** 2 (- 1))");
        assert_eq!(expr("(-2) ** 2"), "(** (- 2) 2)");
    }

    #[test]
    fn comparison_chains_are_one_node() {
        assert_eq!(expr("1 < x < 10"), "(cmp 1 < x < 10)");
        assert_eq!(expr("a == b != c"), "(cmp a == b != c)");
        // Comparison binds looser than arithmetic, tighter than `not`.
        assert_eq!(expr("1 + 1 == 2"), "(cmp (+ 1 1) == 2)");
        assert_eq!(expr("not a == b"), "(not (cmp a == b))");
    }

    #[test]
    fn boolean_operators_nest_by_precedence() {
        assert_eq!(expr("a or b and c"), "(or a (and b c))");
        assert_eq!(expr("not a and b"), "(and (not a) b)");
    }

    #[test]
    fn calls_chain_and_take_arguments() {
        assert_eq!(expr("f(1, 2)"), "(call f [1 2])");
        assert_eq!(expr("f()(3)"), "(call (call f []) [3])");
        assert_eq!(expr("f(a + 1,)"), "(call f [(+ a 1)])");
    }

    #[test]
    fn if_elif_else_collects_branches() {
        let stmts = parse("if a:\n    x = 1\nelif b:\n    x = 2\nelse:\n    x = 3\n").unwrap();
        let Stmt::If {
            branches,
            otherwise,
            ..
        } = &stmts[0]
        else {
            panic!("expected an if statement");
        };
        assert_eq!(branches.len(), 2);
        assert_eq!(otherwise.len(), 1);
    }

    #[test]
    fn a_body_may_sit_on_the_header_line() {
        let stmts = parse("while a: x = 1; y = 2\n").unwrap();
        let Stmt::While { body, .. } = &stmts[0] else {
            panic!("expected a while statement");
        };
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn functions_capture_parameters_and_body() {
        let stmts = parse("def f(a, b):\n    return a + b\n").unwrap();
        let Stmt::Def {
            name, params, body, ..
        } = &stmts[0]
        else {
            panic!("expected a def");
        };
        assert_eq!(name, "f");
        assert_eq!(params, &["a".to_string(), "b".to_string()]);
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn augmented_assignment_records_its_operator() {
        let stmts = parse("x += 1\n").unwrap();
        assert!(
            matches!(&stmts[0], Stmt::AugAssign { op: BinOp::Add, target, .. } if target == "x")
        );
    }

    #[test]
    fn misplaced_control_flow_is_rejected_at_parse_time() {
        assert_eq!(err("break\n"), "SyntaxError: 'break' outside loop");
        assert_eq!(err("return 1\n"), "SyntaxError: 'return' outside function");
        // A loop outside a function does not enclose the function's body.
        assert_eq!(
            err("while a:\n    def f():\n        break\n"),
            "SyntaxError: 'break' outside loop"
        );
        // ...but a loop inside one does.
        assert!(parse("def f():\n    while a:\n        break\n").is_ok());
    }

    #[test]
    fn assignment_targets_must_be_names() {
        assert_eq!(err("1 = 2\n"), "SyntaxError: cannot assign to literal here");
        assert_eq!(
            err("f(x) = 2\n"),
            "SyntaxError: cannot assign to function call here"
        );
        assert_eq!(
            err("a + b = 2\n"),
            "SyntaxError: cannot assign to expression here"
        );
    }

    #[test]
    fn duplicate_parameters_are_rejected() {
        assert_eq!(
            err("def f(a, a):\n    pass\n"),
            "SyntaxError: duplicate argument 'a' in function definition"
        );
    }

    #[test]
    fn a_block_header_must_be_followed_by_an_indented_body() {
        assert_eq!(
            err("if a:\nx = 1\n"),
            "IndentationError: expected an indented block"
        );
    }

    #[test]
    fn line_numbers_point_at_the_failing_construct() {
        let e = parse("x = 1\ny = 2\nz = *\n").unwrap_err();
        assert_eq!(e.line, 3);
    }
}
