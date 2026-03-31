import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

export function useKeyboardShortcuts() {
  const nav = useNavigate();

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't capture when typing in inputs
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      // Navigation shortcuts: g + <key>
      if (e.key === 'g' && !e.ctrlKey && !e.metaKey) {
        const handler2 = (e2: KeyboardEvent) => {
          window.removeEventListener('keydown', handler2);
          switch (e2.key) {
            case 'd': nav('/'); break;
            case 'b': nav('/builds'); break;
            case 'w': nav('/workers'); break;
            case 'r': nav('/repos'); break;
            case 'u': nav('/users'); break;
          }
        };
        window.addEventListener('keydown', handler2, { once: true });
        setTimeout(() => window.removeEventListener('keydown', handler2), 1000);
      }

      // Refresh: Ctrl+R / Cmd+R
      if (e.key === 'r' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        window.location.reload();
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [nav]);
}
