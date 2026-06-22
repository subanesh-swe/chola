/**
 * Theme palette — 17 independent colors per theme.
 *
 * Each preset is a hand-tuned coordinated palette. Users can start from a
 * preset and override individual tokens, or build a theme from scratch.
 *
 * Token layout maps 1:1 to the CSS variables in index.css:
 *   appBg          -> --color-app
 *   chromeBg       -> --color-chrome
 *   surfaceBg      -> --color-surface
 *   surface2Bg     -> --color-surface-2
 *   inputBg        -> --color-input
 *   borderColor    -> --color-border
 *   primaryText    -> --color-text-primary
 *   secondaryText  -> --color-text-secondary
 *   mutedText      -> --color-text-muted
 *   disabledText   -> --color-text-disabled
 *   accent         -> --color-accent
 *   accentText     -> --color-accent-text
 *   onAccent       -> --color-text-on-accent
 *   success        -> --color-success
 *   warning        -> --color-warning
 *   danger         -> --color-danger
 *   info           -> --color-info
 */

export interface ThemePalette {
  appBg: string;
  chromeBg: string;
  surfaceBg: string;
  surface2Bg: string;
  inputBg: string;
  borderColor: string;
  primaryText: string;
  secondaryText: string;
  mutedText: string;
  disabledText: string;
  accent: string;
  accentText: string;
  onAccent: string;
  success: string;
  warning: string;
  danger: string;
  info: string;
}

export type ThemeMode = 'light' | 'dark';

export interface ThemePreset {
  id: string;
  name: string;
  mode: ThemeMode;
  palette: ThemePalette;
}

/** All 17 token field names, in the order shown in the picker UI. */
export const PALETTE_FIELDS: { key: keyof ThemePalette; label: string; group: string }[] = [
  { key: 'appBg',         label: 'App background',  group: 'Surfaces' },
  { key: 'chromeBg',      label: 'Sidebar / header', group: 'Surfaces' },
  { key: 'surfaceBg',     label: 'Card',             group: 'Surfaces' },
  { key: 'surface2Bg',    label: 'Card elevated',    group: 'Surfaces' },
  { key: 'inputBg',       label: 'Input',            group: 'Surfaces' },
  { key: 'borderColor',   label: 'Border',           group: 'Surfaces' },

  { key: 'primaryText',   label: 'Primary text',     group: 'Text' },
  { key: 'secondaryText', label: 'Secondary text',   group: 'Text' },
  { key: 'mutedText',     label: 'Muted text',       group: 'Text' },
  { key: 'disabledText',  label: 'Disabled text',    group: 'Text' },

  { key: 'accent',        label: 'Accent',           group: 'Accent' },
  { key: 'accentText',    label: 'Accent text/link', group: 'Accent' },
  { key: 'onAccent',      label: 'Text on accent',   group: 'Accent' },

  { key: 'success',       label: 'Success',          group: 'State' },
  { key: 'warning',       label: 'Warning',          group: 'State' },
  { key: 'danger',        label: 'Danger',           group: 'State' },
  { key: 'info',          label: 'Info',             group: 'State' },
];

