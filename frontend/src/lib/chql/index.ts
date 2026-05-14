import { ChqlLexer } from './lexer';
import { getParser } from './parser';
import { getVisitor } from './visitor';
import { VALID_FIELDS } from './ast';

export type { Ast, CmpOp, Value } from './ast';
export { suggestAt } from './suggest';
export type { Suggestion, FieldValueProvider } from './suggest';

// ── Parse result types ────────────────────────────────────────────────────────

export interface Issue {
  message: string;
  position?: { start: number; end: number };
  hint?: string;
}

export type ParseResult =
  | { ok: true; ast: import('./ast').Ast | null; warnings: Issue[] }
  | { ok: false; errors: Issue[] };

// ── Main parse function ────────────────────────────────────────────────────────

export function parse(input: string): ParseResult {
  // Empty input is valid — returns null AST.
  const trimmed = input.trim();
  if (trimmed === '') {
    return { ok: true, ast: null, warnings: [] };
  }

  // ── Lex ──────────────────────────────────────────────────────────────────
  const lexResult = ChqlLexer.tokenize(input);

  // Surface unterminated string literal errors from the lexer.
  if (lexResult.errors.length > 0) {
    const lexErr = lexResult.errors[0];
    const msg = classifyLexerError(input, lexErr);
    return {
      ok: false,
      errors: [{ message: msg, position: { start: lexErr.offset, end: lexErr.offset + (lexErr.length ?? 1) } }],
    };
  }

  // ── Parse ─────────────────────────────────────────────────────────────────
  const parser = getParser();
  parser.input = lexResult.tokens;
  const cst = parser.query();

  if (parser.errors.length > 0) {
    const errors = parser.errors.map((e) => ({
      message: classifyParserError(input, e),
      position: e.token
        ? { start: e.token.startOffset, end: e.token.endOffset ?? e.token.startOffset }
        : undefined,
    }));
    return { ok: false, errors };
  }

  // ── Visit (CST → AST) ────────────────────────────────────────────────────
  const visitor = getVisitor();
  visitor.errors = [];
  const ast = visitor.visit(cst) as import('./ast').Ast | null;

  if (visitor.errors.length > 0) {
    return { ok: false, errors: visitor.errors.map((e) => ({ message: e.message, position: e.position })) };
  }

  return { ok: true, ast: ast ?? null, warnings: [] };
}

// ── Error classification helpers ─────────────────────────────────────────────

function classifyLexerError(input: string, err: { message: string; offset: number }): string {
  const msg = err.message.toLowerCase();
  if (msg.includes('unexpected character')) {
    const ch = input[err.offset];
    if (ch === '"') return 'Unterminated string literal';
  }
  return err.message;
}

function classifyParserError(input: string, err: { message: string; token: { image: string; startOffset: number } }): string {
  const tok = err.token?.image ?? '';
  const offset = err.token?.startOffset ?? 0;
  const msg = err.message;

  // Double colon: state::
  if (tok === ':') return `Unexpected token ':'`;

  // Extra close paren
  if (tok === ')') return `Unexpected token ')'`;

  // Leading OR
  if (tok.toUpperCase() === 'OR') return `Unexpected 'OR': expected expression`;

  // Trailing AND / OR
  if (tok === '' && msg.includes('EOF')) {
    // Find the last non-whitespace token region
    const trimmed = input.trimEnd();
    const lastWord = trimmed.split(/\s+/).pop() ?? '';
    if (lastWord.toUpperCase() === 'AND') return `Expected expression after 'AND'`;
    if (lastWord.toUpperCase() === 'OR') return `Expected expression after 'OR'`;
    return `Unexpected end of input`;
  }

  // Unclosed paren
  if (msg.includes('expecting') && msg.includes(')')) return `Unclosed parenthesis: expected ')'`;

  // Range errors
  if (msg.includes('expecting') && (msg.includes(']') || msg.includes('RBracket'))) {
    return `Unterminated range: expected number or string then ']'`;
  }

  // Empty value after colon
  if (msg.includes('expecting') && offset > 0 && input[offset - 1] === ':') {
    return `Expected value after ':'`;
  }

  return msg || `Parse error at position ${offset}`;
}

export { VALID_FIELDS };
