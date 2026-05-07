import { useEffect } from 'react';
import { useThemeStore, type ThemeMode } from '../stores/theme';

function resolveColorScheme(mode: ThemeMode, mq: MediaQueryList): 'dark' | 'light' {
  if (mode === 'system') return mq.matches ? 'dark' : 'light';
  return mode;
}

export function ThemeProvider() {
  const mode = useThemeStore((s) => s.mode);
  const accent = useThemeStore((s) => s.accent);

  useEffect(() => {
    const html = document.documentElement;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');

    function apply() {
      html.setAttribute('data-theme', mode);
      html.setAttribute('data-accent', accent);

      // Keep Tailwind's built-in dark: variant working if anything uses it.
      const scheme = resolveColorScheme(mode, mq);
      if (scheme === 'dark') {
        html.classList.add('dark');
        html.classList.remove('light');
      } else {
        html.classList.add('light');
        html.classList.remove('dark');
      }
    }

    apply();

    // Re-apply when system preference changes while mode === 'system'
    const handler = () => { if (mode === 'system') apply(); };
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [mode, accent]);

  return null;
}
