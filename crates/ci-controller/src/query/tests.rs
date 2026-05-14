//! Unit tests for the ChQL pipeline beyond the corpus runner.

use super::*;
use super::ast::{Ast, CmpOp, Value};
use super::compile::{compile, SqlBind};

// ── Parser ───────────────────────────────────────────────────────────────────

#[test]
fn parse_empty_returns_none() {
    assert_eq!(parse("").unwrap(), None);
    assert_eq!(parse("   ").unwrap(), None);
}

#[test]
fn parse_simple_eq() {
    let ast = parse("state:failed").unwrap().unwrap();
    let expected = Ast::Cmp {
        field: "state".into(),
        op: CmpOp::Eq,
        value: Value::Str { v: "failed".into() },
    };
    assert_eq!(ast, expected);
}

#[test]
fn parse_unknown_field_errors() {
    let err = parse("foo:bar").unwrap_err();
    assert!(err.message.contains("Unknown field"));
    assert_eq!(err.position, Some(0));
}

#[test]
fn parse_trailing_and_errors() {
    let err = parse("state:failed AND").unwrap_err();
    assert!(err.message.contains("Expected expression after 'AND'"));
}

// ── Compiler ─────────────────────────────────────────────────────────────────

#[test]
fn compile_eq_produces_placeholder() {
    let ast = parse("state:failed").unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert_eq!(frag.sql, "state = ?");
    assert_eq!(frag.binds, vec![SqlBind::Str("failed".into())]);
}

#[test]
fn compile_and_groups() {
    let ast = parse("state:failed AND branch:main").unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("AND"));
    assert_eq!(frag.binds.len(), 2);
}

#[test]
fn compile_wildcard_replaces_star_with_percent() {
    let ast = parse(r#"branch:"feat/*""#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert_eq!(frag.sql, "branch ILIKE ?");
    assert_eq!(frag.binds, vec![SqlBind::Str("feat/%".into())]);
}

#[test]
fn compile_ilike_wraps_with_percent() {
    let ast = parse(r#"branch:i("foo")"#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("ILIKE '%' || ? || '%'"));
    assert_eq!(frag.binds, vec![SqlBind::Str("foo".into())]);
}

#[test]
fn compile_regex_uses_tilde() {
    let ast = parse(r#"branch:re("^feat/")"#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("~ ?"));
    assert_eq!(frag.binds, vec![SqlBind::Str("^feat/".into())]);
}

#[test]
fn compile_exit_code_range_uses_subquery() {
    let ast = parse("exit_code:[1, 5]").unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("EXISTS"));
    assert!(frag.sql.contains("BETWEEN"));
    assert_eq!(frag.binds, vec![SqlBind::Num(1.0), SqlBind::Num(5.0)]);
}

#[test]
fn compile_stage_uses_jobs_subquery() {
    let ast = parse("stage:lint").unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("EXISTS"));
    assert!(frag.sql.contains("j_q.stage_name"));
    assert_eq!(frag.binds, vec![SqlBind::Str("lint".into())]);
}

#[test]
fn compile_repo_friendly_name_uses_subselect() {
    let ast = parse(r#"repo:"chola""#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("repo_id IN (SELECT id FROM repos WHERE repo_name = ?)"));
    assert_eq!(frag.binds, vec![SqlBind::Str("chola".into())]);
}

#[test]
fn compile_created_at_date() {
    let ast = parse(r#"created_at:>="2026-04-01""#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert_eq!(frag.sql, "created_at >= ?");
    assert_eq!(frag.binds.len(), 1);
    assert!(matches!(frag.binds[0], SqlBind::Date(_)));
}

#[test]
fn compile_to_pg_renumbers() {
    let frag = compile::SqlFragment {
        sql: "(a = ?) AND (b = ?)".into(),
        binds: vec![SqlBind::Str("x".into()), SqlBind::Str("y".into())],
    };
    let (pg, next) = frag.to_pg(7);
    assert_eq!(pg, "(a = $7) AND (b = $8)");
    assert_eq!(next, 9);
}

// ── SQL injection probes ─────────────────────────────────────────────────────

#[test]
fn injection_attempt_is_just_a_bind() {
    let ast = parse(r#"state:"'; DROP TABLE jobs; --""#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert_eq!(frag.sql, "state = ?");
    match &frag.binds[0] {
        SqlBind::Str(s) => assert_eq!(s, "'; DROP TABLE jobs; --"),
        _ => panic!("expected string bind"),
    }
}

#[test]
fn injection_in_regex_is_just_a_bind() {
    let ast = parse(r#"branch:re("'; DROP TABLE; --")"#).unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert!(frag.sql.contains("~ ?"));
    match &frag.binds[0] {
        SqlBind::Str(s) => assert_eq!(s, "'; DROP TABLE; --"),
        _ => panic!("expected string bind"),
    }
}

#[test]
fn no_sql_keywords_leak_from_user_field_value() {
    // The value contains keywords but only ends up as a bind, never SQL.
    let ast = parse("branch:UNION").unwrap().unwrap();
    let frag = compile(&ast).unwrap();
    assert_eq!(frag.sql, "branch = ?");
    assert_eq!(frag.binds, vec![SqlBind::Str("UNION".into())]);
}
