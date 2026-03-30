import { type ReactNode } from 'react';

interface Props {
  open: boolean;
  title: string;
  message: string | ReactNode;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  variant?: 'danger' | 'default';
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = 'Confirm',
  onConfirm,
  onCancel,
  variant = 'default',
}: Props) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full mx-4">
        <h3 className="text-lg font-semibold text-white mb-2">{title}</h3>
        <div className="text-sm text-slate-300 mb-6">{message}</div>
        <div className="flex justify-end gap-3">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            className={
              variant === 'danger'
                ? 'px-4 py-2 text-sm text-white bg-red-600 hover:bg-red-700 rounded-lg transition-colors'
                : 'px-4 py-2 text-sm text-white bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors'
            }
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
