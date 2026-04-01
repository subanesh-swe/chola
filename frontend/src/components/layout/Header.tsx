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

interface HeaderProps {
  onOpenSearch?: () => void;
}

export function Header({ onOpenSearch }: HeaderProps) {
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const nav = useNavigate();

  return (
    <header className="h-14 bg-slate-900 border-b border-slate-700 flex items-center justify-between px-6 pl-16 md:pl-6">
      <Breadcrumbs />
      <div className="flex items-center gap-4">
        {onOpenSearch && (
          <button
            onClick={onOpenSearch}
            aria-label="Search (Cmd+K)"
            className="flex items-center gap-2 px-3 py-1.5 text-sm text-slate-400 bg-slate-800 border border-slate-700 rounded-lg hover:bg-slate-700 hover:text-white transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            <span className="hidden sm:inline">Search</span>
            <kbd className="hidden sm:inline text-[10px] border border-slate-600 rounded px-1">Cmd+K</kbd>
          </button>
        )}
        <span className="hidden sm:inline text-sm text-slate-300">{user?.display_name || user?.username}</span>
        <span className="text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300">{user?.role}</span>
        <button
          onClick={() => nav('/profile')}
          className="text-sm text-slate-400 hover:text-white transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 rounded"
        >
          Profile
        </button>
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
