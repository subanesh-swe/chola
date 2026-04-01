import { useEffect, useRef, useState } from 'react';
import { useQueries } from '@tanstack/react-query';
import { listBuilds } from '../../api/builds';
import { listWorkers } from '../../api/workers';
import { listRepos } from '../../api/repos';
import { SearchResult, type SearchResultItem } from './SearchResult';

function useDebounce(value: string, ms: number) {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return debounced;
}

interface Props {
  open: boolean;
  onClose: () => void;
}

export function SearchDialog({ open, onClose }: Props) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const q = useDebounce(query.trim().toLowerCase(), 300);

  useEffect(() => {
    if (open) {
      setQuery('');
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  const [buildsQ, workersQ, reposQ] = useQueries({
    queries: [
      { queryKey: ['builds-all'], queryFn: () => listBuilds({ limit: 200 }), staleTime: 30000 },
      { queryKey: ['workers'], queryFn: listWorkers, staleTime: 10000 },
      { queryKey: ['repos'], queryFn: listRepos, staleTime: 60000 },
    ],
  });

  const results: SearchResultItem[] = q.length < 2 ? [] : [
    ...(buildsQ.data?.job_groups ?? [])
      .filter(b => b.job_group_id.includes(q) || b.branch?.toLowerCase().includes(q) || b.commit_sha?.toLowerCase().startsWith(q))
      .slice(0, 5)
      .map(b => ({
        type: 'build' as const,
        id: b.job_group_id,
        title: b.job_group_id.slice(0, 8),
        subtitle: `${b.branch ?? 'no branch'} · ${b.state}`,
        href: `/builds/${b.job_group_id}`,
      })),
    ...(reposQ.data?.repos ?? [])
      .filter(r => r.repo_name.toLowerCase().includes(q) || r.id.includes(q))
      .slice(0, 5)
      .map(r => ({
        type: 'repo' as const,
        id: r.id,
        title: r.repo_name,
        subtitle: r.repo_url,
        href: `/repos/${r.id}`,
      })),
    ...(workersQ.data?.workers ?? [])
      .filter(w => w.worker_id.toLowerCase().includes(q) || w.hostname.toLowerCase().includes(q))
      .slice(0, 5)
      .map(w => ({
        type: 'worker' as const,
        id: w.worker_id,
        title: w.worker_id,
        subtitle: `${w.hostname} · ${w.status}`,
        href: `/workers`,
      })),
  ];

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh]" onClick={onClose}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <div
        className="relative w-full max-w-lg bg-slate-900 border border-slate-700 rounded-xl shadow-2xl overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700">
          <svg className="w-4 h-4 text-slate-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <input
            ref={inputRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Search builds, repos, workers..."
            className="flex-1 bg-transparent text-white placeholder-slate-500 outline-none text-sm"
          />
          <kbd className="text-[10px] text-slate-600 border border-slate-700 rounded px-1.5 py-0.5">esc</kbd>
        </div>
        <div className="max-h-80 overflow-y-auto p-2">
          {q.length < 2 && (
            <p className="text-center text-xs text-slate-600 py-6">Type at least 2 characters to search</p>
          )}
          {q.length >= 2 && results.length === 0 && (
            <p className="text-center text-xs text-slate-500 py-6">No results for "{q}"</p>
          )}
          {results.map(item => (
            <SearchResult key={`${item.type}-${item.id}`} item={item} onClose={onClose} />
          ))}
        </div>
      </div>
    </div>
  );
}
