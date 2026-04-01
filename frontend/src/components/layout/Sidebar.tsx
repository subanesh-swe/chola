import React, { useState, useEffect } from 'react';
import { NavLink } from 'react-router-dom';
import { useAuthStore } from '../../stores/auth';
import { clsx } from 'clsx';

function DashIcon() {
  return (
    <svg className="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
    </svg>
  );
}
function BuildIcon() {
  return (
    <svg className="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
    </svg>
  );
}
function WorkerIcon() {
  return (
    <svg className="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
    </svg>
  );
}
function RepoIcon() {
  return (
    <svg className="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
    </svg>
  );
}
function UserIcon() {
  return (
    <svg className="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
    </svg>
  );
}
function SettingsIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
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
  { to: '/settings', label: 'Settings', Icon: SettingsIcon },
];

function NavItem({ to, label, Icon, collapsed, end, onClick }: NavItemDef & { collapsed: boolean; end?: boolean; onClick?: () => void }) {
  return (
    <NavLink
      to={to}
      end={end}
      onClick={onClick}
      className={({ isActive }) =>
        clsx(
          'flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm transition-all relative',
          'focus:outline-none focus:ring-2 focus:ring-blue-500',
          isActive ? 'bg-blue-600/20 text-blue-400' : 'text-slate-400 hover:bg-slate-800 hover:text-white',
          collapsed && 'justify-center',
        )
      }
      aria-label={collapsed ? label : undefined}
    >
      {({ isActive }) => (
        <>
          {isActive && (
            <div className="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-5 bg-blue-500 rounded-r" aria-hidden="true" />
          )}
          <Icon />
          {!collapsed && <span>{label}</span>}
        </>
      )}
    </NavLink>
  );
}

function HamburgerIcon() {
  return (
    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
    </svg>
  );
}

export function Sidebar() {
  const user = useAuthStore((s) => s.user);
  const showAdmin = user?.role === 'super_admin';
  const [collapsed, setCollapsed] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);

  // Auto-collapse on small screens
  useEffect(() => {
    const mq = window.matchMedia('(max-width: 767px)');
    const handler = (e: MediaQueryListEvent) => {
      if (e.matches) setMobileOpen(false);
    };
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);

  return (
    <>
      {/* Hamburger button — mobile only */}
      <button
        className="md:hidden fixed top-4 left-4 z-50 p-2 rounded-lg bg-slate-900 border border-slate-700 text-slate-400 hover:text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
        onClick={() => setMobileOpen(!mobileOpen)}
        aria-label={mobileOpen ? 'Close navigation menu' : 'Open navigation menu'}
        aria-expanded={mobileOpen}
        aria-controls="sidebar-nav"
      >
        <HamburgerIcon />
      </button>

      {/* Mobile overlay backdrop */}
      {mobileOpen && (
        <div
          className="md:hidden fixed inset-0 z-30 bg-black/60"
          onClick={() => setMobileOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Sidebar */}
      <aside
        id="sidebar-nav"
        role="navigation"
        aria-label="Main navigation"
        className={clsx(
          'bg-slate-900 border-r border-slate-700 flex flex-col min-h-screen transition-all duration-200 z-40',
          // Desktop: always visible, collapsible
          'hidden md:flex',
          collapsed ? 'md:w-16' : 'md:w-60',
          // Mobile: fixed overlay when open
          mobileOpen && '!flex fixed inset-y-0 left-0 w-60',
        )}
      >
        <div className="p-4 border-b border-slate-700 flex items-center justify-between">
          {(!collapsed || mobileOpen) && (
            <div>
              <h1 className="text-xl font-bold text-white">Chola CI</h1>
              <p className="text-xs text-slate-500">v0.1.0</p>
            </div>
          )}
          {/* Collapse button — desktop only */}
          <button
            onClick={() => setCollapsed(!collapsed)}
            className="hidden md:block text-slate-500 hover:text-white p-1 rounded transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
            aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
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
            <NavItem
              key={to}
              to={to}
              label={label}
              Icon={Icon}
              collapsed={collapsed && !mobileOpen}
              end={to === '/'}
              onClick={() => setMobileOpen(false)}
            />
          ))}
          {showAdmin && (
            <>
              <div className="my-3 border-t border-slate-800" />
              {(!collapsed || mobileOpen) && (
                <p className="px-3 text-[10px] font-semibold text-slate-600 uppercase tracking-wider">
                  Admin
                </p>
              )}
              {adminItems.map(({ to, label, Icon }) => (
                <NavItem
                  key={to}
                  to={to}
                  label={label}
                  Icon={Icon}
                  collapsed={collapsed && !mobileOpen}
                  onClick={() => setMobileOpen(false)}
                />
              ))}
            </>
          )}
        </nav>

        {(!collapsed || mobileOpen) && (
          <div className="p-3 border-t border-slate-800">
            <div className="flex items-center gap-2 px-2">
              <div className="w-7 h-7 rounded-full bg-blue-600 flex items-center justify-center text-xs font-bold text-white shrink-0" aria-hidden="true">
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
    </>
  );
}
