// @vitest-environment node
import { describe, it, expect } from 'vitest';
import { suggestAt } from './suggest';
import type { FieldValueProvider, Suggestion } from './suggest';

// Stub field-value provider used across all tests.
const STATE_VALUES = ['queued', 'reserved', 'running', 'success', 'failed', 'cancelled', 'expired'];
const BRANCH_VALUES = ['main', 'dev', 'feature/foo'];
const REPO_VALUES = ['acme/api', 'acme/web'];
const STAGE_VALUES = ['build', 'test', 'deploy'];

const fieldValues: FieldValueProvider = (field) => {
  switch (field) {
    case 'state':      return STATE_VALUES;
    case 'branch':     return BRANCH_VALUES;
    case 'repo':       return REPO_VALUES;
    case 'stage':      return STAGE_VALUES;
    case 'exit_code':  return [];
    default:           return [];
  }
};

// Helpers
function labels(suggestions: Suggestion[]): string[] {
  return suggestions.map((s) => s.label);
}

describe('suggestAt', () => {
  it('empty input → fields appear, no values', async () => {
    const s = await suggestAt('', 0, fieldValues);
    const l = labels(s);
    expect(l).toContain('state');
    expect(l).toContain('branch');
    expect(l).toContain('exit_code');
    // No values in results
    expect(s.every((x) => x.kind !== 'value')).toBe(true);
  });

  it('"sta" cursor 3 → field "state" appears (prefix match)', async () => {
    const s = await suggestAt('sta', 3, fieldValues);
    const l = labels(s);
    expect(l).toContain('state');
    // Should not contain unrelated fields
    expect(l).not.toContain('branch');
    expect(l).not.toContain('exit_code');
  });

  it('"state:" cursor 6 → state enum values appear', async () => {
    const s = await suggestAt('state:', 6, fieldValues);
    const l = labels(s);
    for (const v of STATE_VALUES) {
      expect(l).toContain(v);
    }
    // No field suggestions
    expect(s.every((x) => x.kind !== 'field')).toBe(true);
  });

  it('"state:fa" cursor 8 → only state values starting with "fa"', async () => {
    const s = await suggestAt('state:fa', 8, fieldValues);
    const l = labels(s);
    expect(l).toContain('failed');
    expect(l).not.toContain('queued');
    expect(l).not.toContain('running');
    // No field suggestions
    expect(s.every((x) => x.kind !== 'field')).toBe(true);
  });

  it('"state:failed " cursor 13 → AND, OR, NOT keywords appear', async () => {
    const s = await suggestAt('state:failed ', 13, fieldValues);
    const l = labels(s);
    expect(l).toContain('AND');
    expect(l).toContain('OR');
    expect(l).toContain('NOT');
  });

  it('"state:failed AND " cursor 17 → fields appear', async () => {
    const s = await suggestAt('state:failed AND ', 17, fieldValues);
    const l = labels(s);
    expect(l).toContain('state');
    expect(l).toContain('branch');
  });

  it('"exit_code:" cursor 10 → numeric operators appear', async () => {
    const s = await suggestAt('exit_code:', 10, fieldValues);
    const l = labels(s);
    expect(l).toContain('!=');
    expect(l).toContain('>=');
    expect(l).toContain('<=');
    expect(l).toContain('>');
    expect(l).toContain('<');
    // All are operators
    const ops = s.filter((x) => x.kind === 'operator');
    expect(ops.length).toBeGreaterThan(0);
  });

  it('"branch:" cursor 7 → i("…"), re("…"), ! operators + branch values', async () => {
    const s = await suggestAt('branch:', 7, fieldValues);
    const l = labels(s);
    expect(l).toContain('i("…")');
    expect(l).toContain('re("…")');
    expect(l).toContain('!');
    // Also contains actual branch values
    expect(l).toContain('main');
    expect(l).toContain('dev');
  });

  it('insertText for field ends with ":"', async () => {
    const s = await suggestAt('', 0, fieldValues);
    const stateSug = s.find((x) => x.label === 'state');
    expect(stateSug?.insertText).toBe('state:');
    expect(stateSug?.kind).toBe('field');
  });

  it('"state:fa" → failed insertText is "failed" (no quotes needed)', async () => {
    const s = await suggestAt('state:fa', 8, fieldValues);
    const failedSug = s.find((x) => x.label === 'failed');
    expect(failedSug?.insertText).toBe('failed');
  });
});
