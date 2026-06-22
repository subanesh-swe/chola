import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { useAuthStore } from '../stores/auth';
import { useThemeStore } from '../stores/theme';
import { PRESETS, PALETTE_FIELDS, getPreset, type ThemePalette } from '../themes/presets';
import { PATTERNS, getPattern } from '../themes/patterns';
import { changePassword } from '../api/auth';
import { TimeAgo } from '../components/ui/TimeAgo';
import { toast } from 'sonner';
import type { MutationError } from '../types';

const inputClass =
  'w-full bg-input border border-border rounded-lg px-3 py-2 text-sm text-primary placeholder:text-disabled focus:outline-none focus:ring-2 focus:ring-accent';

function ChangePasswordSection() {
  const [currentPw, setCurrentPw] = useState('');
  const [newPw, setNewPw] = useState('');
  const [confirmPw, setConfirmPw] = useState('');
  const [validationError, setValidationError] = useState('');

  const mutation = useMutation({
    mutationFn: changePassword,
    onSuccess: () => {
      toast.success('Password changed successfully.');
      setCurrentPw('');
      setNewPw('');
      setConfirmPw('');
      setValidationError('');
      mutation.reset();
    },
    onError: (err: unknown) => {
      toast.error((err as MutationError).userMessage || 'Failed to change password.');
    },
  });

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setValidationError('');
    if (newPw.length < 8) {
      setValidationError('New password must be at least 8 characters.');
      return;
    }
    if (newPw !== confirmPw) {
      setValidationError('New passwords do not match.');
      return;
    }
    mutation.mutate({ current_password: currentPw, new_password: newPw });
  }

  return (
    <div className="bg-surface border border-border rounded-xl p-6">
      <h3 className="text-lg font-semibold text-primary mb-4">Change Password</h3>
      <form onSubmit={handleSubmit} className="space-y-4 max-w-sm">
        <div>
          <label htmlFor="current-pw" className="block text-xs text-muted mb-1">Current password</label>
          <input
            id="current-pw"
            type="password"
            value={currentPw}
            onChange={e => setCurrentPw(e.target.value)}
            className={inputClass}
            required
            autoComplete="current-password"
          />
        </div>
        <div>
          <label htmlFor="new-pw" className="block text-xs text-muted mb-1">New password</label>
          <input
            id="new-pw"
            type="password"
            value={newPw}
            onChange={e => setNewPw(e.target.value)}
            className={inputClass}
            required
            autoComplete="new-password"
          />
        </div>
        <div>
          <label htmlFor="confirm-pw" className="block text-xs text-muted mb-1">Confirm new password</label>
          <input
            id="confirm-pw"
            type="password"
            value={confirmPw}
            onChange={e => setConfirmPw(e.target.value)}
            className={inputClass}
            required
            autoComplete="new-password"
          />
        </div>
        {validationError && (
          <p className="text-sm text-red-400" role="alert">{validationError}</p>
        )}
        <button
          type="submit"
          disabled={mutation.isPending}
          className="px-4 py-2 text-sm text-on-accent bg-accent hover:bg-accent-hover disabled:opacity-50 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
        >
          {mutation.isPending ? 'Saving…' : 'Save Password'}
        </button>
      </form>
    </div>
  );
}

