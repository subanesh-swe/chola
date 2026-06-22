import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type ThemeMode = 'light' | 'dark' | 'system';
export type ThemeAccent =
  | 'indigo'
  | 'emerald'
  | 'rose'
  | 'sky'
  | 'violet'
  | 'amber'
  | 'teal'
  | 'fuchsia'
  | 'slate';

interface ThemeState {
  mode: ThemeMode;
  accent: ThemeAccent;
  setMode: (mode: ThemeMode) => void;
  setAccent: (accent: ThemeAccent) => void;
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      mode: 'dark',
      accent: 'indigo',
      setMode: (mode) => set({ mode }),
      setAccent: (accent) => set({ accent }),
    }),
    {
      name: 'chola-theme',
      partialize: (state) => ({ mode: state.mode, accent: state.accent }),
    },
  ),
);
