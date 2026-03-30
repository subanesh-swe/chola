import { NavLink } from 'react-router-dom';
import { useAuthStore } from '../../stores/auth';
import { clsx } from 'clsx';

const navItems = [
  { to: '/', label: 'Dashboard', icon: '~' },
  { to: '/builds', label: 'Builds', icon: '>' },
  { to: '/workers', label: 'Workers', icon: '#' },
  { to: '/repos', label: 'Repos', icon: '@' },
];

const adminItems = [
  { to: '/users', label: 'Users', icon: '+' },
];

export function Sidebar() {
  const user = useAuthStore((s) => s.user);
  const showAdmin = user?.role === 'super_admin';

  return (
    <aside className="w-60 bg-slate-900 border-r border-slate-700 flex flex-col min-h-screen">
      <div className="p-4 border-b border-slate-700">
        <h1 className="text-xl font-bold text-white">Chola CI</h1>
        <p className="text-xs text-slate-400 mt-1">Build Orchestrator</p>
      </div>
      <nav className="flex-1 p-3 space-y-1">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === '/'}
            className={({ isActive }) =>
              clsx(
                'flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors',
                isActive
                  ? 'bg-blue-600/20 text-blue-400'
                  : 'text-slate-300 hover:bg-slate-800 hover:text-white',
              )
            }
          >
            <span className="w-5 text-center font-mono">{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
        {showAdmin && (
          <>
            <div className="my-3 border-t border-slate-700" />
            <p className="px-3 text-xs font-semibold text-slate-500 uppercase">Admin</p>
            {adminItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  clsx(
                    'flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors',
                    isActive
                      ? 'bg-blue-600/20 text-blue-400'
                      : 'text-slate-300 hover:bg-slate-800 hover:text-white',
                  )
                }
              >
                <span className="w-5 text-center font-mono">{item.icon}</span>
                {item.label}
              </NavLink>
            ))}
          </>
        )}
      </nav>
    </aside>
  );
}
