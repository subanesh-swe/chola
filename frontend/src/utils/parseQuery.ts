/**
 * KQL-lite query parser.
 *
 * Grammar: term (WS term)*
 *   term  := field operator value
 *   field := branch | repo | stage | state | exit_code | from | to
 *   operator := ":" | ":!=" | ":>=" | ":<=" | ":>" | ":<"
 *   value := quoted-string | bare-token
 *
 * Whitespace between terms is treated as AND. No OR, no parens.
 *
 * Field mapping to BuildFilters:
 *   branch    -> branch  (wildcards stripped, warning emitted)
 *   repo      -> repo (UUID) or warning if not UUID
 *   stage     -> stage  (wildcards stripped, warning emitted)
 *   state     -> state[] (appended)
 *   exit_code -> exitCode  ("!=0" / ">=N" / "<N" -> "nonzero" sentinel; "0" -> "0")
 *   from      -> dateFrom
 *   to        -> dateTo
 */

import type { BuildFilters } from '../hooks/useUrlFilters';

export interface ParseError {
  message: string;
  hint?: string;
  start?: number;
  end?: number;
}

export type ParseResult =
  | { ok: true; filters: Partial<BuildFilters>; warnings: ParseError[] }
  | { ok: false; error: ParseError };

const KNOWN_FIELDS = ['branch', 'repo', 'stage', 'state', 'exit_code', 'from', 'to'] as const;
type KnownField = (typeof KNOWN_FIELDS)[number];

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

// Operators that may appear between ":" and the value in exit_code terms.
// e.g. exit_code:!=0  exit_code:>=1  exit_code:<5
const EXIT_OPS = ['!=', '>=', '<=', '>', '<'] as const;
type ExitOp = (typeof EXIT_OPS)[number];

/** Advance past whitespace, return new index. */
function skipWs(s: string, i: number): number {
  while (i < s.length && (s[i] === ' ' || s[i] === '\t')) i++;
  return i;
}

/** Read a quoted string starting at index i (i points to opening `"`).
 *  Returns [value, endIndex] where endIndex is after the closing `"`. */
function readQuoted(s: string, i: number): [string, number] | null {
  if (s[i] !== '"') return null;
  let j = i + 1;
  let out = '';
  while (j < s.length && s[j] !== '"') {
    if (s[j] === '\\' && j + 1 < s.length) {
      out += s[j + 1];
      j += 2;
    } else {
      out += s[j];
      j++;
    }
  }
  if (j >= s.length) return null; // unterminated quote
  return [out, j + 1];
}

/** Read a bare token (non-whitespace, stops at whitespace). */
function readBare(s: string, i: number): [string, number] {
  let j = i;
  while (j < s.length && s[j] !== ' ' && s[j] !== '\t') j++;
  return [s.slice(i, j), j];
}

/** Try to read exit_code operator prefix. Returns [op, newIndex] or null. */
function readExitOp(s: string, i: number): [ExitOp, number] | null {
  // Try longer first to avoid greedy match of ">" when ">=" is present.
  for (const op of EXIT_OPS) {
    if (s.startsWith(op, i)) return [op, i + op.length];
  }
  return null;
}

interface Term {
  field: string;
  op: string; // "=" for default colon, "!=", ">=", "<=", ">", "<"
  value: string;
  start: number;
  end: number;
}

/** Parse the raw query string into a list of terms, or return a ParseError. */
function tokenize(input: string): Term[] | ParseError {
  const terms: Term[] = [];
  let i = 0;

  while (true) {
    i = skipWs(input, i);
    if (i >= input.length) break;

    const termStart = i;

    // Read field name (up to ':')
    let fieldEnd = i;
    while (fieldEnd < input.length && input[fieldEnd] !== ':' && input[fieldEnd] !== ' ' && input[fieldEnd] !== '\t') {
      fieldEnd++;
    }
    if (fieldEnd >= input.length || input[fieldEnd] !== ':') {
      const tok = input.slice(i, fieldEnd || i + 10);
      return {
        message: `Expected "field:value" but got "${tok}"`,
        hint: 'Each term must have the form field:value (e.g. branch:main)',
        start: i,
        end: fieldEnd,
      };
    }

    const field = input.slice(i, fieldEnd);
    i = fieldEnd + 1; // skip ':'

    // For exit_code, try to read operator prefix before the value.
    let op: string = '=';
    if (field === 'exit_code') {
      const maybeOp = readExitOp(input, i);
      if (maybeOp) {
        [op, i] = maybeOp;
      }
    }

    // Read value (quoted or bare).
    let value: string;
    if (i < input.length && input[i] === '"') {
      const res = readQuoted(input, i);
      if (!res) {
        return {
          message: 'Unterminated quoted string',
          start: i,
          end: input.length,
        };
      }
      [value, i] = res;
    } else {
      [value, i] = readBare(input, i);
    }

    if (value === '') {
      return {
        message: `Empty value for field "${field}"`,
        start: termStart,
        end: i,
      };
    }

    terms.push({ field, op, value, start: termStart, end: i });
  }

  return terms;
}