/** A swatch + HEX text + native color picker for one token. */
function TokenRow({
  field,
  label,
  hex,
  onChange,
}: {
  field: keyof ThemePalette;
  label: string;
  hex: string;
  onChange: (next: string) => void;
}) {
  // Validate before propagating so users can type intermediate values.
  const [draft, setDraft] = useState(hex);
  // Sync external -> local when palette changes (preset switch, reset, etc.)
  if (draft !== hex && /^#[0-9a-fA-F]{3,8}$/.test(draft)) {
    // user is in the middle of typing — don't reset
  } else if (draft !== hex && !/^#/.test(draft)) {
    setDraft(hex);
  } else if (draft !== hex && draft.length === 0) {
    setDraft(hex);
  } else if (draft !== hex && /^#[0-9a-fA-F]{3,8}$/.test(hex) && draft !== hex) {
    // hex changed externally (preset applied) — sync if user isn't editing
    setDraft(hex);
  }

  return (
    <div className="flex items-center gap-3 py-1.5">
      <label htmlFor={`tok-${field}`} className="flex-1 text-sm text-secondary truncate">{label}</label>
      <span
        className="w-6 h-6 rounded border border-border shrink-0"
        style={{ backgroundColor: hex }}
        aria-hidden="true"
      />
      <input
        type="text"
        value={draft}
        onChange={(e) => {
          const v = e.target.value;
          setDraft(v);
          if (/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(v)) onChange(v);
        }}
        onBlur={() => {
          if (!/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(draft)) setDraft(hex);
        }}
        spellCheck={false}
        className="w-24 bg-input border border-border rounded px-2 py-1 text-xs text-primary font-mono focus:outline-none focus:ring-2 focus:ring-accent"
        aria-label={`${label} hex value`}
      />
      <input
        id={`tok-${field}`}
        type="color"
        value={hex.length >= 7 ? hex.slice(0, 7) : hex}
        onChange={(e) => {
          const v = e.target.value;
          setDraft(v);
          onChange(v);
        }}
        className="w-8 h-8 rounded border border-border cursor-pointer bg-transparent"
        aria-label={`${label} color picker`}
      />
    </div>
  );
}

