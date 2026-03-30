import { useEffect, useRef } from 'react';
import { clsx } from 'clsx';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface Props {
  content?: string;
  liveChunks?: string[];
  className?: string;
}

export function LogViewer({ content, liveChunks, className }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const writtenRef = useRef(0);

  useEffect(() => {
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
      scrollback: 10000,
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
  }, [content]);

  useEffect(() => {
    if (!terminalRef.current || !liveChunks) return;
    for (let i = writtenRef.current; i < liveChunks.length; i++) {
      terminalRef.current.write(liveChunks[i]);
    }
    writtenRef.current = liveChunks.length;
  }, [liveChunks]);

  return (
    <div
      ref={containerRef}
      className={clsx('rounded-lg overflow-hidden border border-slate-700', className)}
      style={{ minHeight: 200 }}
    />
  );
}
