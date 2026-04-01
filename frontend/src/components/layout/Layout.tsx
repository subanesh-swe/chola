import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { Header } from './Header';
import { SkipToContent } from '../ui/SkipToContent';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';

export function Layout() {
  useKeyboardShortcuts();

  return (
    <>
      <SkipToContent />
      <div className="flex min-h-screen bg-slate-950">
        <Sidebar />
        <div className="flex-1 flex flex-col min-w-0">
          <Header />
          <main id="main-content" role="main" className="flex-1 p-6 overflow-auto">
            <Outlet />
          </main>
        </div>
      </div>
    </>
  );
}
