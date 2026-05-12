import { describe, it, expect } from 'vitest';
import { parseQuery } from './parseQuery';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function ok(input: string) {
  const r = parseQuery(input);
  if (!r.ok) throw new Error(`Expected ok but got error: ${r.error.message}`);
  return r;
}

function err(input: string) {
  const r = parseQuery(input);
  if (r.ok) throw new Error(`Expected error but got ok with filters: ${JSON.stringify(r.filters)}`);
  return r.error;
}

// A valid UUID used for repo tests.
const VALID_UUID = 'a1b2c3d4-1234-5678-9abc-def012345678';

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------

describe('empty input', () => {
  it('returns ok with empty filters and no warnings', () => {
    const r = ok('');
    expect(r.filters).toEqual({});
    expect(r.warnings).toHaveLength(0);
  });

  it('whitespace-only is treated as empty', () => {
    const r = ok('   ');
    expect(r.filters).toEqual({});
    expect(r.warnings).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Single field: branch
// ---------------------------------------------------------------------------

describe('branch field', () => {
  it('bare value', () => {
    const r = ok('branch:main');
    expect(r.filters.branch).toBe('main');
    expect(r.warnings).toHaveLength(0);
  });

  it('quoted value with space', () => {
    const r = ok('branch:"feat/foo bar"');
    expect(r.filters.branch).toBe('feat/foo bar');
    expect(r.warnings).toHaveLength(0);
  });

  it('quoted value with slash', () => {
    const r = ok('branch:"release/1.2.3"');
    expect(r.filters.branch).toBe('release/1.2.3');
  });

  it('trailing wildcard emits warning, strips *', () => {
    const r = ok('branch:feat-*');
    expect(r.filters.branch).toBe('feat-');
    expect(r.warnings).toHaveLength(1);
    expect(r.warnings[0].message).toMatch(/Wildcard/i);
  });

  it('leading wildcard emits warning, strips *', () => {
    const r = ok('branch:*-hotfix');
    expect(r.filters.branch).toBe('-hotfix');
    expect(r.warnings).toHaveLength(1);
  });

  it('multiple wildcards all stripped', () => {
    const r = ok('branch:feat-*-fix-*');
    expect(r.filters.branch).toBe('feat--fix-');
    expect(r.warnings).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Single field: stage
// ---------------------------------------------------------------------------

describe('stage field', () => {
  it('bare value', () => {
    const r = ok('stage:lint');
    expect(r.filters.stage).toBe('lint');
    expect(r.warnings).toHaveLength(0);
  });

  it('wildcard emits warning', () => {
    const r = ok('stage:build*');
    expect(r.filters.stage).toBe('build');
    expect(r.warnings).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Single field: state
// ---------------------------------------------------------------------------

describe('state field', () => {
  it('failed', () => {
    const r = ok('state:failed');
    expect(r.filters.state).toEqual(['failed']);
  });

  it('multiple state terms accumulate', () => {
    const r = ok('state:failed state:running');
    expect(r.filters.state).toEqual(['failed', 'running']);
  });

  it('duplicate state terms are deduplicated', () => {
    const r = ok('state:failed state:failed');
    expect(r.filters.state).toEqual(['failed']);
  });

  it('pass-through unknown state value (backend validates)', () => {
    const r = ok('state:custom_state');
    expect(r.filters.state).toEqual(['custom_state']);
  });
});

// ---------------------------------------------------------------------------
// Single field: exit_code
// ---------------------------------------------------------------------------

describe('exit_code field', () => {
  it('exit_code:0 -> "0"', () => {
    const r = ok('exit_code:0');
    expect(r.filters.exitCode).toBe('0');
  });

  it('exit_code:1 -> "nonzero"', () => {
    const r = ok('exit_code:1');
    expect(r.filters.exitCode).toBe('nonzero');
  });

  it('exit_code:!=0 -> "nonzero"', () => {
    const r = ok('exit_code:!=0');
    expect(r.filters.exitCode).toBe('nonzero');
  });

  it('exit_code:>=1 -> "nonzero"', () => {
    const r = ok('exit_code:>=1');
    expect(r.filters.exitCode).toBe('nonzero');
  });

  it('exit_code:<5 -> "nonzero"', () => {
    const r = ok('exit_code:<5');
    expect(r.filters.exitCode).toBe('nonzero');
  });

  it('exit_code:<=255 -> "nonzero"', () => {
    const r = ok('exit_code:<=255');
    expect(r.filters.exitCode).toBe('nonzero');
  });

  it('exit_code:>0 -> "nonzero"', () => {
    const r = ok('exit_code:>0');
    expect(r.filters.exitCode).toBe('nonzero');
  });

  it('non-integer exit_code returns error', () => {
    const e = err('exit_code:abc');
    expect(e.message).toMatch(/not an integer/i);
  });

  it('float exit_code returns error', () => {
    const e = err('exit_code:1.5');
    expect(e.message).toMatch(/not an integer/i);
  });
});

// ---------------------------------------------------------------------------
// Single field: from / to (date)
// ---------------------------------------------------------------------------

describe('date fields', () => {
  it('from YYYY-MM-DD', () => {
    const r = ok('from:2026-04-12');
    expect(r.filters.dateFrom).toBe('2026-04-12');
  });

  it('to YYYY-MM-DD', () => {
    const r = ok('to:2026-05-12');
    expect(r.filters.dateTo).toBe('2026-05-12');
  });

  it('from YYYY-MM-DDTHH:mm', () => {
    const r = ok('from:2026-04-12T08:30');
    expect(r.filters.dateFrom).toBe('2026-04-12T08:30');
  });

  it('to RFC3339', () => {
    const r = ok('to:2026-05-12T23:59:59Z');
    expect(r.filters.dateTo).toBe('2026-05-12T23:59:59Z');
  });
});

// ---------------------------------------------------------------------------
// repo field
// ---------------------------------------------------------------------------

describe('repo field', () => {
  it('valid UUID -> repo filter', () => {
    const r = ok(`repo:${VALID_UUID}`);
    expect(r.filters.repo).toBe(VALID_UUID);
    expect(r.warnings).toHaveLength(0);
  });

  it('friendly name emits warning and is ignored', () => {
    const r = ok('repo:my-cool-project');
    expect(r.filters.repo).toBeUndefined();
    expect(r.warnings).toHaveLength(1);
    expect(r.warnings[0].message).toMatch(/not a UUID/i);
  });
});

// ---------------------------------------------------------------------------
// Multiple terms (combined)
// ---------------------------------------------------------------------------

describe('multiple terms', () => {
  it('branch + state', () => {
    const r = ok('branch:main state:failed');
    expect(r.filters.branch).toBe('main');
    expect(r.filters.state).toEqual(['failed']);
  });

  it('repo UUID + state + stage', () => {
    const r = ok(`repo:${VALID_UUID} state:running stage:lint`);
    expect(r.filters.repo).toBe(VALID_UUID);
    expect(r.filters.state).toEqual(['running']);
    expect(r.filters.stage).toBe('lint');
  });

  it('full coverage: branch + from + to + exit_code', () => {
    const r = ok('branch:main from:2026-04-12 to:2026-05-12 exit_code:!=0');
    expect(r.filters.branch).toBe('main');
    expect(r.filters.dateFrom).toBe('2026-04-12');
    expect(r.filters.dateTo).toBe('2026-05-12');
    expect(r.filters.exitCode).toBe('nonzero');
    expect(r.warnings).toHaveLength(0);
  });

  it('multiple quoted terms with spaces', () => {
    const r = ok('branch:"feat/my branch" stage:"my stage"');
    expect(r.filters.branch).toBe('feat/my branch');
    expect(r.filters.stage).toBe('my stage');
  });

  it('mixed warnings and valid terms', () => {
    const r = ok('branch:feat-* state:failed');
    expect(r.filters.branch).toBe('feat-');
    expect(r.filters.state).toEqual(['failed']);
    expect(r.warnings).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

describe('errors', () => {
  it('unknown field', () => {
    const e = err('xyz:foo');
    expect(e.message).toMatch(/Unknown field "xyz"/);
    expect(e.hint).toMatch(/branch/);
  });

  it('unknown field lists all known fields in hint', () => {
    const e = err('foobar:baz');
    expect(e.hint).toContain('exit_code');
    expect(e.hint).toContain('from');
    expect(e.hint).toContain('to');
  });

  it('bare word with no colon', () => {
    const e = err('branch');
    expect(e.message).toMatch(/Expected.*field:value/i);
  });

  it('multiple terms, second is unknown', () => {
    const e = err('branch:main unknown:field');
    expect(e.message).toMatch(/Unknown field "unknown"/);
  });

  it('unterminated quoted string', () => {
    const e = err('branch:"unclosed');
    expect(e.message).toMatch(/Unterminated/i);
  });

  it('empty value after colon', () => {
    const e = err('branch:');
    expect(e.message).toMatch(/Empty value/i);
  });

  it('unknown field hint now lists bucket', () => {
    const e = err('bogus:val');
    expect(e.hint).toContain('bucket');
  });
});

// ---------------------------------------------------------------------------
// bucket field (granularity)
// ---------------------------------------------------------------------------

describe('bucket field', () => {
  it('bucket:hour -> granularity hour', () => {
    const r = ok('bucket:hour');
    expect(r.filters.granularity).toBe('hour');
    expect(r.warnings).toHaveLength(0);
  });

  it('bucket:day -> granularity day', () => {
    const r = ok('bucket:day');
    expect(r.filters.granularity).toBe('day');
  });

  it('bucket:auto -> granularity auto', () => {
    const r = ok('bucket:auto');
    expect(r.filters.granularity).toBe('auto');
  });

  it('bucket:invalid returns ParseResult error', () => {
    const e = err('bucket:invalid');
    expect(e.message).toMatch(/Invalid bucket value "invalid"/);
    expect(e.hint).toMatch(/auto.*hour.*day|hour.*day.*auto/);
  });

  it('bucket:HOUR (uppercase) returns error (values are case-sensitive)', () => {
    const e = err('bucket:HOUR');
    expect(e.message).toMatch(/Invalid bucket value/i);
  });

  it('combined: state:failed bucket:day', () => {
    const r = ok('state:failed bucket:day');
    expect(r.filters.state).toEqual(['failed']);
    expect(r.filters.granularity).toBe('day');
    expect(r.warnings).toHaveLength(0);
  });

  it('bucket works with other terms', () => {
    const r = ok('branch:main bucket:auto state:running');
    expect(r.filters.branch).toBe('main');
    expect(r.filters.granularity).toBe('auto');
    expect(r.filters.state).toEqual(['running']);
  });
});
