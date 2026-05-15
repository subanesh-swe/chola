//! Shared-corpus runner. Walks `tests/corpus/*.json` (workspace-relative,
//! resolved via `CARGO_MANIFEST_DIR/../../tests/corpus`), parses each `input`,
//! and compares the resulting AST to the expected JSON. For error cases
//! (`expected.error` present) we only assert that parse returned `Err`; the
//! exact message wording is owned by each parser and may drift.
//!
//! Run with: `cargo test -p chola-controller chql_corpus -- --nocapture`

use std::fs;
use std::path::PathBuf;

use super::{parse, Ast};

/// Resolve the workspace-root `tests/corpus` directory. We use
/// `CARGO_MANIFEST_DIR` (the controller crate dir) and go up two levels.
fn corpus_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("..")
        .join("..")
        .join("tests")
        .join("corpus")
}

/// Each corpus file is either `{ input, ast }` or `{ input, error }`.
/// We parse into a generic `Value` first so we can distinguish "no ast key"
/// (error case) from "ast: null" (empty-input case).
#[derive(serde::Deserialize)]
struct CorpusCase {
    input: String,
}

#[test]
fn chql_corpus_all_cases() {
    let dir = corpus_dir();
    assert!(
        dir.is_dir(),
        "corpus directory not found at {}",
        dir.display()
    );

    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .map(|e| e.path())
        .collect();
    files.sort();

    let mut failures: Vec<String> = Vec::new();
    let mut ok_success = 0usize;
    let mut ok_error = 0usize;

    for path in &files {
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("[{}] read error: {e}", path.display()));
                continue;
            }
        };
        let case: CorpusCase = match serde_json::from_str(&raw) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("[{}] json parse error: {e}", path.display()));
                continue;
            }
        };
        // Re-parse as generic Value so we can tell "ast: null" from "no ast key".
        let raw_json: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("[{}] raw json error: {e}", path.display()));
                continue;
            }
        };
        let has_ast_key = raw_json.get("ast").is_some();
        let has_error_key = raw_json.get("error").is_some();
        let expected_ast_opt = if has_ast_key {
            Some(
                raw_json
                    .get("ast")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            )
        } else {
            None
        };

        let result = parse(&case.input);

        if let Some(expected_ast) = expected_ast_opt {
            match result {
                Ok(actual) => {
                    let actual_json = ast_to_json(&actual);
                    if actual_json != expected_ast {
                        failures.push(format!(
                            "[{}] AST mismatch\n  input:    {:?}\n  expected: {}\n  got:      {}",
                            path.display(),
                            case.input,
                            serde_json::to_string(&expected_ast).unwrap_or_default(),
                            serde_json::to_string(&actual_json).unwrap_or_default(),
                        ));
                    } else {
                        ok_success += 1;
                    }
                }
                Err(e) => {
                    failures.push(format!(
                        "[{}] expected ast but parse failed: {} (input: {:?})",
                        path.display(),
                        e,
                        case.input
                    ));
                }
            }
        } else if has_error_key {
            // Lenient: just require Err. Exact message/position is per-parser.
            match result {
                Err(_) => ok_error += 1,
                Ok(actual) => {
                    failures.push(format!(
                        "[{}] expected parse error but got AST {:?} (input: {:?})",
                        path.display(),
                        actual,
                        case.input
                    ));
                }
            }
        } else {
            failures.push(format!(
                "[{}] malformed corpus case: neither ast nor error present",
                path.display()
            ));
        }
    }

    let total = files.len();
    eprintln!(
        "ChQL corpus: {} files total, {} success-cases passed, {} error-cases passed, {} failures",
        total,
        ok_success,
        ok_error,
        failures.len()
    );
    if !failures.is_empty() {
        for f in &failures {
            eprintln!("FAIL: {f}");
        }
        panic!("{} corpus case(s) failed", failures.len());
    }
}

/// Helper to convert an `Option<Ast>` (the parser's return value) into a JSON
/// value matching the corpus shape. `None` → `null`.
fn ast_to_json(actual: &Option<Ast>) -> serde_json::Value {
    match actual {
        Some(a) => serde_json::to_value(a).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    }
}
