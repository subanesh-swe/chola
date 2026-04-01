import { useState } from 'react';
import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { Header } from './Header';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { SearchDialog } from '../ui/SearchDialog';

export function Layout() {
  const [searchOpen, setSearchOpen] = useState(false);
  useKeyboardShortcuts({ onSearch: () => setSearchOpen(true) });

  return (
    <div className="flex min-h-screen bg-slate-950">
      <Sidebar />
      <div className="flex-1 flex flex-col">
        <Header onSearch={() => setSearchOpen(true)} />
        <main className="flex-1 p-6 overflow-auto">
          <Outlet />
        </main>
      </div>
      <SearchDialog open={searchOpen} onClose={() => setSearchOpen(false)} />
    </div>
  );
}