/** Strip trailing/leading '*' wildcard, return {stripped, hadWildcard}. */
function stripWildcard(v: string): { stripped: string; hadWildcard: boolean } {
  const hadWildcard = v.includes('*');
  const stripped = v.replace(/\*/g, '');
  return { stripped, hadWildcard };
}

/** Map a parsed term into a BuildFilters patch + optional warning. */
function applyTerm(
  term: Term,
  acc: Partial<BuildFilters>,
  warnings: ParseError[],
): Partial<BuildFilters> | ParseError {
  const { field, op, value, start, end } = term;

  if (!(KNOWN_FIELDS as readonly string[]).includes(field)) {
    return {
      message: `Unknown field "${field}"`,
      hint: `Try one of: ${KNOWN_FIELDS.join(', ')}`,
      start,
      end,
    };
  }

  const knownField = field as KnownField;

  switch (knownField) {
    case 'branch': {
      const { stripped, hadWildcard } = stripWildcard(value);
      if (hadWildcard) {
        warnings.push({
          message: `Wildcards not yet supported on "branch"; using exact match "${stripped}"`,
          hint: 'Wildcard support is planned for a future release.',
          start,
          end,
        });
      }
      return { ...acc, branch: stripped };
    }

    case 'stage': {
      const { stripped, hadWildcard } = stripWildcard(value);
      if (hadWildcard) {
        warnings.push({
          message: `Wildcards not yet supported on "stage"; using exact match "${stripped}"`,
          hint: 'Wildcard support is planned for a future release.',
          start,
          end,
        });
      }
      return { ...acc, stage: stripped };
    }

    case 'repo': {
      if (UUID_RE.test(value)) {
        return { ...acc, repo: value };
      }
      warnings.push({
        message: `"repo:${value}" is not a UUID — friendly-name lookup not yet supported; ignoring.`,
        hint:
          'Use the repo UUID (visible in the URL on the Repo detail page). ' +
          'Name-based lookup is planned as a follow-up (W6).',
        start,
        end,
      });
      return acc;
    }

    case 'state': {
      const prev = acc.state ?? [];
      // Deduplicate: only add if not already present.
      if (!prev.includes(value)) {
        return { ...acc, state: [...prev, value] };
      }
      return acc;
    }

    case 'exit_code': {
      // op = "=" means plain "exit_code:N"
      // op = "!=" | ">=" | "<=" | ">" | "<" — treat as "non-zero" sentinel
      if (op === '=') {
        // Plain numeric value.
        const n = Number(value);
        if (!Number.isInteger(n) || String(n) !== value) {
          return {
            message: `exit_code value "${value}" is not an integer`,
            start,
            end,
          };
        }
        // "0" -> "0"; anything else -> "nonzero" sentinel
        if (n === 0) return { ...acc, exitCode: '0' };
        return { ...acc, exitCode: 'nonzero' };
      }
      // Any comparison operator -> treat as non-zero in v1.
      return { ...acc, exitCode: 'nonzero' };
    }

    case 'from': {
      return { ...acc, dateFrom: value };
    }

    case 'to': {
      return { ...acc, dateTo: value };
    }
  }
}

export function parseQuery(input: string): ParseResult {
  const trimmed = input.trim();

  if (trimmed === '') {
    return { ok: true, filters: {}, warnings: [] };
  }

  const tokensOrError = tokenize(trimmed);
  if (!Array.isArray(tokensOrError)) {
    return { ok: false, error: tokensOrError };
  }

  let acc: Partial<BuildFilters> = {};
  const warnings: ParseError[] = [];

  for (const term of tokensOrError) {
    const result = applyTerm(term, acc, warnings);
    if ('message' in result && 'start' in result) {
      // applyTerm returns a ParseError for unknown fields.
      return { ok: false, error: result as ParseError };
    }
    acc = result as Partial<BuildFilters>;
  }

  return { ok: true, filters: acc, warnings };
}
