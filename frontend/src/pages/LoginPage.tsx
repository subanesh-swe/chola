import { useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuthStore } from '../stores/auth';
import { login } from '../api/auth';
import { toast } from 'sonner';
import type { MutationError } from '../types';

function EyeIcon({ open }: { open: boolean }) {
  return open ? (
    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
    </svg>
  ) : (
    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
    </svg>
  );
}

function Spinner() {
  return (
    <svg className="animate-spin h-4 w-4 text-on-accent" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" aria-hidden="true">
      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
    </svg>
  );
}

export default function LoginPage() {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const authLogin = useAuthStore(s => s.login);
  const nav = useNavigate();

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setLoading(true);
    try {
      const res = await login({ username, password });
      authLogin(res.token, res.expires_at, res.user);
      nav('/');
    } catch (err: unknown) {
      const e = err as MutationError & { response?: unknown };
      if (!e.response) {
        toast.error('Cannot reach server. Check your connection.');
      } else if (e.statusCode === 401) {
        toast.error('Invalid credentials.');
      } else if (e.statusCode === 403) {
        // Not a credentials problem — e.g. a cross-origin/CSRF block. Show the
        // backend's actual reason instead of misleading "Invalid credentials".
        toast.error(e.userMessage || 'Request blocked.');
      } else if (e.statusCode && e.statusCode >= 500) {
        toast.error('Server error. Please try again later.');
      } else {
        toast.error(e.userMessage || 'Login failed.');
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-app">
      <div className="w-full max-w-sm min-h-[340px] flex flex-col justify-center">
        <div className="bg-surface border border-border rounded-xl p-8">
          <h1 className="text-2xl font-bold text-primary mb-1">Chola CI</h1>
          <p className="text-sm text-muted mb-6">Sign in to your account</p>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm text-secondary mb-1">Username</label>
              <input
                type="text"
                value={username}
                onChange={e => setUsername(e.target.value)}
                className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary placeholder:text-disabled focus:outline-none focus:ring-2 focus:ring-accent"
                placeholder="admin"
                required
              />
            </div>
            <div>
              <label className="block text-sm text-secondary mb-1">Password</label>
              <div className="relative">
                <input
                  type={showPassword ? 'text' : 'password'}
                  value={password}
                  onChange={e => setPassword(e.target.value)}
                  className="w-full px-3 py-2 pr-10 bg-input border border-border rounded-lg text-primary placeholder:text-disabled focus:outline-none focus:ring-2 focus:ring-accent"
                  required
                />
                <button
                  type="button"
                  onClick={() => setShowPassword(v => !v)}
                  aria-label={showPassword ? 'Hide password' : 'Show password'}
                  className="absolute inset-y-0 right-0 flex items-center px-3 text-muted hover:text-secondary focus:outline-none"
                >
                  <EyeIcon open={showPassword} />
                </button>
              </div>
            </div>
            <button
              type="submit"
              disabled={loading}
              className="w-full py-2 flex items-center justify-center gap-2 bg-accent hover:bg-accent-hover disabled:opacity-50 disabled:cursor-not-allowed text-on-accent rounded-lg font-medium transition-colors"
            >
              {loading && <Spinner />}
              {loading ? 'Signing in...' : 'Sign In'}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
