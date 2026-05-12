import { useCallback, useEffect, useState } from 'react';

function storageKey(pageKey: string): string {
  return `chola.queryHistory.${pageKey}`;
}

function readFromStorage(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((v): v is string => typeof v === 'string');
  } catch {
    return [];
  }
}

function writeToStorage(key: string, history: string[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(history));
  } catch {
    // Quota exceeded or storage unavailable — silently ignore.
  }
}

export function useQueryHistory(
  pageKey: string,
  max = 20,
): {
  history: string[];
  push: (q: string) => void;
  clear: () => void;
  remove: (q: string) => void;
} {
  const key = storageKey(pageKey);

  const [history, setHistory] = useState<string[]>(() => readFromStorage(key));

  // Sync across tabs via storage events.
  useEffect(() => {
    const handler = (e: StorageEvent) => {
      if (e.key !== key) return;
      setHistory(readFromStorage(key));
    };
    window.addEventListener('storage', handler);
    return () => window.removeEventListener('storage', handler);
  }, [key]);

  const push = useCallback(
    (q: string) => {
      const trimmed = q.trim();
      if (!trimmed) return;
      setHistory((prev) => {
        const without = prev.filter((item) => item !== trimmed);
        const next = [trimmed, ...without].slice(0, max);
        writeToStorage(key, next);
        return next;
      });
    },
    [key, max],
  );

  const clear = useCallback(() => {
    setHistory([]);
    try {
      localStorage.removeItem(key);
    } catch {
      // ignore
    }
  }, [key]);

  const remove = useCallback(
    (q: string) => {
      setHistory((prev) => {
        const next = prev.filter((item) => item !== q);
        writeToStorage(key, next);
        return next;
      });
    },
    [key],
  );

  return { history, push, clear, remove };
}