/** Curated presets — each is a real palette from a popular theme. */
export const PRESETS: ThemePreset[] = [
  {
    id: 'onyx',
    name: 'Onyx',
    mode: 'dark',
    palette: {
      appBg:         '#000000',
      chromeBg:      '#1c1c1e',
      surfaceBg:     '#1c1c1e',
      surface2Bg:    '#2c2c2e',
      inputBg:       '#1c1c1e',
      borderColor:   '#38383a',
      primaryText:   '#ffffff',
      secondaryText: '#ebebf5',
      mutedText:     '#8e8e93',
      disabledText:  '#48484a',
      accent:        '#0a84ff',
      accentText:    '#0a84ff',
      onAccent:      '#ffffff',
      success:       '#30d158',
      warning:       '#ff9f0a',
      danger:        '#ff453a',
      info:          '#64d2ff',
    },
  },
  {
    id: 'snow',
    name: 'Snow',
    mode: 'light',
    palette: {
      appBg:         '#f2f2f7',
      chromeBg:      '#ffffff',
      surfaceBg:     '#ffffff',
      surface2Bg:    '#f2f2f7',
      inputBg:       '#ffffff',
      borderColor:   '#d1d1d6',
      primaryText:   '#000000',
      secondaryText: '#3c3c43',
      mutedText:     '#8e8e93',
      disabledText:  '#c7c7cc',
      accent:        '#007aff',
      accentText:    '#007aff',
      onAccent:      '#ffffff',
      success:       '#34c759',
      warning:       '#ff9500',
      danger:        '#ff3b30',
      info:          '#5ac8fa',
    },
  },
  {
    id: 'midnight',
    name: 'Midnight',
    mode: 'dark',
    palette: {
      appBg:         '#17212b',
      chromeBg:      '#17212b',
      surfaceBg:     '#17212b',
      surface2Bg:    '#232e3c',
      inputBg:       '#242f3d',
      borderColor:   '#2b3a4b',
      primaryText:   '#ffffff',
      secondaryText: '#aab8c8',
      mutedText:     '#6d7c8b',
      disabledText:  '#475a6e',
      accent:        '#5288c1',
      accentText:    '#6cb5f9',
      onAccent:      '#ffffff',
      success:       '#67ef85',
      warning:       '#ffc444',
      danger:        '#ec6657',
      info:          '#5fb4e2',
    },
  },
  {
    id: 'daylight',
    name: 'Daylight',
    mode: 'light',
    palette: {
      appBg:         '#ffffff',
      chromeBg:      '#517da2',
      surfaceBg:     '#ffffff',
      surface2Bg:    '#f1f1f1',
      inputBg:       '#f7f7f7',
      borderColor:   '#e1e1e1',
      primaryText:   '#000000',
      secondaryText: '#4d4d4d',
      mutedText:     '#a0a0a0',
      disabledText:  '#c4c4c4',
      accent:        '#2481cc',
      accentText:    '#2481cc',
      onAccent:      '#ffffff',
      success:       '#4dcd5e',
      warning:       '#ffa700',
      danger:        '#fc3c2e',
      info:          '#5481b0',
    },
  },
  {
    id: 'arctic',
    name: 'Arctic',
    mode: 'dark',
    palette: {
      appBg:         '#2e3440',
      chromeBg:      '#3b4252',
      surfaceBg:     '#3b4252',
      surface2Bg:    '#434c5e',
      inputBg:       '#434c5e',
      borderColor:   '#4c566a',
      primaryText:   '#eceff4',
      secondaryText: '#d8dee9',
      mutedText:     '#81a1c1',
      disabledText:  '#4c566a',
      accent:        '#88c0d0',
      accentText:    '#88c0d0',
      onAccent:      '#2e3440',
      success:       '#a3be8c',
      warning:       '#ebcb8b',
      danger:        '#bf616a',
      info:          '#5e81ac',
    },
  },
  {
    id: 'vampire',
    name: 'Vampire',
    mode: 'dark',
    palette: {
      appBg:         '#282a36',
      chromeBg:      '#21222c',
      surfaceBg:     '#282a36',
      surface2Bg:    '#44475a',
      inputBg:       '#21222c',
      borderColor:   '#44475a',
      primaryText:   '#f8f8f2',
      secondaryText: '#f8f8f2',
      mutedText:     '#6272a4',
      disabledText:  '#44475a',
      accent:        '#bd93f9',
      accentText:    '#ff79c6',
      onAccent:      '#282a36',
      success:       '#50fa7b',
      warning:       '#f1fa8c',
      danger:        '#ff5555',
      info:          '#8be9fd',
    },
  },
  {
    id: 'mocha',
    name: 'Mocha',
    mode: 'dark',
    palette: {
      appBg:         '#1e1e2e',
      chromeBg:      '#181825',
      surfaceBg:     '#313244',
      surface2Bg:    '#45475a',
      inputBg:       '#11111b',
      borderColor:   '#585b70',
      primaryText:   '#cdd6f4',
      secondaryText: '#bac2de',
      mutedText:     '#7f849c',
      disabledText:  '#6c7086',
      accent:        '#cba6f7',
      accentText:    '#89b4fa',
      onAccent:      '#1e1e2e',
      success:       '#a6e3a1',
      warning:       '#f9e2af',
      danger:        '#f38ba8',
      info:          '#74c7ec',
    },
  },
  {
    id: 'latte',
    name: 'Latte',
    mode: 'light',
    palette: {
      appBg:         '#eff1f5',
      chromeBg:      '#e6e9ef',
      surfaceBg:     '#ffffff',
      surface2Bg:    '#ccd0da',
      inputBg:       '#dce0e8',
      borderColor:   '#bcc0cc',
      primaryText:   '#4c4f69',
      secondaryText: '#5c5f77',
      mutedText:     '#8c8fa1',
      disabledText:  '#9ca0b0',
      accent:        '#8839ef',
      accentText:    '#1e66f5',
      onAccent:      '#ffffff',
      success:       '#40a02b',
      warning:       '#df8e1d',
      danger:        '#d20f39',
      info:          '#209fb5',
    },
  },
  {
    id: 'cobalt',
    name: 'Cobalt',
    mode: 'dark',
    palette: {
      appBg:         '#1a1b26',
      chromeBg:      '#16161e',
      surfaceBg:     '#1a1b26',
      surface2Bg:    '#24283b',
      inputBg:       '#16161e',
      borderColor:   '#3b4261',
      primaryText:   '#c0caf5',
      secondaryText: '#a9b1d6',
      mutedText:     '#565f89',
      disabledText:  '#3b4261',
      accent:        '#7aa2f7',
      accentText:    '#bb9af7',
      onAccent:      '#1a1b26',
      success:       '#9ece6a',
      warning:       '#e0af68',
      danger:        '#f7768e',
      info:          '#7dcfff',
    },
  },
  {
    id: 'slate',
    name: 'Slate',
    mode: 'dark',
    palette: {
      appBg:         '#22272e',
      chromeBg:      '#2d333b',
      surfaceBg:     '#2d333b',
      surface2Bg:    '#373e47',
      inputBg:       '#22272e',
      borderColor:   '#444c56',
      primaryText:   '#adbac7',
      secondaryText: '#909dab',
      mutedText:     '#768390',
      disabledText:  '#545d68',
      accent:        '#539bf5',
      accentText:    '#539bf5',
      onAccent:      '#ffffff',
      success:       '#57ab5a',
      warning:       '#c69026',
      danger:        '#e5534b',
      info:          '#6cb6ff',
    },
  },
];

export const DEFAULT_PRESET_ID = 'onyx';

export function getPreset(id: string): ThemePreset {
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0];
}

/** Maps each ThemePalette field to the CSS variable name it controls. */
export const CSS_VAR_MAP: Record<keyof ThemePalette, string> = {
  appBg:         '--color-app',
  chromeBg:      '--color-chrome',
  surfaceBg:     '--color-surface',
  surface2Bg:    '--color-surface-2',
  inputBg:       '--color-input',
  borderColor:   '--color-border',
  primaryText:   '--color-text-primary',
  secondaryText: '--color-text-secondary',
  mutedText:     '--color-text-muted',
  disabledText:  '--color-text-disabled',
  accent:        '--color-accent',
  accentText:    '--color-accent-text',
  onAccent:      '--color-text-on-accent',
  success:       '--color-success',
  warning:       '--color-warning',
  danger:        '--color-danger',
  info:          '--color-info',
};
