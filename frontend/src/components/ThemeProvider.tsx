import { useEffect } from 'react';
import { useThemeStore } from '../stores/theme';
import { CSS_VAR_MAP, type ThemePalette } from '../themes/presets';
import { getPattern } from '../themes/patterns';

export function ThemeProvider() {
  const palette = useThemeStore((s) => s.palette);
  const mode = useThemeStore((s) => s.mode);
  const patternId = useThemeStore((s) => s.patternId);
  const patternOpacity = useThemeStore((s) => s.patternOpacity);

  useEffect(() => {
    const html = document.documentElement;

    // Write each palette token as an inline CSS variable on <html>.
    for (const [field, varName] of Object.entries(CSS_VAR_MAP)) {
      html.style.setProperty(varName, palette[field as keyof ThemePalette]);
    }

    // Pattern — write image, size and resolved opacity.
    const pat = getPattern(patternId);
    html.style.setProperty('--pattern-image', pat.image);
    html.style.setProperty('--pattern-size', pat.size);
    html.style.setProperty('--pattern-opacity', String(pat.opacity * patternOpacity));

    // Keep Tailwind's `dark:` variant working.
    html.setAttribute('data-theme', mode);
    if (mode === 'dark') {
      html.classList.add('dark');
      html.classList.remove('light');
    } else {
      html.classList.add('light');
      html.classList.remove('dark');
    }
  }, [palette, mode, patternId, patternOpacity]);

  return null;
}
