import { useState, useEffect, useRef } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { listRepos, createRepo, deleteRepo } from '../api/repos';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { EmptyState } from '../components/ui/EmptyState';
import { TableSkeleton } from '../components/ui/PageSkeleton';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';
import type { MutationError } from '../types';

const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

function AddRepoDialog({
  onClose,
  onSubmit,
  pending,
}: {
  onClose: () => void;
  onSubmit: (name: string, url: string) => void;
  pending: boolean;
}) {
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const dialogRef = useRef<HTMLDivElement>(null);

  // Escape closes
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [onClose]);

  // Focus trap
  useEffect(() => {
    if (!dialogRef.current) return;
    const el = dialogRef.current;
    const focusable = Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE));
    focusable[0]?.focus();

    const trap = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last.focus(); }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    document.addEventListener('keydown', trap);
    return () => document.removeEventListener('keydown', trap);
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="add-repo-title"
    >
      <div ref={dialogRef} className="bg-surface border border-border rounded-xl p-6 max-w-md w-full">
        <h3 id="add-repo-title" className="text-lg font-semibold text-primary mb-4">Add Repository</h3>
        <div className="space-y-3">
          <div>
            <label htmlFor="repo-name" className="block text-sm text-secondary mb-1">Name</label>
            <input
              id="repo-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary focus:outline-none focus:ring-2 focus:ring-accent"
              placeholder="my-service"
            />
          </div>
          <div>
            <label htmlFor="repo-url" className="block text-sm text-secondary mb-1">URL</label>
            <input
              id="repo-url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary focus:outline-none focus:ring-2 focus:ring-accent"
              placeholder="git@github.com:org/repo.git"
            />
          </div>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-secondary bg-surface-2 hover:bg-surface-hover rounded-lg focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={() => onSubmit(name, url)}
            disabled={!name || !url || pending}
            className="px-4 py-2 text-sm bg-accent hover:bg-accent-hover text-on-accent rounded-lg disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}

export default function ReposPage() {
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();
  const [showAdd, setShowAdd] = useState(false);
  const [delId, setDelId] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({ queryKey: ['repos'], queryFn: () => listRepos() });

  const addMut = useMutation({
    mutationFn: ({ name, url }: { name: string; url: string }) =>
      createRepo({ repo_name: name, repo_url: url }),
    onSuccess: () => {
      toast.success('Repo created');
      qc.invalidateQueries({ queryKey: ['repos'] });
      setShowAdd(false);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to create repo'),
  });

  const delMut = useMutation({
    mutationFn: (id: string) => deleteRepo(id),
    onSuccess: () => {
      toast.success('Repo deleted');
      qc.invalidateQueries({ queryKey: ['repos'] });
      setDelId(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to delete repo'),
  });

  const repos = data?.data ?? [];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-primary">Repositories ({repos.length})</h2>
        {canManageRepos && (
          <button
            onClick={() => setShowAdd(true)}
            aria-label="Add repository"
            className="px-4 py-2 text-sm bg-accent hover:bg-accent-hover text-on-accent rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Add Repo
          </button>
        )}
      </div>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load repositories. Please try again.
        </div>
      )}
      {/* item 30: skeleton instead of plain text */}
      {isLoading ? (
        <TableSkeleton rows={4} cols={5} />
      ) : (
        <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full" aria-label="Repositories">
              <thead>
                <tr className="border-b border-slate-700">
                  <th scope="col" className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Name</th>
                  <th scope="col" className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">URL</th>
                  <th scope="col" className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Branch</th>
                  <th scope="col" className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
                  <th scope="col" className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
                  {canManageRepos && <th scope="col" className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase">Actions</th>}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800">
                {repos.map((r) => {
                  const href = `/repos/${r.id}`;
                  return (
                    <tr
                      key={r.id}
                      className="hover:bg-slate-800/50 transition-colors"
                    >
                      <td className="p-0">
                        <Link to={href} className="block px-4 py-3 text-sm text-blue-400 hover:underline focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                          {r.repo_name}
                        </Link>
                      </td>
                      <td className="p-0">
                        <Link to={href} className="block px-4 py-3 text-sm text-slate-400 font-mono truncate max-w-xs focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                          {r.repo_url}
                        </Link>
                      </td>
                      <td className="p-0">
                        <Link to={href} className="block px-4 py-3 text-sm text-slate-300 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                          {r.default_branch}
                        </Link>
                      </td>
                      <td className="p-0">
                        <Link to={href} className="block px-4 py-3 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                          <StatusBadge status={r.enabled ? 'Connected' : 'Disconnected'} />
                        </Link>
                      </td>
                      <td className="p-0">
                        <Link to={href} className="block px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500">
                          <TimeAgo date={r.created_at} className="text-slate-500" />
                        </Link>
                      </td>
                      {canManageRepos && (
                        <td className="px-4 py-3 text-center">
                          <button
                            onClick={(e) => { e.stopPropagation(); setDelId(r.id); }}
                            aria-label={`Delete repository ${r.repo_name}`}
                            className="text-xs text-red-400 hover:text-red-300 focus:outline-none focus:ring-2 focus:ring-red-500 rounded"
                          >
                            Delete
                          </button>
                        </td>
                      )}
                    </tr>
                  );
                })}
                {!repos.length && (
                  <tr>
                    <td colSpan={canManageRepos ? 6 : 5} className="px-4 py-8 text-center text-slate-500">
                      No repositories configured
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {showAdd && (
        <AddRepoDialog
          onClose={() => setShowAdd(false)}
          onSubmit={(name, url) => addMut.mutate({ name, url })}
          pending={addMut.isPending}
        />
      )}

      <ConfirmDialog
        open={!!delId}
        title="Delete Repository"
        message="This will remove the repository and its stage configs."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => delId && delMut.mutate(delId)}
        onCancel={() => setDelId(null)}
      />
    </div>
  );
}
