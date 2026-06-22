import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { DEFAULT_PRESET_ID, getPreset, type ThemePalette, type ThemeMode } from '../themes/presets';
import { DEFAULT_PATTERN_ID } from '../themes/patterns';

export type { ThemePalette, ThemeMode } from '../themes/presets';

/**
 * Snapshot of everything that defines a theme. Both the live (draft) state
 * and the persisted (saved) state use this shape so save() and reset() are
 * just copies in either direction.
 */
export interface ThemeSnapshot {
  presetId: string;
  palette: ThemePalette;
  mode: ThemeMode;
  patternId: string;
  patternOpacity: number;
}

interface ThemeState extends ThemeSnapshot {
  /**
   * Last-saved snapshot. ThemeProvider always renders from the live (top-level)
   * fields, so edits show in the UI immediately. Only the `saved` snapshot is
   * persisted to localStorage; clicking Save copies live -> saved.
   */
  saved: ThemeSnapshot;

  // Edits (mutate live state only — no persistence until save())
  applyPreset: (presetId: string) => void;
  setToken: (key: keyof ThemePalette, hex: string) => void;
  setPattern: (id: string) => void;
  setPatternOpacity: (n: number) => void;
  importPalette: (json: string) => boolean;
  exportPalette: () => string;

  // Persistence
  save: () => void;
  reset: () => void;
  isDirty: () => boolean;
  changedFields: () => ChangedField[];
}

export type ChangedField =
  | keyof ThemePalette
  | 'presetId'
  | 'patternId'
  | 'patternOpacity'
  | 'mode';

const defaultPreset = getPreset(DEFAULT_PRESET_ID);

function defaultSnapshot(): ThemeSnapshot {
  return {
    presetId: defaultPreset.id,
    palette: defaultPreset.palette,
    mode: defaultPreset.mode,
    patternId: DEFAULT_PATTERN_ID,
    patternOpacity: 1.0,
  };
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      ...defaultSnapshot(),
      saved: defaultSnapshot(),

      applyPreset: (presetId) => {
        const p = getPreset(presetId);
        set({ presetId: p.id, palette: p.palette, mode: p.mode });
      },

      setToken: (key, hex) => {
        const next = { ...get().palette, [key]: hex };
        set({ palette: next, presetId: 'custom' });
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

      save: () => {
        const s = get();
        set({
          saved: {
            presetId: s.presetId,
            palette: s.palette,
            mode: s.mode,
            patternId: s.patternId,
            patternOpacity: s.patternOpacity,
          },
        });
      },

      reset: () => {
        const { saved } = get();
        set({
          presetId: saved.presetId,
          palette: saved.palette,
          mode: saved.mode,
          patternId: saved.patternId,
          patternOpacity: saved.patternOpacity,
        });
      },

      isDirty: () => {
        const s = get();
        const a = s.saved;
        if (s.presetId !== a.presetId) return true;
        if (s.mode !== a.mode) return true;
        if (s.patternId !== a.patternId) return true;
        if (s.patternOpacity !== a.patternOpacity) return true;
        for (const k of Object.keys(s.palette) as (keyof ThemePalette)[]) {
          if (s.palette[k] !== a.palette[k]) return true;
        }
        return false;
      },

      changedFields: () => {
        const s = get();
        const a = s.saved;
        const out: ChangedField[] = [];
        if (s.presetId !== a.presetId) out.push('presetId');
        if (s.mode !== a.mode) out.push('mode');
        if (s.patternId !== a.patternId) out.push('patternId');
        if (s.patternOpacity !== a.patternOpacity) out.push('patternOpacity');
        for (const k of Object.keys(s.palette) as (keyof ThemePalette)[]) {
          if (s.palette[k] !== a.palette[k]) out.push(k);
        }
        return out;
      },
    }),
    {
      name: 'chola-theme-v2',
      // Persist ONLY the saved snapshot. The live draft is in-memory only.
      partialize: (state) => ({ saved: state.saved }),
      // After hydration, copy `saved` -> live fields so the UI renders the
      // last-saved theme on page load.
      onRehydrateStorage: () => (state) => {
        if (state?.saved) {
          state.presetId = state.saved.presetId;
          state.palette = state.saved.palette;
          state.mode = state.saved.mode;
          state.patternId = state.saved.patternId;
          state.patternOpacity = state.saved.patternOpacity;
        }
      },
      // Migrate from the v1 (mode+accent) and v2-flat (presetId+palette at top
      // level) shapes if present in localStorage.
      version: 3,
      migrate: (persistedState: unknown, version) => {
        if (version < 2 && typeof persistedState === 'object' && persistedState !== null) {
          // v1 -> v3: pick a preset based on old mode
          const old = persistedState as { mode?: string };
          const presetId = old.mode === 'light' ? 'snow' : 'onyx';
          const p = getPreset(presetId);
          const snap: ThemeSnapshot = {
            presetId: p.id,
            palette: p.palette,
            mode: p.mode,
            patternId: DEFAULT_PATTERN_ID,
            patternOpacity: 1.0,
          };
          return { saved: snap };
        }
        if (version === 2 && typeof persistedState === 'object' && persistedState !== null) {
          // v2 -> v3: flat shape becomes the `saved` snapshot
          const flat = persistedState as Partial<ThemeSnapshot>;
          const snap: ThemeSnapshot = {
            presetId: flat.presetId ?? DEFAULT_PRESET_ID,
            palette: flat.palette ?? defaultPreset.palette,
            mode: flat.mode ?? defaultPreset.mode,
            patternId: flat.patternId ?? DEFAULT_PATTERN_ID,
            patternOpacity: flat.patternOpacity ?? 1.0,
          };
          return { saved: snap };
        }
        return persistedState as ThemeState;
      },
    },
  ),
);
