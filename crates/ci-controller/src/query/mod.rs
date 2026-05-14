//! ChQL — Chola Query Language. LogsQL-flavored typed search for CI builds.
//!
//! Pipeline: [`lexer`] → parser → compile. The intermediate representation
//! is [`ast::Ast`], which serialises to the JSON shape shared with the
//! TypeScript parser via `tests/corpus/*.json`.
//!
//! See `local/plans/CHQL.md` for the grammar specification.

pub mod ast;
pub mod compile;
pub mod error;
pub mod lexer;
pub mod parser;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use ast::{Ast, CmpOp, Value};
#[allow(unused_imports)]
pub use compile::{compile, CompileError, SqlBind, SqlFragment};
#[allow(unused_imports)]
pub use error::ChqlError;
#[allow(unused_imports)]
pub use parser::parse;
