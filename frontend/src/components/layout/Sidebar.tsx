import React, { useState } from 'react';
import { NavLink } from 'react-router-dom';
import { useAuthStore } from '../../stores/auth';
import { clsx } from 'clsx';

function DashIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
    </svg>
  );
}
function BuildIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
    </svg>
  );
}
function WorkerIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
    </svg>
  );
}
function RepoIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
    </svg>
  );
}
function UserIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
    </svg>
  );
}

type NavItemDef = { to: string; label: string; Icon: () => React.ReactElement };

const navItems: NavItemDef[] = [
  { to: '/', label: 'Dashboard', Icon: DashIcon },
  { to: '/builds', label: 'Builds', Icon: BuildIcon },
  { to: '/workers', label: 'Workers', Icon: WorkerIcon },
  { to: '/repos', label: 'Repos', Icon: RepoIcon },
];

const adminItems: NavItemDef[] = [
  { to: '/users', label: 'Users', Icon: UserIcon },
];

function NavItem({ to, label, Icon, collapsed, end }: NavItemDef & { collapsed: boolean; end?: boolean }) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        clsx(
          'flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm transition-all relative',
          isActive ? 'bg-blue-600/20 text-blue-400' : 'text-slate-400 hover:bg-slate-800 hover:text-white',
          collapsed && 'justify-center',
        )
      }
    >
      {({ isActive }) => (
        <>
          {isActive && (
            <div className="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-5 bg-blue-500 rounded-r" />
          )}
          <Icon />
          {!collapsed && <span>{label}</span>}
        </>
      )}
    </NavLink>
  );
}

export function Sidebar() {
  const user = useAuthStore((s) => s.user);
  const showAdmin = user?.role === 'super_admin';
  const [collapsed, setCollapsed] = useState(false);

  return (
    <aside
      className={clsx(
        'bg-slate-900 border-r border-slate-700 flex flex-col min-h-screen transition-all duration-200',
        collapsed ? 'w-16' : 'w-60',
      )}
    >
      <div className="p-4 border-b border-slate-700 flex items-center justify-between">
        {!collapsed && (
          <div>
            <h1 className="text-xl font-bold text-white">Chola CI</h1>
            <p className="text-xs text-slate-500">v0.1.0</p>
          </div>
        )}
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="text-slate-500 hover:text-white p-1 rounded transition-colors"
          title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d={collapsed ? 'M13 5l7 7-7 7' : 'M11 19l-7-7 7-7'}
            />
          </svg>
        </button>
      </div>

      <nav className="flex-1 p-2 space-y-1">
        {navItems.map(({ to, label, Icon }) => (
          <NavItem key={to} to={to} label={label} Icon={Icon} collapsed={collapsed} end={to === '/'} />
        ))}
        {showAdmin && (
          <>
            <div className="my-3 border-t border-slate-800" />
            {!collapsed && (
              <p className="px-3 text-[10px] font-semibold text-slate-600 uppercase tracking-wider">
                Admin
              </p>
            )}
            {adminItems.map(({ to, label, Icon }) => (
              <NavItem key={to} to={to} label={label} Icon={Icon} collapsed={collapsed} />
            ))}
          </>
        )}
      </nav>

      {!collapsed && (
        <div className="p-3 border-t border-slate-800">
          <div className="flex items-center gap-2 px-2">
            <div className="w-7 h-7 rounded-full bg-blue-600 flex items-center justify-center text-xs font-bold text-white shrink-0">
              {(user?.username?.[0] ?? 'U').toUpperCase()}
            </div>
            <div className="truncate">
              <p className="text-xs text-slate-300 truncate">{user?.display_name || user?.username}</p>
              <p className="text-[10px] text-slate-600">{user?.role}</p>
            </div>
          </div>
        </div>
      )}
    </aside>
  );
}
