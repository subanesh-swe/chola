import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { useAuthStore } from '../stores/auth';
import { useThemeStore, type ThemeMode, type ThemeAccent } from '../stores/theme';
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

const ACCENT_SWATCHES: Record<ThemeAccent, string> = {
  indigo:  'bg-indigo-600',
  emerald: 'bg-emerald-600',
  rose:    'bg-rose-600',
};

function AppearanceSection() {
  const mode = useThemeStore((s) => s.mode);
  const accent = useThemeStore((s) => s.accent);
  const setMode = useThemeStore((s) => s.setMode);
  const setAccent = useThemeStore((s) => s.setAccent);

  const modes: { value: ThemeMode; label: string; beta?: boolean }[] = [
    { value: 'light',  label: 'Light',  beta: true },
    { value: 'dark',   label: 'Dark'               },
    { value: 'system', label: 'System'              },
  ];

  const accents: { value: ThemeAccent; label: string }[] = [
    { value: 'indigo',  label: 'Indigo'  },
    { value: 'emerald', label: 'Emerald' },
    { value: 'rose',    label: 'Rose'    },
  ];

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-white mb-1">Appearance</h3>
        <p className="text-xs text-slate-500">
          Light/accent themes will progressively roll out across pages.
        </p>
      </div>

      {/* Mode */}
      <div>
        <p className="text-sm font-medium text-slate-300 mb-3">Mode</p>
        <div className="flex gap-3" role="radiogroup" aria-label="Color mode">
          {modes.map(({ value, label, beta }) => {
            const active = mode === value;
            return (
              <label
                key={value}
                className={`flex items-center gap-2 px-4 py-2 rounded-lg border cursor-pointer text-sm transition-colors
                  ${active
                    ? 'border-blue-500 bg-blue-600/10 text-white'
                    : 'border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200'
                  }`}
              >
                <input
                  type="radio"
                  name="theme-mode"
                  value={value}
                  checked={active}
                  onChange={() => setMode(value)}
                  className="sr-only"
                />
                {label}
                {beta && (
                  <span
                    title="Some pages still render dark. Full theming coming soon."
                    className="text-[10px] px-1 py-0.5 rounded bg-amber-500/20 text-amber-400 border border-amber-500/30 leading-none"
                  >
                    Beta
                  </span>
                )}
              </label>
            );
          })}
        </div>
      </div>

      {/* Accent */}
      <div>
        <p className="text-sm font-medium text-slate-300 mb-3">Accent color</p>
        <div className="flex gap-3" role="radiogroup" aria-label="Accent color">
          {accents.map(({ value, label }) => {
            const active = accent === value;
            return (
              <label
                key={value}
                className={`flex items-center gap-2 px-4 py-2 rounded-lg border cursor-pointer text-sm transition-colors
                  ${active
                    ? 'border-blue-500 bg-blue-600/10 text-white'
                    : 'border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200'
                  }`}
              >
                <input
                  type="radio"
                  name="theme-accent"
                  value={value}
                  checked={active}
                  onChange={() => setAccent(value)}
                  className="sr-only"
                />
                <span className={`w-3 h-3 rounded-full shrink-0 ${ACCENT_SWATCHES[value]}`} aria-hidden="true" />
                {label}
              </label>
            );
          })}
        </div>
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
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6">
        <h3 className="text-lg font-semibold text-white mb-2">API Keys</h3>
        <p className="text-sm text-slate-500">API key management coming soon.</p>
      </div>
    </div>
  );
}
