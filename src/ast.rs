//! The syntax tree.
//!
//! Nodes that can fail at runtime carry the source line, so an error can name
//! the line the user actually wrote.

use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
}

impl BinOp {
    /// The symbol, as it appears in a `TypeError`.
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl CmpOp {
    pub fn symbol(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtEq => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtEq => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Bool(bool),
    None,
    Name {
        name: String,
        line: usize,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        line: usize,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        line: usize,
    },
    /// A whole comparison chain: `a < b <= c` is one node, so that `b` is
    /// evaluated once and the chain short-circuits like Python's.
    Compare {
        first: Box<Expr>,
        rest: Vec<(CmpOp, Expr)>,
        line: usize,
    },
    /// `and` / `or` yield one of their operands, not a bool.
    And {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Or {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        line: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr {
        value: Expr,
        line: usize,
    },
    Assign {
        target: String,
        value: Expr,
        line: usize,
    },
    AugAssign {
        target: String,
        op: BinOp,
        value: Expr,
        line: usize,
    },
    /// `if` / `elif` / `else`: one entry in `branches` per tested condition.
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        otherwise: Vec<Stmt>,
        line: usize,
    },
    While {
        test: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    Def {
        name: String,
        params: Vec<String>,
        /// Shared with every closure made from this definition.
        body: Rc<Vec<Stmt>>,
        line: usize,
    },
    Return {
        value: Option<Expr>,
        line: usize,
    },
    Pass,
    Break {
        line: usize,
    },
    Continue {
        line: usize,
    },
}
