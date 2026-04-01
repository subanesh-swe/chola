import { useAuthStore } from '../../stores/auth';
import { useNavigate, useLocation } from 'react-router-dom';

function Breadcrumbs() {
  const location = useLocation();
  const parts = location.pathname.split('/').filter(Boolean);

  if (parts.length === 0) return <span className="text-sm text-slate-400">Dashboard</span>;

  return (
    <div className="flex items-center gap-1 text-sm">
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          {i > 0 && <span className="text-slate-600">/</span>}
          <span className={i === parts.length - 1 ? 'text-slate-200' : 'text-slate-500'}>
            {part.charAt(0).toUpperCase() + part.slice(1)}
          </span>
        </span>
      ))}
    </div>
  );
}

export function Header() {
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const nav = useNavigate();

  return (
    <header className="h-14 bg-slate-900 border-b border-slate-700 flex items-center justify-between px-6 pl-16 md:pl-6">
      <Breadcrumbs />
      <div className="flex items-center gap-4">
        <span className="hidden sm:inline text-sm text-slate-300">{user?.display_name || user?.username}</span>
        <span className="text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300">{user?.role}</span>
        <button
          onClick={() => {
            logout();
            nav('/login');
          }}
          className="text-sm text-slate-400 hover:text-white transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 rounded"
        >
          Logout
        </button>
      </div>
    </header>
  );
}
