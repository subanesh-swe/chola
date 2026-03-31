import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listRepos, createRepo, deleteRepo } from '../api/repos';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';

export default function ReposPage() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();
  const [showAdd, setShowAdd] = useState(false);
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [delId, setDelId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({ queryKey: ['repos'], queryFn: listRepos });

  const addMut = useMutation({
    mutationFn: () => createRepo({ repo_name: name, repo_url: url }),
    onSuccess: () => { toast.success('Repo created'); qc.invalidateQueries({ queryKey: ['repos'] }); setShowAdd(false); setName(''); setUrl(''); },
    onError: () => toast.error('Failed to create repo'),
  });

  const delMut = useMutation({
    mutationFn: (id: string) => deleteRepo(id),
    onSuccess: () => { toast.success('Repo deleted'); qc.invalidateQueries({ queryKey: ['repos'] }); setDelId(null); },
  });

  const repos = data?.data ?? [];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">Repositories ({repos.length})</h2>
        {canManageRepos && (
          <button onClick={() => setShowAdd(true)} className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors">Add Repo</button>
        )}
      </div>

      {isLoading ? <div className="text-slate-400">Loading...</div> : (
        <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
          <table className="w-full">
            <thead><tr className="border-b border-slate-700">
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Name</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">URL</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Branch</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
              {canManageRepos && <th className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase">Actions</th>}
            </tr></thead>
            <tbody className="divide-y divide-slate-800">
              {repos.map(r => (
                <tr key={r.id} className="hover:bg-slate-800/50 transition-colors">
                  <td className="px-4 py-3 text-sm text-blue-400 cursor-pointer hover:underline" onClick={() => nav(`/repos/${r.id}`)}>{r.repo_name}</td>
                  <td className="px-4 py-3 text-sm text-slate-400 font-mono truncate max-w-xs">{r.repo_url}</td>
                  <td className="px-4 py-3 text-sm text-slate-300">{r.default_branch}</td>
                  <td className="px-4 py-3"><StatusBadge status={r.enabled ? 'Connected' : 'Disconnected'} /></td>
                  <td className="px-4 py-3 text-sm"><TimeAgo date={r.created_at} className="text-slate-500" /></td>
                  {canManageRepos && (
                    <td className="px-4 py-3 text-center">
                      <button onClick={() => setDelId(r.id)} className="text-xs text-red-400 hover:text-red-300">Delete</button>
                    </td>
                  )}
                </tr>
              ))}
              {!repos.length && <tr><td colSpan={6} className="px-4 py-8 text-center text-slate-500">No repositories configured</td></tr>}
            </tbody>
          </table>
        </div>
      )}

      {/* Add Repo Dialog */}
      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-4">Add Repository</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-slate-300 mb-1">Name</label>
                <input value={name} onChange={e => setName(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" placeholder="my-service" />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">URL</label>
                <input value={url} onChange={e => setUrl(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" placeholder="git@github.com:org/repo.git" />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg">Cancel</button>
              <button onClick={() => addMut.mutate()} disabled={!name || !url} className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50">Create</button>
            </div>
          </div>
        </div>
      )}

      <ConfirmDialog open={!!delId} title="Delete Repository" message="This will remove the repository and its stage configs." confirmLabel="Delete" variant="danger" onConfirm={() => delId && delMut.mutate(delId)} onCancel={() => setDelId(null)} />
    </div>
  );
}
