import { useState, useEffect, useRef, type KeyboardEvent } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listRepos, createRepo, deleteRepo } from '../api/repos';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';

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
      <div ref={dialogRef} className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
        <h3 id="add-repo-title" className="text-lg font-semibold text-white mb-4">Add Repository</h3>
        <div className="space-y-3">
          <div>
            <label htmlFor="repo-name" className="block text-sm text-slate-300 mb-1">Name</label>
            <input
              id="repo-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="my-service"
            />
          </div>
          <div>
            <label htmlFor="repo-url" className="block text-sm text-slate-300 mb-1">URL</label>
            <input
              id="repo-url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="git@github.com:org/repo.git"
            />
          </div>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-slate-300 bg-slate-800 hover:bg-slate-700 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={() => onSubmit(name, url)}
            disabled={!name || !url || pending}
            className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-blue-500 transition-colors"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}

export default function ReposPage() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();
  const [showAdd, setShowAdd] = useState(false);
  const [delId, setDelId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({ queryKey: ['repos'], queryFn: () => listRepos() });

  const addMut = useMutation({
    mutationFn: ({ name, url }: { name: string; url: string }) =>
      createRepo({ repo_name: name, repo_url: url }),
    onSuccess: () => {
      toast.success('Repo created');
      qc.invalidateQueries({ queryKey: ['repos'] });
      setShowAdd(false);
    },
    onError: () => toast.error('Failed to create repo'),
  });

  const delMut = useMutation({
    mutationFn: (id: string) => deleteRepo(id),
    onSuccess: () => {
      toast.success('Repo deleted');
      qc.invalidateQueries({ queryKey: ['repos'] });
      setDelId(null);
    },
  });

  const repos = data?.data ?? [];

  const handleRowKey = (e: KeyboardEvent<HTMLTableRowElement>, id: string) => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); nav(`/repos/${id}`); }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-2xl font-bold text-white">Repositories ({repos.length})</h2>
        {canManageRepos && (
          <button
            onClick={() => setShowAdd(true)}
            aria-label="Add repository"
            className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Add Repo
          </button>
        )}
      </div>

      {isLoading ? (
        <div className="text-slate-400" role="status" aria-live="polite">Loading...</div>
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
                {repos.map((r) => (
                  <tr
                    key={r.id}
                    onClick={() => nav(`/repos/${r.id}`)}
                    onKeyDown={(e) => handleRowKey(e, r.id)}
                    tabIndex={0}
                    className="hover:bg-slate-800/50 transition-colors cursor-pointer focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-500"
                    aria-label={`Repository ${r.repo_name}`}
                  >
                    <td className="px-4 py-3 text-sm text-blue-400 hover:underline">{r.repo_name}</td>
                    <td className="px-4 py-3 text-sm text-slate-400 font-mono truncate max-w-xs">{r.repo_url}</td>
                    <td className="px-4 py-3 text-sm text-slate-300">{r.default_branch}</td>
                    <td className="px-4 py-3"><StatusBadge status={r.enabled ? 'Connected' : 'Disconnected'} /></td>
                    <td className="px-4 py-3 text-sm"><TimeAgo date={r.created_at} className="text-slate-500" /></td>
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
                ))}
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
