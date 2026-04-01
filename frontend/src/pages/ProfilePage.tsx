import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { useAuthStore } from '../stores/auth';
import { changePassword } from '../api/auth';
import { TimeAgo } from '../components/ui/TimeAgo';
import { toast } from 'sonner';
import type { MutationError } from '../types';

const inputClass =
  'w-full bg-slate-800 border border-slate-600 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500';

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
    <div className="bg-slate-900 border border-slate-700 rounded-xl p-6">
      <h3 className="text-lg font-semibold text-white mb-4">Change Password</h3>
      <form onSubmit={handleSubmit} className="space-y-4 max-w-sm">
        <div>
          <label htmlFor="current-pw" className="block text-xs text-slate-400 mb-1">Current password</label>
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
          <label htmlFor="new-pw" className="block text-xs text-slate-400 mb-1">New password</label>
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
          <label htmlFor="confirm-pw" className="block text-xs text-slate-400 mb-1">Confirm new password</label>
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
          className="px-4 py-2 text-sm text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          {mutation.isPending ? 'Saving…' : 'Save Password'}
        </button>
      </form>
    </div>
  );
}

export default function ProfilePage() {
  const user = useAuthStore(s => s.user);

  if (!user) return null;

  return (
    <div className="space-y-6 max-w-2xl">
      <h2 className="text-2xl font-bold text-white">Profile</h2>

      {/* User info */}
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6">
        <h3 className="text-lg font-semibold text-white mb-4">Account Info</h3>
        <dl className="grid grid-cols-2 gap-4">
          <div>
            <dt className="text-xs text-slate-500">Username</dt>
            <dd className="text-sm text-slate-200 mt-0.5">{user.username}</dd>
          </div>
          <div>
            <dt className="text-xs text-slate-500">Display Name</dt>
            <dd className="text-sm text-slate-200 mt-0.5">{user.display_name || '—'}</dd>
          </div>
          <div>
            <dt className="text-xs text-slate-500">Role</dt>
            <dd className="mt-0.5">
              <span className="text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300">{user.role}</span>
            </dd>
          </div>
          <div>
            <dt className="text-xs text-slate-500">Member Since</dt>
            <dd className="text-sm text-slate-200 mt-0.5">
              <TimeAgo date={user.created_at} />
            </dd>
          </div>
        </dl>
      </div>

      <ChangePasswordSection />

      {/* API Keys — placeholder */}
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6">
        <h3 className="text-lg font-semibold text-white mb-2">API Keys</h3>
        <p className="text-sm text-slate-500">API key management coming soon.</p>
      </div>

      {/* Preferences — placeholder */}
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6">
        <h3 className="text-lg font-semibold text-white mb-2">Preferences</h3>
        <p className="text-sm text-slate-500">User preferences coming soon.</p>
      </div>
    </div>
  );
}
