import { useEffect, useRef } from 'react';
import { clsx } from 'clsx';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface Props {
  content?: string;
  liveChunks?: string[];
  className?: string;
  /**
   * RFC3339 timestamp from `files_purged_at`. When set, the viewer shows a
   * "logs purged" notice instead of attempting to fetch or display logs.
   */
  filesPurgedAt?: string | null;
}

function fmtDateShort(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
  });
}

export function LogViewer({ content, liveChunks, className, filesPurgedAt }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const writtenRef = useRef(0);

  // Bail out early — don't create a terminal for purged logs.
  const isPurged = !!filesPurgedAt;

  useEffect(() => {
    if (isPurged) return;
    if (!containerRef.current) return;

    const terminal = new Terminal({
      theme: {
        background: '#0f172a',
        foreground: '#e2e8f0',
        cursor: '#e2e8f0',
        selectionBackground: '#334155',
        black: '#1e293b',
        red: '#ef4444',
        green: '#22c55e',
        yellow: '#eab308',
        blue: '#3b82f6',
        magenta: '#a855f7',
        cyan: '#06b6d4',
        white: '#f1f5f9',
        brightBlack: '#475569',
        brightRed: '#f87171',
        brightGreen: '#4ade80',
        brightYellow: '#facc15',
        brightBlue: '#60a5fa',
        brightMagenta: '#c084fc',
        brightCyan: '#22d3ee',
        brightWhite: '#f8fafc',
      },
      fontSize: 13,
      fontFamily: 'JetBrains Mono, Menlo, Monaco, Consolas, monospace',
      convertEol: true,
      disableStdin: true,
      scrollback: 1_000_000,
    });

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(containerRef.current);
    fitAddon.fit();

    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;
    writtenRef.current = 0;

    if (content) {
      terminal.write(content);
    }

    const handleResize = () => fitAddon.fit();
    window.addEventListener('resize', handleResize);

    return () => {
      window.removeEventListener('resize', handleResize);
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [content, isPurged]);

  useEffect(() => {
    if (!terminalRef.current || !liveChunks) return;
    for (let i = writtenRef.current; i < liveChunks.length; i++) {
      terminalRef.current.write(liveChunks[i]);
    }
    writtenRef.current = liveChunks.length;
  }, [liveChunks]);

  if (isPurged) {
    return (
      <div
        className={clsx(
          'rounded-lg border border-slate-700 bg-slate-900/80 flex flex-col items-center justify-center gap-2 px-6 py-8 text-center',
          className,
        )}
        style={{ minHeight: 200 }}
      >
        <svg className="w-8 h-8 text-slate-600" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5}
            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
        <p className="text-sm font-medium text-slate-400">
          Logs purged on {fmtDateShort(filesPurgedAt)}
        </p>
        <p className="text-xs text-slate-600 max-w-xs">
          The on-disk log files for this build were removed by the retention policy.
          The DB record is still available.
        </p>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className={clsx('rounded-lg overflow-hidden border border-border', className)}
      style={{ minHeight: 200 }}
    />
  );
}
