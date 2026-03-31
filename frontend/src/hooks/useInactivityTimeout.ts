import { useEffect, useCallback } from 'react';
import { useAuthStore } from '../stores/auth';

export function useInactivityTimeout(timeoutMs: number = 30 * 60 * 1000) {
  const logout = useAuthStore(s => s.logout);
  const isAuthenticated = useAuthStore(s => s.isAuthenticated);

  const handleActivity = useCallback(() => {
    if (isAuthenticated) {
      localStorage.setItem('chola-last-activity', Date.now().toString());
    }
  }, [isAuthenticated]);

  useEffect(() => {
    if (!isAuthenticated) return;

    const events = ['mousedown', 'keydown', 'scroll', 'touchstart'] as const;
    events.forEach(e => window.addEventListener(e, handleActivity, { passive: true }));

    const checker = setInterval(() => {
      const last = parseInt(localStorage.getItem('chola-last-activity') || '0');
      if (Date.now() - last > timeoutMs) {
        logout();
        window.location.href = '/login';
      }
    }, 60000);

    handleActivity();

    return () => {
      events.forEach(e => window.removeEventListener(e, handleActivity));
      clearInterval(checker);
    };
  }, [isAuthenticated, timeoutMs, handleActivity, logout]);
}
