//! ChQL → SQL compiler. Walks the [`Ast`] producing a parameterised SQL
//! fragment (with `?` placeholders) + an ordered list of binds. The caller
//! is responsible for splicing the fragment into a parent WHERE clause and
//! renumbering placeholders to Postgres `$N` form via [`SqlFragment::to_pg`].
//!
//! Safety: NO user string is ever interpolated into SQL. Column names come
//! from a Rust enum allowlist; values always flow through [`SqlBind`].

use super::ast::{Ast, CmpOp, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum CompileError {
    UnknownField(String),
    OperatorNotSupported { field: String, op: CmpOp },
    InvalidDate(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownField(name) => write!(f, "Unknown field '{name}'"),
            Self::OperatorNotSupported { field, op } => {
                write!(f, "Operator {op:?} not supported on field '{field}'")
            }
            Self::InvalidDate(v) => write!(f, "Invalid date literal '{v}' (expected YYYY-MM-DD or RFC3339)"),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<CompileError> for super::error::ChqlError {
    fn from(err: CompileError) -> Self {
        super::error::ChqlError::new(err.to_string())
    }
}

/// Allowed columns. Mapping below is the *only* place SQL identifiers come
/// from — user strings never touch SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    State,
    Branch,
    Repo,    // friendly name — resolved via subquery on `repos.name`
    RepoId,
    StageName,
    ExitCode,
    CreatedAt,
    CompletedAt,
}

impl Column {
    /// SQL fragment for this column when filtering on the `job_groups`-rooted
    /// query. For stage/exit_code we use an EXISTS subquery against `jobs`.
    /// The fragment is a value-context expression (e.g. `state` for an equals,
    /// or the inner column reference used inside a subquery).
    pub fn sql_ident(self) -> &'static str {
        match self {
            Column::State => "state",
            Column::Branch => "branch",
            // `repo` resolves through a subquery — see `compile_cmp` special path.
            Column::Repo => "repo_id",
            Column::RepoId => "repo_id",
            Column::StageName => "stage_name",
            Column::ExitCode => "exit_code",
            Column::CreatedAt => "created_at",
            Column::CompletedAt => "completed_at",
        }
    }

    /// True if filtering this column requires an EXISTS subquery against `jobs`.
    pub fn needs_jobs_subquery(self) -> bool {
        matches!(self, Column::StageName | Column::ExitCode)
    }
}

pub fn resolve(field: &str) -> Result<Column, CompileError> {
    Ok(match field {
        "state" => Column::State,
        "branch" => Column::Branch,
        "repo" => Column::Repo,
        "repo_id" => Column::RepoId,
        "stage" => Column::StageName,
        "exit_code" => Column::ExitCode,
        "created_at" => Column::CreatedAt,
        "completed_at" => Column::CompletedAt,
        other => return Err(CompileError::UnknownField(other.to_string())),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum SqlBind {
    Str(String),
    Num(f64),
    /// Date or datetime — stored as `DateTime<Utc>` for direct sqlx binding.
    Date(chrono::DateTime<chrono::Utc>),
}

impl SqlBind {
    /// Apply this bind to a sqlx Postgres query.
    pub fn bind<'q>(
        &'q self,
        q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
        match self {
            SqlBind::Str(s) => q.bind(s.as_str()),
            SqlBind::Num(n) => q.bind(*n),
            SqlBind::Date(d) => q.bind(*d),
        }
    }

    /// Same as `bind` but for `query_scalar`.
    pub fn bind_scalar<'q, T>(
        &'q self,
        q: sqlx::query::QueryScalar<'q, sqlx::Postgres, T, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, T, sqlx::postgres::PgArguments> {
        match self {
            SqlBind::Str(s) => q.bind(s.as_str()),
            SqlBind::Num(n) => q.bind(*n),
            SqlBind::Date(d) => q.bind(*d),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SqlFragment {
    /// SQL with `?` placeholders, one per `binds` entry, left-to-right.
    pub sql: String,
    pub binds: Vec<SqlBind>,
}

impl SqlFragment {
    /// Convert `?` placeholders to Postgres `$N` form, starting numbering
    /// from `start_idx`. Returns `(sql, new_next_idx)`. Robust against `?`
    /// chars inside literals — but we never emit literals, so a plain scan
    /// is fine.
    pub fn to_pg(&self, start_idx: usize) -> (String, usize) {
        let mut out = String::with_capacity(self.sql.len() + self.binds.len() * 2);
        let mut idx = start_idx;
        for ch in self.sql.chars() {
            if ch == '?' {
                out.push('$');
                out.push_str(&idx.to_string());
                idx += 1;
            } else {
                out.push(ch);
            }
        }
        (out, idx)
    }
}

/// Compile an [`Ast`] to a [`SqlFragment`].
pub fn compile(ast: &Ast) -> Result<SqlFragment, CompileError> {
    match ast {
        Ast::And { left, right } => {
            let l = compile(left)?;
            let r = compile(right)?;
            let mut binds = l.binds;
            binds.extend(r.binds);
            Ok(SqlFragment {
                sql: format!("({}) AND ({})", l.sql, r.sql),
                binds,
            })
        }
        Ast::Or { left, right } => {
            let l = compile(left)?;
            let r = compile(right)?;
            let mut binds = l.binds;
            binds.extend(r.binds);
            Ok(SqlFragment {
                sql: format!("({}) OR ({})", l.sql, r.sql),
                binds,
            })
        }
        Ast::Not { expr } => {
            let inner = compile(expr)?;
            Ok(SqlFragment {
                sql: format!("NOT ({})", inner.sql),
                binds: inner.binds,
            })
        }
        Ast::Cmp { field, op, value } => compile_cmp(field, *op, value),
        Ast::InRange { field, lo, hi } => compile_range(field, lo, hi),
    }
}

fn compile_cmp(field: &str, op: CmpOp, value: &Value) -> Result<SqlFragment, CompileError> {
    let col = resolve(field)?;

    // `repo` (friendly name) is the only field with a subquery — handle it
    // up front so the rest of the function can assume direct column access.
    if col == Column::Repo {
        return compile_repo_name(op, value);
    }

    // stage / exit_code → EXISTS subquery against jobs.
    if col.needs_jobs_subquery() {
        return compile_jobs_subquery(col, op, value, field);
    }

    let ident = col.sql_ident();
    let (op_sql, bind) = match op {
        CmpOp::Eq => ("=", value_to_bind(col, value)?),
        CmpOp::Neq => ("<>", value_to_bind(col, value)?),
        CmpOp::Gt => (">", value_to_bind(col, value)?),
        CmpOp::Gte => (">=", value_to_bind(col, value)?),
        CmpOp::Lt => ("<", value_to_bind(col, value)?),
        CmpOp::Lte => ("<=", value_to_bind(col, value)?),
        CmpOp::IlikeContains => {
            return Ok(SqlFragment {
                sql: format!("{ident} ILIKE '%' || ? || '%'"),
                binds: vec![value_to_bind_string(value)?],
            });
        }
        CmpOp::Regex => {
            return Ok(SqlFragment {
                sql: format!("{ident} ~ ?"),
                binds: vec![value_to_bind_string(value)?],
            });
        }
        CmpOp::Wildcard => {
            let s = value_to_string(value)?;
            let pat = s.replace('*', "%");
            return Ok(SqlFragment {
                sql: format!("{ident} ILIKE ?"),
                binds: vec![SqlBind::Str(pat)],
            });
        }
    };
    Ok(SqlFragment {
        sql: format!("{ident} {op_sql} ?"),
        binds: vec![bind],
    })
}

fn compile_repo_name(op: CmpOp, value: &Value) -> Result<SqlFragment, CompileError> {
    // Always resolve via subquery; only Eq makes sense for v1.
    let bind = match op {
        CmpOp::Eq | CmpOp::Neq => value_to_bind_string(value)?,
        _ => {
            return Err(CompileError::OperatorNotSupported {
                field: "repo".into(),
                op,
            })
        }
    };
    let cmp = if op == CmpOp::Neq { "NOT IN" } else { "IN" };
    Ok(SqlFragment {
        sql: format!("repo_id {cmp} (SELECT id FROM repos WHERE repo_name = ?)"),
        binds: vec![bind],
    })
}

fn compile_jobs_subquery(
    col: Column,
    op: CmpOp,
    value: &Value,
    field: &str,
) -> Result<SqlFragment, CompileError> {
    let inner_col = col.sql_ident(); // "stage_name" or "exit_code"
    let (op_sql, bind) = match (col, op) {
        (Column::StageName, CmpOp::Eq) => ("=", value_to_bind_string(value)?),
        (Column::StageName, CmpOp::Neq) => ("<>", value_to_bind_string(value)?),
        (Column::ExitCode, CmpOp::Eq) => ("=", value_to_bind_number(value)?),
        (Column::ExitCode, CmpOp::Neq) => ("<>", value_to_bind_number(value)?),
        (Column::ExitCode, CmpOp::Gt) => (">", value_to_bind_number(value)?),
        (Column::ExitCode, CmpOp::Gte) => (">=", value_to_bind_number(value)?),
        (Column::ExitCode, CmpOp::Lt) => ("<", value_to_bind_number(value)?),
        (Column::ExitCode, CmpOp::Lte) => ("<=", value_to_bind_number(value)?),
        _ => {
            return Err(CompileError::OperatorNotSupported {
                field: field.into(),
                op,
            })
        }
    };
    Ok(SqlFragment {
        sql: format!(
            "EXISTS (SELECT 1 FROM jobs j_q WHERE j_q.job_group_id = job_groups.id \
             AND j_q.{inner_col} {op_sql} ?)"
        ),
        binds: vec![bind],
    })
}

fn compile_range(field: &str, lo: &Value, hi: &Value) -> Result<SqlFragment, CompileError> {
    let col = resolve(field)?;
    if col.needs_jobs_subquery() {
        // exit_code range is the only realistic case.
        let lo_b = value_to_bind_number(lo)?;
        let hi_b = value_to_bind_number(hi)?;
        return Ok(SqlFragment {
            sql: "EXISTS (SELECT 1 FROM jobs j_q WHERE j_q.job_group_id = job_groups.id \
                  AND j_q.exit_code BETWEEN ? AND ?)"
                .to_string(),
            binds: vec![lo_b, hi_b],
        });
    }
    let ident = col.sql_ident();
    Ok(SqlFragment {
        sql: format!("{ident} BETWEEN ? AND ?"),
        binds: vec![value_to_bind(col, lo)?, value_to_bind(col, hi)?],
    })
}

fn value_to_bind(col: Column, value: &Value) -> Result<SqlBind, CompileError> {
    // Date columns: accept Date kind directly; coerce Str if it parses.
    match col {
        Column::CreatedAt | Column::CompletedAt => match value {
            Value::Date { v } | Value::Str { v } => parse_date_bind(v),
            Value::Num { .. } => Err(CompileError::InvalidDate("(number)".into())),
        },
        Column::ExitCode => value_to_bind_number(value),
        _ => value_to_bind_string(value),
    }
}

fn value_to_bind_string(value: &Value) -> Result<SqlBind, CompileError> {
    match value {
        Value::Str { v } => Ok(SqlBind::Str(v.clone())),
        Value::Date { v } => Ok(SqlBind::Str(v.clone())),
        Value::Num { v } => Ok(SqlBind::Str(v.to_string())),
    }
}

fn value_to_bind_number(value: &Value) -> Result<SqlBind, CompileError> {
    match value {
        Value::Num { v } => Ok(SqlBind::Num(*v)),
        Value::Str { v } => v
            .parse::<f64>()
            .map(SqlBind::Num)
            .map_err(|_| CompileError::InvalidDate(v.clone())),
        Value::Date { v } => Err(CompileError::InvalidDate(v.clone())),
    }
}

fn value_to_string(value: &Value) -> Result<String, CompileError> {
    Ok(match value {
        Value::Str { v } => v.clone(),
        Value::Date { v } => v.clone(),
        Value::Num { v } => v.to_string(),
    })
}

/// Parse `YYYY-MM-DD` (interpreted as UTC midnight start) or RFC3339 into a
/// `DateTime<Utc>` bind. Anything else → `InvalidDate`.
fn parse_date_bind(v: &str) -> Result<SqlBind, CompileError> {
    // Try RFC3339 first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(v) {
        return Ok(SqlBind::Date(dt.with_timezone(&chrono::Utc)));
    }
    // Then YYYY-MM-DD → 00:00:00Z
    if let Ok(d) = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        let dt = d
            .and_hms_opt(0, 0, 0)
            .map(|n| n.and_utc())
            .ok_or_else(|| CompileError::InvalidDate(v.into()))?;
        return Ok(SqlBind::Date(dt));
    }
    Err(CompileError::InvalidDate(v.into()))
}
