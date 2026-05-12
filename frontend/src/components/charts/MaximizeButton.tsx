import { Maximize2 } from 'lucide-react';

interface MaximizeButtonProps {
  onClick: () => void;
  'aria-label': string;
}

export function MaximizeButton({ onClick, 'aria-label': ariaLabel }: MaximizeButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={ariaLabel}
      className="p-1 rounded text-muted hover:text-primary hover:bg-surface-2 transition-colors"
    >
      <Maximize2 size={14} />
    </button>
  );
}
