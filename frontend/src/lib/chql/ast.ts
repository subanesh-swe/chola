// ChQL AST — mirrors Rust backend's serde-serialized shape exactly.

export type CmpOp =
  | 'eq'
  | 'neq'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte'
  | 'ilike_contains'
  | 'regex'
  | 'wildcard';

export type Value =
  | { kind: 'str'; v: string }
  | { kind: 'num'; v: number }
  | { kind: 'date'; v: string };

export type Ast =
  | { type: 'and'; left: Ast; right: Ast }
  | { type: 'or'; left: Ast; right: Ast }
  | { type: 'not'; expr: Ast }
  | { type: 'cmp'; field: string; op: CmpOp; value: Value }
  | { type: 'in_range'; field: string; lo: Value; hi: Value };

export const VALID_FIELDS = [
  'state',
  'branch',
  'repo',
  'repo_id',
  'stage',
  'exit_code',
  'created_at',
  'completed_at',
] as const;

export type ValidField = (typeof VALID_FIELDS)[number];

/** Fields where value is a date (ISO-8601 string). */
export const DATE_FIELDS: ReadonlySet<string> = new Set(['created_at', 'completed_at']);

/** Fields where value is numeric. */
export const NUMERIC_FIELDS: ReadonlySet<string> = new Set(['exit_code']);

/** Regex for ISO-8601 date strings (YYYY-MM-DD). */
const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

export function isDateString(s: string): boolean {
  return DATE_RE.test(s);
}
