//! ChQL AST types. Serialization is the shared contract with the TS parser —
//! the JSON shape must exactly match `tests/corpus/*.json`. See
//! `local/plans/CHQL.md` for the grammar.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ast {
    And {
        left: Box<Ast>,
        right: Box<Ast>,
    },
    Or {
        left: Box<Ast>,
        right: Box<Ast>,
    },
    Not {
        expr: Box<Ast>,
    },
    Cmp {
        field: String,
        op: CmpOp,
        value: Value,
    },
    InRange {
        field: String,
        lo: Value,
        hi: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    IlikeContains,
    Regex,
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Value {
    Str { v: String },
    Num { v: f64 },
    Date { v: String },
}