function AppearanceSection() {
  const presetId = useThemeStore((s) => s.presetId);
  const palette = useThemeStore((s) => s.palette);
  const applyPreset = useThemeStore((s) => s.applyPreset);
  const setToken = useThemeStore((s) => s.setToken);
  const importPalette = useThemeStore((s) => s.importPalette);
  const exportPalette = useThemeStore((s) => s.exportPalette);
  const patternId = useThemeStore((s) => s.patternId);
  const patternOpacity = useThemeStore((s) => s.patternOpacity);
  const setPattern = useThemeStore((s) => s.setPattern);
  const setPatternOpacity = useThemeStore((s) => s.setPatternOpacity);
  // Subscribe to the saved snapshot so the diff re-renders on save/reset.
  const saved = useThemeStore((s) => s.saved);
  const saveTheme = useThemeStore((s) => s.save);
  const resetTheme = useThemeStore((s) => s.reset);

  // Compute the diff against the saved snapshot. Re-derives whenever any
  // edit happens (because every dependency above re-runs the subscription).
  const tokenDiff = PALETTE_FIELDS.filter((f) => palette[f.key] !== saved.palette[f.key]);
  const presetChanged = presetId !== saved.presetId;
  const modeChanged = useThemeStore((s) => s.mode) !== saved.mode;
  const patternChanged = patternId !== saved.patternId;
  const patternOpacityChanged = patternOpacity !== saved.patternOpacity;
  const dirty =
    tokenDiff.length > 0 || presetChanged || modeChanged || patternChanged || patternOpacityChanged;

  const [importDraft, setImportDraft] = useState('');
  const [showImport, setShowImport] = useState(false);

  // Group tokens by their `group` field for readable layout
  const groups = Array.from(new Set(PALETTE_FIELDS.map((f) => f.group)));

  function handleCopy() {
    navigator.clipboard.writeText(exportPalette()).then(
      () => toast.success('Theme JSON copied'),
      () => toast.error('Copy failed'),
    );
  }

  function handleImport() {
    if (importPalette(importDraft)) {
      toast.success('Theme imported');
      setShowImport(false);
      setImportDraft('');
    } else {
      toast.error('Invalid theme JSON');
    }
  }

  return (
    <div className="bg-surface border border-border rounded-xl p-6 space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-primary mb-1">Appearance</h3>
        <p className="text-xs text-muted">
          Pick a preset or edit any of the 17 colors individually. Changes preview live;
          click <span className="text-secondary font-medium">Save</span> to keep them or
          {' '}<span className="text-secondary font-medium">Reset</span> to revert.
        </p>
      </div>

      {/* Presets */}
      <div>
        <p className="text-sm font-medium text-secondary mb-3">Preset</p>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-2" role="radiogroup" aria-label="Theme preset">
          {PRESETS.map((p) => {
            const active = presetId === p.id;
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => applyPreset(p.id)}
                aria-pressed={active}
                className={`flex items-center gap-2 px-3 py-2 rounded-lg border text-left text-sm transition-colors
                  ${active
                    ? 'border-accent bg-accent-soft text-primary'
                    : 'border-border text-muted hover:border-border-strong hover:text-secondary'
                  }`}
              >
                {/* Mini palette preview — 4 swatches */}
                <span className="flex shrink-0 rounded overflow-hidden border border-border">
                  <span className="w-3 h-5" style={{ backgroundColor: p.palette.appBg }} aria-hidden="true" />
                  <span className="w-3 h-5" style={{ backgroundColor: p.palette.chromeBg }} aria-hidden="true" />
                  <span className="w-3 h-5" style={{ backgroundColor: p.palette.surfaceBg }} aria-hidden="true" />
                  <span className="w-3 h-5" style={{ backgroundColor: p.palette.accent }} aria-hidden="true" />
                </span>
                <span className="truncate">{p.name}</span>
              </button>
            );
          })}
          {presetId === 'custom' && (
            <span className="flex items-center px-3 py-2 rounded-lg border border-warning/30 bg-warning-soft text-warning text-xs">
              Custom (modified)
            </span>
          )}
        </div>
      </div>

      {/* Per-token editor — grouped */}
      <div>
        <div className="flex items-center justify-between mb-3">
          <p className="text-sm font-medium text-secondary">Colors</p>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={handleCopy}
              className="text-xs text-muted hover:text-primary underline focus:outline-none focus:ring-1 focus:ring-accent rounded"
            >
              Copy JSON
            </button>
            <button
              type="button"
              onClick={() => setShowImport((v) => !v)}
              className="text-xs text-muted hover:text-primary underline focus:outline-none focus:ring-1 focus:ring-accent rounded"
            >
              Import…
            </button>
          </div>
        </div>

        {showImport && (
          <div className="mb-4 space-y-2">
            <textarea
              value={importDraft}
              onChange={(e) => setImportDraft(e.target.value)}
              placeholder='{"appBg":"#…","accent":"#…",…}'
              rows={4}
              className="w-full bg-input border border-border rounded-lg px-3 py-2 text-xs text-primary font-mono focus:outline-none focus:ring-2 focus:ring-accent"
              spellCheck={false}
            />
            <div className="flex gap-2 justify-end">
              <button
                type="button"
                onClick={() => { setShowImport(false); setImportDraft(''); }}
                className="px-3 py-1 text-xs text-muted hover:text-primary focus:outline-none focus:ring-1 focus:ring-border rounded"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleImport}
                disabled={!importDraft.trim()}
                className="px-3 py-1 text-xs bg-accent text-on-accent rounded hover:bg-accent-hover disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-accent"
              >
                Import
              </button>
            </div>
          </div>
        )}

        <div className="space-y-4">
          {groups.map((g) => (
            <div key={g}>
              <p className="text-xs font-semibold text-disabled uppercase tracking-wider mb-1">{g}</p>
              <div className="divide-y divide-border/40 border border-border/40 rounded-lg px-3">
                {PALETTE_FIELDS.filter((f) => f.group === g).map((f) => (
                  <TokenRow
                    key={f.key}
                    field={f.key}
                    label={f.label}
                    hex={palette[f.key]}
                    onChange={(next) => setToken(f.key, next)}
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Background pattern */}
      <div>
        <p className="text-sm font-medium text-secondary mb-3">Background pattern</p>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2" role="radiogroup" aria-label="Background pattern">
          {PATTERNS.map((p) => {
            const active = patternId === p.id;
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => setPattern(p.id)}
                aria-pressed={active}
                className={`relative flex flex-col items-stretch h-16 rounded-lg border overflow-hidden text-left text-xs transition-colors
                  ${active
                    ? 'border-accent ring-2 ring-accent/30'
                    : 'border-border hover:border-border-strong'
                  }`}
              >
                {/* Tile preview using the actual pattern image */}
                <span
                  className="absolute inset-0"
                  style={{
                    backgroundImage: p.image === 'none' ? undefined : p.image,
                    backgroundSize: p.size,
                    backgroundRepeat: 'repeat',
                    opacity: p.opacity,
                    color: 'var(--color-text-primary)',
                  }}
                  aria-hidden="true"
                />
                <span className="relative mt-auto px-2 py-1 bg-surface/70 text-primary truncate">
                  {p.name}
                </span>
              </button>
            );
          })}
        </div>

        {patternId !== 'none' && (
          <div className="mt-3 flex items-center gap-3">
            <label htmlFor="pattern-opacity" className="text-xs text-muted shrink-0">Intensity</label>
            <input
              id="pattern-opacity"
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={patternOpacity}
              onChange={(e) => setPatternOpacity(parseFloat(e.target.value))}
              className="flex-1 accent-accent"
            />
            <span className="text-xs text-muted w-10 text-right tabular-nums">{Math.round(patternOpacity * 100)}%</span>
          </div>
        )}
      </div>

      {/* Diff + Save/Reset bar — only renders when there are unsaved edits */}
      {dirty && (
        <ChangeReview
          tokenDiff={tokenDiff}
          presetChanged={presetChanged}
          patternChanged={patternChanged}
          patternOpacityChanged={patternOpacityChanged}
          onSave={() => { saveTheme(); toast.success('Theme saved'); }}
          onReset={() => { resetTheme(); toast.info('Changes discarded'); }}
        />
      )}
    </div>
  );
}

/** Diff + Save/Reset bar shown above the rest of the page when there are unsaved theme edits. */
function ChangeReview({
  tokenDiff,
  presetChanged,
  patternChanged,
  patternOpacityChanged,
  onSave,
  onReset,
}: {
  tokenDiff: typeof PALETTE_FIELDS;
  presetChanged: boolean;
  patternChanged: boolean;
  patternOpacityChanged: boolean;
  onSave: () => void;
  onReset: () => void;
}) {
  const presetId = useThemeStore((s) => s.presetId);
  const palette = useThemeStore((s) => s.palette);
  const patternId = useThemeStore((s) => s.patternId);
  const patternOpacity = useThemeStore((s) => s.patternOpacity);
  const saved = useThemeStore((s) => s.saved);

  const total =
    tokenDiff.length +
    (presetChanged ? 1 : 0) +
    (patternChanged ? 1 : 0) +
    (patternOpacityChanged ? 1 : 0);

  const presetLabel = (id: string) =>
    id === 'custom' ? 'Custom' : getPreset(id).name;

  return (
    <div
      className="sticky bottom-4 z-10 -mx-2 mt-4 bg-surface border border-accent/40 rounded-xl p-4 shadow-lg"
      role="region"
      aria-label="Unsaved theme changes"
    >
      <div className="flex items-center justify-between flex-wrap gap-3 mb-3">
        <div>
          <p className="text-sm font-semibold text-primary">
            {total} unsaved change{total === 1 ? '' : 's'}
          </p>
          <p className="text-xs text-muted">Preview is live. Save to keep, or reset to revert.</p>
        </div>
        <div className="flex gap-2 shrink-0">
          <button
            type="button"
            onClick={onReset}
            className="px-3 py-1.5 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Reset
          </button>
          <button
            type="button"
            onClick={onSave}
            className="px-4 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Save
          </button>
        </div>
      </div>

      <div className="max-h-44 overflow-y-auto space-y-1 text-xs">
        {presetChanged && (
          <div className="flex items-center gap-2 px-2 py-1 rounded bg-surface-2/40">
            <span className="text-muted w-32 shrink-0">Preset</span>
            <span className="text-disabled">{presetLabel(saved.presetId)}</span>
            <span className="text-disabled" aria-hidden="true">→</span>
            <span className="text-accent-text font-medium">{presetLabel(presetId)}</span>
          </div>
        )}
        {patternChanged && (
          <div className="flex items-center gap-2 px-2 py-1 rounded bg-surface-2/40">
            <span className="text-muted w-32 shrink-0">Background pattern</span>
            <span className="text-disabled">{getPattern(saved.patternId).name}</span>
            <span className="text-disabled" aria-hidden="true">→</span>
            <span className="text-accent-text font-medium">{getPattern(patternId).name}</span>
          </div>
        )}
        {patternOpacityChanged && (
          <div className="flex items-center gap-2 px-2 py-1 rounded bg-surface-2/40">
            <span className="text-muted w-32 shrink-0">Pattern intensity</span>
            <span className="text-disabled">{Math.round(saved.patternOpacity * 100)}%</span>
            <span className="text-disabled" aria-hidden="true">→</span>
            <span className="text-accent-text font-medium">{Math.round(patternOpacity * 100)}%</span>
          </div>
        )}
        {tokenDiff.map((f) => (
          <div key={f.key} className="flex items-center gap-2 px-2 py-1 rounded bg-surface-2/40">
            <span className="text-muted w-32 shrink-0 truncate" title={f.label}>{f.label}</span>
            <span
              className="w-4 h-4 rounded border border-border shrink-0"
              style={{ backgroundColor: saved.palette[f.key] }}
              aria-hidden="true"
            />
            <span className="text-disabled font-mono text-[10px]">{saved.palette[f.key]}</span>
            <span className="text-disabled" aria-hidden="true">→</span>
            <span
              className="w-4 h-4 rounded border border-accent/40 shrink-0"
              style={{ backgroundColor: palette[f.key] }}
              aria-hidden="true"
            />
            <span className="text-accent-text font-mono text-[10px] font-medium">{palette[f.key]}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function ProfilePage() {
  const user = useAuthStore(s => s.user);

  if (!user) return null;

  return (
    <div className="space-y-6 max-w-2xl">
      <h2 className="text-2xl font-bold text-primary">Profile</h2>

      {/* User info */}
      <div className="bg-surface border border-border rounded-xl p-6">
        <h3 className="text-lg font-semibold text-primary mb-4">Account Info</h3>
        <dl className="grid grid-cols-2 gap-4">
          <div>
            <dt className="text-xs text-muted">Username</dt>
            <dd className="text-sm text-secondary mt-0.5">{user.username}</dd>
          </div>
          <div>
            <dt className="text-xs text-muted">Display Name</dt>
            <dd className="text-sm text-secondary mt-0.5">{user.display_name || '—'}</dd>
          </div>
          <div>
            <dt className="text-xs text-muted">Role</dt>
            <dd className="mt-0.5">
              <span className="text-xs px-2 py-0.5 rounded bg-surface-2 text-secondary">{user.role}</span>
            </dd>
          </div>
          <div>
            <dt className="text-xs text-muted">Member Since</dt>
            <dd className="text-sm text-secondary mt-0.5">
              <TimeAgo date={user.created_at} />
            </dd>
          </div>
        </dl>
      </div>

      <ChangePasswordSection />

      <AppearanceSection />

      {/* API Keys — placeholder */}
      <div className="bg-surface border border-border rounded-xl p-6">
        <h3 className="text-lg font-semibold text-primary mb-2">API Keys</h3>
        <p className="text-sm text-muted">API key management coming soon.</p>
      </div>
    </div>
  );
}
