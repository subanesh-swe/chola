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
    Str {
        v: String,
    },
    Num {
        #[serde(with = "num_compact")]
        v: f64,
    },
    Date {
        v: String,
    },
}

/// JSON serialiser for numeric values: emits integers as integers, fractions
/// as floats. The corpus uses `5` (not `5.0`) for whole numbers — `serde_json`
/// would otherwise emit `5.0` for any `f64`, breaking equality.
mod num_compact {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde_json::Number;

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        // If the value is finite AND has no fractional component AND fits in
        // i64, emit as an integer. Otherwise emit as a JSON float.
        if v.is_finite() && v.fract() == 0.0 && *v >= i64::MIN as f64 && *v <= i64::MAX as f64 {
            s.serialize_i64(*v as i64)
        } else {
            s.serialize_f64(*v)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        let n = Number::deserialize(d)?;
        n.as_f64()
            .ok_or_else(|| serde::de::Error::custom("number out of range"))
    }
}
