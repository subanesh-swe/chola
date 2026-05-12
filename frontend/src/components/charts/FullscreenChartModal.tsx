import { useEffect, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';

interface FullscreenChartModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
}

export function FullscreenChartModal({ open, onClose, title, children }: FullscreenChartModalProps) {
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [open, onClose]);

  if (!open) return null;

  return createPortal(
    <div
      className="fixed inset-0 bg-app/80 backdrop-blur-sm z-50 flex items-center justify-center"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="bg-surface border border-border rounded-lg p-6 max-w-[95vw] max-h-[95vh] w-full h-full m-4 flex flex-col"
      >
        <div className="flex items-center justify-between mb-4 shrink-0">
          {title && <h2 className="text-sm font-semibold text-secondary">{title}</h2>}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close fullscreen chart"
            autoFocus
            className="ml-auto p-1.5 rounded text-muted hover:text-primary hover:bg-surface-2 transition-colors"
          >
            <X size={16} />
          </button>
        </div>
        <div className="flex-1 min-h-0">
          {children}
        </div>
      </div>
    </div>,
    document.body,
  );
}
