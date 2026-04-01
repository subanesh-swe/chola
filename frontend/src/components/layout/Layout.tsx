import { useState } from 'react';
import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { Header } from './Header';
import { SkipToContent } from '../ui/SkipToContent';
import { SearchDialog } from '../ui/SearchDialog';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';

export function Layout() {
  const [searchOpen, setSearchOpen] = useState(false);
  useKeyboardShortcuts({ onSearch: () => setSearchOpen(true) });

  return (
    <>
      <SkipToContent />
      <div className="flex min-h-screen bg-slate-950">
        <Sidebar />
        <div className="flex-1 flex flex-col min-w-0">
          <Header onOpenSearch={() => setSearchOpen(true)} />
          <main id="main-content" role="main" className="flex-1 p-6 overflow-auto">
            <Outlet />
          </main>
        </div>
      </div>
      <SearchDialog open={searchOpen} onClose={() => setSearchOpen(false)} />
    </>
  );
}
