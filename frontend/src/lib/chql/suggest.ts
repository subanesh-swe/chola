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

const KEYWORDS: Suggestion[] = [
  { label: 'AND', insertText: 'AND ', kind: 'keyword', detail: 'logical and' },
  { label: 'OR', insertText: 'OR ', kind: 'keyword', detail: 'logical or' },
  { label: 'NOT', insertText: 'NOT ', kind: 'keyword', detail: 'logical not' },
];

const OPERATOR_SUGGESTIONS: Suggestion[] = [
  { label: '>=', insertText: '>=', kind: 'operator', detail: 'greater than or equal' },
  { label: '<=', insertText: '<=', kind: 'operator', detail: 'less than or equal' },
  { label: '>', insertText: '>', kind: 'operator', detail: 'greater than' },
  { label: '<', insertText: '<', kind: 'operator', detail: 'less than' },
  { label: '!', insertText: '!', kind: 'operator', detail: 'not equal' },
  { label: 'i(', insertText: 'i("', kind: 'operator', detail: 'case-insensitive contains' },
  { label: 're(', insertText: 're("', kind: 'operator', detail: 'regex match' },
];

const MAX_SUGGESTIONS = 20;

// ── Main function ─────────────────────────────────────────────────────────────

export async function suggestAt(
  input: string,
  cursorPos: number,
  fieldValues: FieldValueProvider,
): Promise<Suggestion[]> {
  const prefix = input.slice(0, cursorPos);

  // Use Chevrotain content assist.
  const lexResult = ChqlLexer.tokenize(prefix);
  const parser = getParser();

  // computeContentAssist requires a fresh tokenization up to cursor.
  const contentAssist = parser.computeContentAssist('query', lexResult.tokens);

  // Determine context: what is the last "complete" token before cursor?
  const lastToken = lexResult.tokens[lexResult.tokens.length - 1];
  const lastImage = lastToken?.image ?? '';
  const lastType = lastToken?.tokenType?.name ?? '';

  // If we're typing after a colon, suggest value operators + values.
  // Check if last complete token is ':'.
  const isAfterColon = lastType === 'Colon';

  // Check if we're mid-identifier (partial field name).
  // The cursor is not preceded by space, so we're extending an identifier.
  const isAtFieldStart = !isAfterColon && (prefix.length === 0 || /\s$/.test(prefix) || prefix.endsWith('('));

  const nextTokenNames = new Set(contentAssist.map((a) => a.nextTokenType.name));

  // ── Gather suggestions ───────────────────────────────────────────────────

  const results: Suggestion[] = [];

  // Field suggestions
  if (nextTokenNames.has('Identifier') || isAtFieldStart) {
    const partialField = isAtFieldStart ? '' : lastImage;
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

  // Keyword suggestions (AND, OR, NOT)
  if (nextTokenNames.has('And') || nextTokenNames.has('Or') || nextTokenNames.has('Not')) {
    results.push(...KEYWORDS.filter((k) => nextTokenNames.has(k.label)));
  }

  // Value-level suggestions (after a colon)
  if (isAfterColon) {
    results.push(...OPERATOR_SUGGESTIONS);

    // Try to infer the field from the token before the colon.
    const tokens = lexResult.tokens;
    const colonIdx = tokens.length - 1;
    if (colonIdx > 0 && tokens[colonIdx - 1]?.tokenType?.name === 'Identifier') {
      const fieldName = tokens[colonIdx - 1].image as ValidField;
      if ((VALID_FIELDS as readonly string[]).includes(fieldName)) {
        const values = await fieldValues(fieldName);
        const valueSuggestions = values.slice(0, MAX_SUGGESTIONS - results.length).map((v): Suggestion => ({
          label: v,
          insertText: v.includes(' ') ? `"${v}"` : v,
          kind: 'value',
          detail: fieldName,
        }));
        results.push(...valueSuggestions);
      }
    }
  }

  return results.slice(0, MAX_SUGGESTIONS);
}
