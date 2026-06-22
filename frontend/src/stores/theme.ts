import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { DEFAULT_PRESET_ID, getPreset, type ThemePalette, type ThemeMode } from '../themes/presets';
import { DEFAULT_PATTERN_ID } from '../themes/patterns';

export type { ThemePalette, ThemeMode } from '../themes/presets';

interface ThemeState {
  /** ID of the preset the user picked (or 'custom' if they edited any token). */
  presetId: string;
  /** Resolved palette — what actually drives CSS variables. */
  palette: ThemePalette;
  /** Whether the current palette is light or dark — for Tailwind's `dark:` variant. */
  mode: ThemeMode;
  /** Background pattern id (see themes/patterns.ts). 'none' = solid bg. */
  patternId: string;
  /** Pattern intensity 0…1 (multiplied into pattern's base opacity). */
  patternOpacity: number;

  applyPreset: (presetId: string) => void;
  setToken: (key: keyof ThemePalette, hex: string) => void;
  resetToCurrentPreset: () => void;
  setPattern: (id: string) => void;
  setPatternOpacity: (n: number) => void;
  importPalette: (json: string) => boolean;
  exportPalette: () => string;
}

const defaultPreset = getPreset(DEFAULT_PRESET_ID);

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      presetId: defaultPreset.id,
      palette: defaultPreset.palette,
      mode: defaultPreset.mode,
      patternId: DEFAULT_PATTERN_ID,
      patternOpacity: 1.0,

      applyPreset: (presetId) => {
        const p = getPreset(presetId);
        set({ presetId: p.id, palette: p.palette, mode: p.mode });
      },

      setToken: (key, hex) => {
        const next = { ...get().palette, [key]: hex };
        set({ palette: next, presetId: 'custom' });
      },

      resetToCurrentPreset: () => {
        const id = get().presetId === 'custom' ? DEFAULT_PRESET_ID : get().presetId;
        const p = getPreset(id);
        set({ presetId: p.id, palette: p.palette, mode: p.mode });
      },

      setPattern: (id) => set({ patternId: id }),
      setPatternOpacity: (n) => set({ patternOpacity: Math.max(0, Math.min(1, n)) }),

      importPalette: (json) => {
        try {
          const parsed = JSON.parse(json) as Partial<ThemePalette> & { mode?: ThemeMode };
          const cur = get().palette;
          const merged: ThemePalette = { ...cur };
          let touched = false;
          for (const k of Object.keys(merged) as (keyof ThemePalette)[]) {
            const v = parsed[k];
            if (typeof v === 'string' && /^#[0-9a-fA-F]{3,8}$/.test(v)) {
              merged[k] = v;
              touched = true;
            }
          }
          if (!touched) return false;
          set({
            palette: merged,
            presetId: 'custom',
            mode: parsed.mode ?? get().mode,
          });
          return true;
        } catch {
          return false;
        }
      },

      exportPalette: () => {
        const { palette, mode } = get();
        return JSON.stringify({ mode, ...palette }, null, 2);
      },
    }),
    {
      name: 'chola-theme-v2',
      partialize: (state) => ({
        presetId: state.presetId,
        palette: state.palette,
        mode: state.mode,
        patternId: state.patternId,
        patternOpacity: state.patternOpacity,
      }),
      // Migrate from the v1 mode+accent shape if present.
      version: 2,
      migrate: (persistedState: unknown, version) => {
        if (version < 2 && typeof persistedState === 'object' && persistedState !== null) {
          const old = persistedState as { mode?: string; accent?: string };
          // Old "dark" → onyx, "light" → snow, anything else → onyx.
          const presetId = old.mode === 'light' ? 'snow' : 'onyx';
          const p = getPreset(presetId);
          return { presetId: p.id, palette: p.palette, mode: p.mode };
        }
        return persistedState as ThemeState;
      },
    },
  ),
);
