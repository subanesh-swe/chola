import { useAuthStore } from '../../stores/auth';
import { useNavigate } from 'react-router-dom';

export function Header() {
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const navigate = useNavigate();

  const handleLogout = () => {
    logout();
    navigate('/login');
  };

  return (
    <header className="h-14 bg-slate-900 border-b border-slate-700 flex items-center justify-between px-6">
      <div />
      <div className="flex items-center gap-4">
        <span className="text-sm text-slate-300">
          {user?.display_name || user?.username}
        </span>
        <span className="text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300">
          {user?.role}
        </span>
        <button
          onClick={handleLogout}
          className="text-sm text-slate-400 hover:text-white transition-colors"
        >
          Logout
        </button>
      </div>
    </header>
  );
}
