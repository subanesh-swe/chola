import { useState, useEffect, useRef } from 'react';
import { useMutation } from '@tanstack/react-query';
import { changePassword } from '../../api/auth';

interface Props {
  open: boolean;
  onClose: () => void;
}

const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

export function ChangePasswordDialog({ open, onClose }: Props) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const [currentPw, setCurrentPw] = useState('');
  const [newPw, setNewPw] = useState('');
  const [confirmPw, setConfirmPw] = useState('');
  const [validationError, setValidationError] = useState('');

  const mutation = useMutation({
    mutationFn: changePassword,
    onSuccess: () => {
      setTimeout(onClose, 1200);
    },
  });

  // Reset state when dialog opens
  useEffect(() => {
    if (open) {
      setCurrentPw('');
      setNewPw('');
      setConfirmPw('');
      setValidationError('');
      mutation.reset();
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Escape key
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [open, onClose]);

  // Focus trap
  useEffect(() => {
    if (!open || !dialogRef.current) return;
    const el = dialogRef.current;
    const focusable = Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (focusable.length) focusable[0].focus();
    const trap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last.focus(); }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    document.addEventListener('keydown', trap);
    return () => document.removeEventListener('keydown', trap);
  }, [open]);

  if (!open) return null;

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

  const inputClass =
    'w-full bg-slate-800 border border-slate-600 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500';

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="change-pw-title"
    >
      <div
        ref={dialogRef}
        className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-sm w-full"
      >
        <h3 id="change-pw-title" className="text-lg font-semibold text-white mb-4">
          Change Password
        </h3>

        {mutation.isSuccess ? (
          <p className="text-sm text-green-400 mb-4">Password changed successfully.</p>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label htmlFor="current-pw" className="block text-xs text-slate-400 mb-1">
                Current password
              </label>
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
              <label htmlFor="new-pw" className="block text-xs text-slate-400 mb-1">
                New password
              </label>
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
              <label htmlFor="confirm-pw" className="block text-xs text-slate-400 mb-1">
                Confirm new password
              </label>
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

            {(validationError || mutation.isError) && (
              <p className="text-sm text-red-400" role="alert">
                {validationError || 'Failed to change password. Check your current password.'}
              </p>
            )}

            <div className="flex justify-end gap-3 pt-2">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 text-sm text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={mutation.isPending}
                className="px-4 py-2 text-sm text-white bg-blue-600 hover:bg-blue-700 disabled:opacity-50 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                {mutation.isPending ? 'Saving…' : 'Save'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
