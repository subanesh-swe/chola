import {
  createToken,
  Lexer,
  type TokenType,
} from 'chevrotain';

// ── Whitespace (skipped) ─────────────────────────────────────────────────────
export const WhiteSpace = createToken({
  name: 'WhiteSpace',
  pattern: /\s+/,
  group: Lexer.SKIPPED,
});

// ── Keywords — must be listed BEFORE Identifier so LONGER_ALT works ─────────
// Case-insensitive using pattern functions.

export const And = createToken({
  name: 'And',
  pattern: /AND|and/,
  longer_alt: undefined as unknown as TokenType, // set below
});

export const Or = createToken({
  name: 'Or',
  pattern: /OR|or/,
  longer_alt: undefined as unknown as TokenType,
});

export const Not = createToken({
  name: 'Not',
  pattern: /NOT|not/,
  longer_alt: undefined as unknown as TokenType,
});

// ── Identifier (field names + bare values) ───────────────────────────────────
// Must match broader set than keywords; includes UUID chars (hex + hyphens)
// and path chars (slash, dot, plus) for repo URLs and branch names as bare values.
export const Identifier = createToken({
  name: 'Identifier',
  pattern: /[A-Za-z_][A-Za-z0-9_\-.\/+]*/,
});

// Disambiguate: AND/OR/NOT that appear inside a longer identifier are Identifiers.
// Chevrotain LONGER_ALT: if Identifier matches a longer string starting at the same pos,
// prefer Identifier.
And.LONGER_ALT = Identifier;
Or.LONGER_ALT = Identifier;
Not.LONGER_ALT = Identifier;

// ── Numeric literal (signed int or float) ────────────────────────────────────
export const NumberLiteral = createToken({
  name: 'NumberLiteral',
  pattern: /-?\d+(?:\.\d+)?/,
});

// ── String literal (double-quoted, backslash-escaped) ────────────────────────
export const StringLiteral = createToken({
  name: 'StringLiteral',
  // Matches "..." with backslash escapes — does NOT handle unterminated strings
  // (those will be rejected by the lexer, surfaced as an error).
  pattern: /"(?:[^"\\]|\\.)*"/,
});

// ── Operators ────────────────────────────────────────────────────────────────
export const BangEq = createToken({ name: 'BangEq', pattern: /!=/ });
export const Gte = createToken({ name: 'Gte', pattern: />=/ });
export const Lte = createToken({ name: 'Lte', pattern: /<=/ });
export const Gt = createToken({ name: 'Gt', pattern: />/ });
export const Lt = createToken({ name: 'Lt', pattern: /</ });
export const Bang = createToken({ name: 'Bang', pattern: /!/ });

// ── Punctuation ──────────────────────────────────────────────────────────────
export const Colon = createToken({ name: 'Colon', pattern: /:/ });
export const Comma = createToken({ name: 'Comma', pattern: /,/ });
export const LBracket = createToken({ name: 'LBracket', pattern: /\[/ });
export const RBracket = createToken({ name: 'RBracket', pattern: /]/ });
export const LParen = createToken({ name: 'LParen', pattern: /\(/ });
export const RParen = createToken({ name: 'RParen', pattern: /\)/ });

// ── Token list ── ORDER MATTERS for chevrotain lexer ────────────────────────
// Multi-char operators before single-char, keywords before Identifier.
export const ALL_TOKENS: TokenType[] = [
  WhiteSpace,
  StringLiteral,
  BangEq,
  Gte,
  Lte,
  Gt,
  Lt,
  Bang,
  And,
  Or,
  Not,
  Identifier,
  NumberLiteral,
  Colon,
  Comma,
  LBracket,
  RBracket,
  LParen,
  RParen,
];

export const ChqlLexer = new Lexer(ALL_TOKENS, {
  // Don't throw on unrecognized chars; let parser handle errors.
  recoveryEnabled: true,
});
