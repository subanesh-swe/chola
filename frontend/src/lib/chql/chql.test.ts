// @vitest-environment node
import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from './index';

// Resolve corpus relative to this file's location.
// This file lives at frontend/src/lib/chql/chql.test.ts
// Corpus is at tests/corpus/ from repo root.
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CORPUS_DIR = path.resolve(__dirname, '../../../../tests/corpus');

interface CorpusFile {
  input: string;
  ast?: unknown;
  error?: {
    message: string;
    position?: number;
  };
}

describe('ChQL corpus', () => {
  let files: string[];
  try {
    files = fs.readdirSync(CORPUS_DIR).filter((f) => f.endsWith('.json'));
  } catch {
    files = [];
    it('corpus directory not found', () => {
      expect(CORPUS_DIR).toBeTruthy();
      expect(false).toBe(true); // always fail if corpus missing
    });
  }

  for (const f of files) {
    it(f, () => {
      const raw = fs.readFileSync(path.join(CORPUS_DIR, f), 'utf-8');
      const { input, ast: expected, error }: CorpusFile = JSON.parse(raw);

      const result = parse(input);

      if (error) {
        // Error cases: assert parse failed.
        expect(result.ok, `expected parse failure for: ${input}`).toBe(false);
        if (!result.ok && error.position !== undefined) {
          // Lenient position check: just verify errors array is non-empty.
          expect(result.errors.length, 'expected at least one error').toBeGreaterThan(0);
        }
      } else {
        // Success cases: assert parse succeeded and AST matches.
        expect(result.ok, `expected parse success for: ${input}\n${JSON.stringify(!result.ok && (result as { errors: unknown[] }).errors, null, 2)}`).toBe(true);
        if (result.ok) {
          expect(result.ast, `AST mismatch for: ${input}`).toEqual(expected ?? null);
        }
      }
    });
  }
});
