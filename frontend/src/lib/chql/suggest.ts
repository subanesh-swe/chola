import type { IToken } from 'chevrotain';
import { ChqlLexer } from './lexer';
import { getParser } from './parser';
import { VALID_FIELDS } from './ast';
import type { ValidField } from './ast';

// ── Public types ──────────────────────────────────────────────────────────────

export interface Suggestion {
  label: string;
  insertText: string;
  detail?: string;
  kind: 'field' | 'value' | 'operator' | 'keyword';
}

/**
 * Async provider of known values for a given field.
 * Return an empty array if the field has no enumerable values.
 */
export type FieldValueProvider = (field: ValidField) => string[] | Promise<string[]>;

// Token-name → keyword suggestion mapping (fixes Bug 2a: token names are 'And'/'Or'/'Not',
// not 'AND'/'OR'/'NOT').
const KW_BY_TOKEN: Record<string, Suggestion> = {
  And: { label: 'AND', insertText: 'AND ', kind: 'keyword', detail: 'logical and' },
  Or:  { label: 'OR',  insertText: 'OR ',  kind: 'keyword', detail: 'logical or'  },
  Not: { label: 'NOT', insertText: 'NOT ', kind: 'keyword', detail: 'logical not' },
};

const MAX_SUGGESTIONS = 20;

// ── Helpers ───────────────────────────────────────────────────────────────────

/** True when cursor is past the end of the last token (trailing whitespace). */
function hasCursorPassedLastToken(tokens: IToken[], cursorPos: number): boolean {
  if (tokens.length === 0) return false;
  const last = tokens[tokens.length - 1];
  return cursorPos > (last.endOffset ?? 0) + 1;
}

// ── Value-context detection (fixes Bug 2b) ────────────────────────────────────

interface ValueContext {
  field: ValidField;
  partial: string;
}

/**
 * Detects whether the cursor is in a value-entry position (<field> ':' [<partial>?]).
 * Returns { field, partial } or null.
 *
 * Trailing-space check: if the cursor is past the last token's end offset, the
 * user has finished typing the value and is now at a word boundary — not in value
 * context any more.
 */
function valueContext(tokens: IToken[], cursorPos: number): ValueContext | null {
  if (tokens.length === 0) return null;
  const last = tokens[tokens.length - 1];

  // Case A: last token IS ':' — cursor is right after the colon.
  if (last.tokenType.name === 'Colon') {
    // Trailing space after ':' is OK — still value context (partial is '').
    const prev = tokens[tokens.length - 2];
    if (
      prev?.tokenType.name === 'Identifier' &&
      (VALID_FIELDS as readonly string[]).includes(prev.image)
    ) {
      return { field: prev.image as ValidField, partial: '' };
    }
    return null;
  }

  // Case B: last token is a partial value right after ':'.
  if (
    last.tokenType.name === 'Identifier' ||
    last.tokenType.name === 'StringLiteral' ||
    last.tokenType.name === 'NumberLiteral'
  ) {
    // If cursor is past the last token, the user already completed the value word
    // and is now separated by whitespace — fall through to field/keyword branch.
    if (hasCursorPassedLastToken(tokens, cursorPos)) return null;

    const beforeLast = tokens[tokens.length - 2];
    const fieldTok = tokens[tokens.length - 3];
    if (
      beforeLast?.tokenType.name === 'Colon' &&
      fieldTok?.tokenType.name === 'Identifier' &&
      (VALID_FIELDS as readonly string[]).includes(fieldTok.image)
    ) {
      const partial =
        last.tokenType.name === 'StringLiteral'
          ? last.image.slice(1, last.image.endsWith('"') ? -1 : undefined)
          : last.image;
      return { field: fieldTok.image as ValidField, partial };
    }
  }

  return null;
}

// ── Main function ─────────────────────────────────────────────────────────────

