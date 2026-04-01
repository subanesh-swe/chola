import { useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuthStore } from '../stores/auth';
import { login } from '../api/auth';
import { toast } from 'sonner';
import type { MutationError } from '../types';
export default function LoginPage() {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const authLogin = useAuthStore(s => s.login);
  const nav = useNavigate();
  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault(); setLoading(true);
    try { const res = await login({ username, password }); authLogin(res.token, res.expires_at, res.user); nav('/'); }
    catch (err: unknown) {
      const e = err as MutationError;
      if (e.statusCode && e.statusCode >= 500) {
        toast.error('Server error. Please try again later.');
      } else if (e.statusCode === 401 || e.statusCode === 403) {
        toast.error('Invalid username or password.');
      } else {
        toast.error(e.userMessage || 'Login failed.');
      }
    }
    finally { setLoading(false); }
  };
  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-950">
      <div className="w-full max-w-sm">
        <div className="bg-slate-900 border border-slate-700 rounded-xl p-8">
          <h1 className="text-2xl font-bold text-white mb-1">Chola CI</h1>
          <p className="text-sm text-slate-400 mb-6">Sign in to your account</p>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div><label className="block text-sm text-slate-300 mb-1">Username</label><input type="text" value={username} onChange={e => setUsername(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="admin" required /></div>
            <div><label className="block text-sm text-slate-300 mb-1">Password</label><input type="password" value={password} onChange={e => setPassword(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-500" required /></div>
            <button type="submit" disabled={loading} className="w-full py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-lg font-medium transition-colors">{loading ? 'Signing in...' : 'Sign In'}</button>
          </form>
        </div>
      </div>
    </div>
  );
}
