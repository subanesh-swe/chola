//! ChQL — Chola Query Language. LogsQL-flavored typed search for CI builds.
//!
//! Pipeline: [`lexer`] → parser → compile. The intermediate representation
//! is [`ast::Ast`], which serialises to the JSON shape shared with the
//! TypeScript parser via `tests/corpus/*.json`.
//!
//! See `local/plans/CHQL.md` for the grammar specification.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

pub use ast::{Ast, CmpOp, Value};
pub use error::ChqlError;
pub use parser::parse;