export async function suggestAt(
  input: string,
  cursorPos: number,
  fieldValues: FieldValueProvider,
): Promise<Suggestion[]> {
  const prefix = input.slice(0, cursorPos);

  const lexResult = ChqlLexer.tokenize(prefix);
  const parser = getParser();
  const contentAssist = parser.computeContentAssist('query', lexResult.tokens);

  const nextTokenNames = new Set(contentAssist.map((a) => a.nextTokenType.name));

  // ── Value-context (Bug 2b: detect partial typing after ':') ──────────────
  const valCtx = valueContext(lexResult.tokens, cursorPos);
  if (valCtx) {
    const values = await fieldValues(valCtx.field);
    const matched = values
      .filter((v) => v.toLowerCase().startsWith(valCtx.partial.toLowerCase()))
      .slice(0, MAX_SUGGESTIONS)
      .map((v): Suggestion => ({
        label: v,
        insertText: /\s/.test(v) ? `"${v}"` : v,
        kind: 'value',
        detail: valCtx.field,
      }));

    // Numeric-friendly fields: include compare operators when no partial yet.
    if (valCtx.field === 'exit_code' && valCtx.partial === '') {
      const numericOps: Suggestion[] = [
        { label: '!=', insertText: '!',  kind: 'operator', detail: 'not equal' },
        { label: '>=', insertText: '>=', kind: 'operator', detail: '>=' },
        { label: '<=', insertText: '<=', kind: 'operator', detail: '<=' },
        { label: '>',  insertText: '>',  kind: 'operator', detail: '>' },
        { label: '<',  insertText: '<',  kind: 'operator', detail: '<' },
      ];
      return [...numericOps, ...matched].slice(0, MAX_SUGGESTIONS);
    }

    // Text-search-friendly fields: include i()/re()/! helpers when no partial yet.
    if (
      (['branch', 'repo', 'stage'] as ValidField[]).includes(valCtx.field) &&
      valCtx.partial === ''
    ) {
      const textOps: Suggestion[] = [
        { label: 'i("…")',  insertText: 'i("',  kind: 'operator', detail: 'case-insensitive contains' },
        { label: 're("…")', insertText: 're("', kind: 'operator', detail: 'regex match' },
        { label: '!',       insertText: '!',    kind: 'operator', detail: 'not equal' },
      ];
      return [...matched, ...textOps].slice(0, MAX_SUGGESTIONS);
    }

    return matched.slice(0, MAX_SUGGESTIONS);
  }

  // ── Not in value context: suggest fields and/or keywords ─────────────────

  const results: Suggestion[] = [];
  const lastToken = lexResult.tokens[lexResult.tokens.length - 1];
  const lastImage = lastToken?.image ?? '';
  const lastType = lastToken?.tokenType?.name ?? '';

  // Field suggestions (Bug 2c: case-insensitive prefix).
  //
  // We show fields when:
  //   a) contentAssist says Identifier is next (start of a new predicate), OR
  //   b) contentAssist says Colon is next — meaning we just typed a partial
  //      field name (Identifier token) that the parser expects ':' after. In
  //      that case filter by the partial (Bug 2a for field names).
  const isPartialField = nextTokenNames.has('Colon') && lastType === 'Identifier';
  if (nextTokenNames.has('Identifier') || isPartialField) {
    const partialField = isPartialField ? lastImage.toLowerCase() : '';
    const fields = VALID_FIELDS
      .filter((f) => f.startsWith(partialField))
      .map((f): Suggestion => ({
        label: f,
        insertText: f + ':',
        detail: 'field',
        kind: 'field',
      }));
    results.push(...fields);
  }

  // Keyword suggestions (Bug 2a: use token-type names, not label strings).
  for (const t of ['And', 'Or', 'Not'] as const) {
    if (nextTokenNames.has(t)) results.push(KW_BY_TOKEN[t]);
  }

  return results.slice(0, MAX_SUGGESTIONS);
}
