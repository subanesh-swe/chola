import { useState, useEffect } from 'react';
import { BrowserRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from 'sonner';
import { AppRouter } from './router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { ThemeProvider } from './components/ThemeProvider';
import { useThemeStore } from './stores/theme';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5000,
      retry: 1,
    },
  },
});

function AppShell() {
  const mode = useThemeStore((s) => s.mode);
  // Resolve to 'dark' or 'light' for Toaster; fall back to dark for 'system'
  const toasterTheme = mode === 'light' ? 'light' : 'dark';

  return (
    <BrowserRouter>
      <ThemeProvider />
      <AppRouter />
      <Toaster theme={toasterTheme} position="bottom-right" />
    </BrowserRouter>
  );
}

export default function App() {
  return (
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <AppShell />
      </QueryClientProvider>
    </ErrorBoundary>
  );
}
