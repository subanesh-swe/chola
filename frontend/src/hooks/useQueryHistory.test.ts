/**
 * Unit tests for useQueryHistory hook.
 *
 * Uses @testing-library/react renderHook + jsdom localStorage.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useQueryHistory } from './useQueryHistory';

const PAGE_KEY = 'test-page';
const STORAGE_KEY = `chola.queryHistory.${PAGE_KEY}`;

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('useQueryHistory — push', () => {
  it('adds a query to history', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push('branch:main'));
    expect(result.current.history).toEqual(['branch:main']);
  });

  it('deduplicates and moves existing entry to front (MRU)', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => {
      result.current.push('branch:main');
      result.current.push('state:failed');
      result.current.push('branch:main'); // re-push; should move to front
    });
    expect(result.current.history[0]).toBe('branch:main');
    expect(result.current.history).toHaveLength(2);
    expect(result.current.history[1]).toBe('state:failed');
  });

  it('ignores empty string', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push(''));
    expect(result.current.history).toHaveLength(0);
  });

  it('ignores whitespace-only string', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push('   '));
    expect(result.current.history).toHaveLength(0);
  });

  it('trims whitespace before storing', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push('  branch:main  '));
    expect(result.current.history[0]).toBe('branch:main');
  });

  it('prepends new entries (most recent first)', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => {
      result.current.push('first');
      result.current.push('second');
    });
    expect(result.current.history[0]).toBe('second');
    expect(result.current.history[1]).toBe('first');
  });
});

describe('useQueryHistory — max cap', () => {
  it('caps history at max entries', () => {
    const max = 5;
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY, max));
    act(() => {
      for (let i = 0; i < 8; i++) {
        result.current.push(`query-${i}`);
      }
    });
    expect(result.current.history).toHaveLength(max);
    // Most recent (query-7) should be first
    expect(result.current.history[0]).toBe('query-7');
  });

  it('default max is 20', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => {
      for (let i = 0; i < 25; i++) {
        result.current.push(`query-${i}`);
      }
    });
    expect(result.current.history).toHaveLength(20);
  });
});

describe('useQueryHistory — clear', () => {
  it('clears all entries', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => {
      result.current.push('a');
      result.current.push('b');
    });
    act(() => result.current.clear());
    expect(result.current.history).toHaveLength(0);
  });

  it('removes key from localStorage on clear', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push('a'));
    act(() => result.current.clear());
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });
});

describe('useQueryHistory — remove', () => {
  it('removes a specific entry', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => {
      result.current.push('a');
      result.current.push('b');
      result.current.push('c');
    });
    act(() => result.current.remove('b'));
    expect(result.current.history).toEqual(['c', 'a']);
  });

  it('removing non-existent entry is a no-op', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => {
      result.current.push('a');
      result.current.push('b');
    });
    act(() => result.current.remove('z'));
    expect(result.current.history).toHaveLength(2);
  });
});

describe('useQueryHistory — localStorage persistence', () => {
  it('persists to localStorage on push', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push('branch:main'));
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]');
    expect(stored).toEqual(['branch:main']);
  });

  it('hydrates from localStorage on mount', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(['pre-existing']));
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    expect(result.current.history).toEqual(['pre-existing']);
  });

  it('tolerates corrupt JSON — starts empty', () => {
    localStorage.setItem(STORAGE_KEY, 'NOT_VALID_JSON{{{');
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    expect(result.current.history).toEqual([]);
  });

  it('tolerates non-array JSON — starts empty', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ key: 'not-an-array' }));
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    expect(result.current.history).toEqual([]);
  });

  it('filters non-string entries from stored array', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(['good', 42, null, 'also-good']));
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    expect(result.current.history).toEqual(['good', 'also-good']);
  });
});

describe('useQueryHistory — cross-tab storage event', () => {
  it('updates state when storage event fires for the same key', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    // Simulate another tab writing to the same key
    act(() => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(['from-other-tab']));
      window.dispatchEvent(
        new StorageEvent('storage', {
          key: STORAGE_KEY,
          newValue: JSON.stringify(['from-other-tab']),
        }),
      );
    });
    expect(result.current.history).toEqual(['from-other-tab']);
  });

  it('ignores storage event for a different key', () => {
    const { result } = renderHook(() => useQueryHistory(PAGE_KEY));
    act(() => result.current.push('local'));
    act(() => {
      window.dispatchEvent(
        new StorageEvent('storage', {
          key: 'chola.queryHistory.other-page',
          newValue: JSON.stringify(['should-be-ignored']),
        }),
      );
    });
    expect(result.current.history).toEqual(['local']);
  });
});
