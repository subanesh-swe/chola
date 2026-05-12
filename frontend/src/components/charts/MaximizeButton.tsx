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
      {/* Maximize2 icon (lucide) rendered as inline SVG */}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <polyline points="15 3 21 3 21 9" />
        <polyline points="9 21 3 21 3 15" />
        <line x1="21" y1="3" x2="14" y2="10" />
        <line x1="3" y1="21" x2="10" y2="14" />
      </svg>
    </button>
  );
}
