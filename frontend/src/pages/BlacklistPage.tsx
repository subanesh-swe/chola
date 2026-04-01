import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  listCommandBlacklist,
  createCommandBlacklist,
  updateCommandBlacklist,
  deleteCommandBlacklist,
  listBranchBlacklist,
  createBranchBlacklist,
  updateBranchBlacklist,
  deleteBranchBlacklist,
} from '../api/blacklist';
import { listRepos } from '../api/repos';
import { listWorkers } from '../api/workers';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { EmptyState } from '../components/ui/EmptyState';
import { PageSkeleton } from '../components/ui/PageSkeleton';
import { TimeAgo } from '../components/ui/TimeAgo';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';
import type { CommandBlacklistEntry, BranchBlacklistEntry, MutationError } from '../types';

// ── Regex test preview ────────────────────────────────────────────────────────

function RegexPreview({ pattern }: { pattern: string }) {
  const [sample, setSample] = useState('');

  let matches: boolean | null = null;
  let error = '';
  if (pattern && sample) {
    try {
      matches = new RegExp(pattern).test(sample);
    } catch {
      error = 'Invalid regex';
    }
  }

  return (
    <div className="mt-2 space-y-1">
      <label className="block text-xs text-slate-500">Test sample input</label>
      <input
        value={sample}
        onChange={(e) => setSample(e.target.value)}
        placeholder="Type a command or branch to test..."
        className="w-full px-2 py-1.5 text-xs bg-slate-800 border border-slate-600 rounded text-white focus:outline-none focus:ring-1 focus:ring-blue-500"
      />
      {error && <p className="text-xs text-red-400">{error}</p>}
      {matches === true && <p className="text-xs text-green-400">Pattern matches</p>}
      {matches === false && <p className="text-xs text-slate-500">No match</p>}
    </div>
  );
}

// ── Edit modal ────────────────────────────────────────────────────────────────

interface EditModalProps {
  entry: CommandBlacklistEntry | BranchBlacklistEntry;
  onClose: () => void;
  onSave: (data: { pattern: string; description: string; enabled: boolean }) => void;
  isPending: boolean;
}

function EditModal({ entry, onClose, onSave, isPending }: EditModalProps) {
  const [pattern, setPattern] = useState(entry.pattern);
  const [description, setDescription] = useState(entry.description ?? '');
  const [enabled, setEnabled] = useState(entry.enabled);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
        <h3 className="text-lg font-semibold text-white mb-4">Edit Rule</h3>
        <div className="space-y-3">
          <div>
            <label className="block text-sm text-slate-300 mb-1">Pattern (regex)</label>
            <input
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="^rm -rf.*"
            />
            <RegexPreview pattern={pattern} />
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">Description</label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
              placeholder="Why this pattern is blocked..."
            />
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-blue-500 focus:ring-blue-500"
            />
            <span className="text-sm text-slate-300">Enabled</span>
          </label>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Cancel
          </button>
          <button
            onClick={() => onSave({ pattern, description, enabled })}
            disabled={!pattern || isPending}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Command Blacklist Tab ─────────────────────────────────────────────────────

function CommandBlacklistTab({ canManage }: { canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [editEntry, setEditEntry] = useState<CommandBlacklistEntry | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  // Add form state
  const [pattern, setPattern] = useState('');
  const [description, setDescription] = useState('');
  const [repoId, setRepoId] = useState('');
  const [stageId, setStageId] = useState('');

  const { data, isLoading, isError } = useQuery({
    queryKey: ['blacklist-commands'],
    queryFn: () => listCommandBlacklist(),
  });

  const { data: reposData } = useQuery({
    queryKey: ['repos'],
    queryFn: () => listRepos({ limit: 100 }),
  });

  const createMut = useMutation({
    mutationFn: () =>
      createCommandBlacklist({
        pattern,
        description: description || undefined,
        repo_id: repoId || undefined,
        stage_config_id: stageId || undefined,
      }),
    onSuccess: () => {
      toast.success('Rule created');
      qc.invalidateQueries({ queryKey: ['blacklist-commands'] });
      setShowAdd(false);
      setPattern('');
      setDescription('');
      setRepoId('');
      setStageId('');
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create rule'),
  });

  const updateMut = useMutation({
    mutationFn: ({
      id,
      data,
    }: {
      id: string;
      data: { pattern?: string; description?: string; enabled?: boolean };
    }) => updateCommandBlacklist(id, data),
    onSuccess: () => {
      toast.success('Rule updated');
      qc.invalidateQueries({ queryKey: ['blacklist-commands'] });
      setEditEntry(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update rule'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteCommandBlacklist(id),
    onSuccess: () => {
      toast.success('Rule deleted');
      qc.invalidateQueries({ queryKey: ['blacklist-commands'] });
      setDeleteId(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to delete rule'),
  });

  const repos = reposData?.data ?? [];
  const entries = data?.entries ?? [];

  const scopeLabel = (e: CommandBlacklistEntry) => {
    if (e.stage_config_id) return `Stage: ${e.stage_config_id.slice(0, 8)}`;
    if (e.repo_id) {
      const repo = repos.find((r) => r.id === e.repo_id);
      return `Repo: ${repo?.repo_name ?? e.repo_id.slice(0, 8)}`;
    }
    return 'Global';
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-slate-400">
          {entries.length} rule{entries.length !== 1 ? 's' : ''} defined
        </p>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Add Rule
          </button>
        )}
      </div>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400 text-sm">
          Failed to load command blacklist.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
          <table className="w-full" aria-label="Command blacklist rules">
            <thead>
              <tr className="border-b border-slate-700">
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Pattern</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Scope</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Description</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
                {canManage && (
                  <th className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase text-center">Actions</th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800">
              {entries.map((e) => (
                <tr key={e.id}>
                  <td className="px-4 py-3 text-sm text-slate-200 font-mono max-w-xs truncate">{e.pattern}</td>
                  <td className="px-4 py-3 text-sm text-slate-400">{scopeLabel(e)}</td>
                  <td className="px-4 py-3 text-sm text-slate-400 max-w-xs truncate">{e.description ?? '—'}</td>
                  <td className="px-4 py-3">
                    {canManage ? (
                      <button
                        onClick={() =>
                          updateMut.mutate({ id: e.id, data: { enabled: !e.enabled } })
                        }
                        className={`text-xs px-2 py-0.5 rounded border transition-colors focus:outline-none focus:ring-1 ${
                          e.enabled
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20'
                            : 'bg-slate-700 text-slate-400 border-slate-600 hover:bg-slate-600'
                        }`}
                        aria-label={`Toggle rule ${e.id}`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </button>
                    ) : (
                      <span
                        className={`text-xs px-2 py-0.5 rounded border ${
                          e.enabled
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-slate-700 text-slate-400 border-slate-600'
                        }`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-sm text-slate-500">
                    <TimeAgo date={e.created_at} />
                  </td>
                  {canManage && (
                    <td className="px-4 py-3 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <button
                          onClick={() => setEditEntry(e)}
                          className="text-xs text-blue-400 hover:text-blue-300 focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                        >
                          Edit
                        </button>
                        <button
                          onClick={() => setDeleteId(e.id)}
                          className="text-xs text-red-400 hover:text-red-300 focus:outline-none focus:ring-1 focus:ring-red-500 rounded"
                        >
                          Delete
                        </button>
                      </div>
                    </td>
                  )}
                </tr>
              ))}
              {!entries.length && (
                <tr>
                  <td colSpan={canManage ? 6 : 5} className="px-4 py-8 text-center">
                    <EmptyState
                      message="No command blacklist rules"
                      description="Add patterns to block forbidden commands."
                    />
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {/* Add rule modal */}
      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-white mb-4">Add Command Rule</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-slate-300 mb-1">Pattern (regex)</label>
                <input
                  value={pattern}
                  onChange={(e) => setPattern(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  placeholder="^rm -rf.*"
                />
                <RegexPreview pattern={pattern} />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Scope — Repo (optional)</label>
                <select
                  value={repoId}
                  onChange={(e) => setRepoId(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  <option value="">Global (all repos)</option>
                  {repos.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.repo_name}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Stage Config ID (optional)</label>
                <input
                  value={stageId}
                  onChange={(e) => setStageId(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  placeholder="UUID of stage config"
                />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  rows={2}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
                  placeholder="Why this pattern is blocked..."
                />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAdd(false)}
                className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Cancel
              </button>
              <button
                onClick={() => createMut.mutate()}
                disabled={!pattern || createMut.isPending}
                className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {editEntry && (
        <EditModal
          entry={editEntry}
          onClose={() => setEditEntry(null)}
          onSave={(d) => updateMut.mutate({ id: editEntry.id, data: d })}
          isPending={updateMut.isPending}
        />
      )}

      <ConfirmDialog
        open={deleteId !== null}
        title="Delete Rule"
        message="This blacklist rule will be permanently removed."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteId && deleteMut.mutate(deleteId)}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}

// ── Branch Blacklist Tab ──────────────────────────────────────────────────────

function BranchBlacklistTab({ canManage }: { canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [editEntry, setEditEntry] = useState<BranchBlacklistEntry | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  // Add form state
  const [workerId, setWorkerId] = useState('');
  const [pattern, setPattern] = useState('');
  const [description, setDescription] = useState('');

  const { data, isLoading, isError } = useQuery({
    queryKey: ['blacklist-branches'],
    queryFn: () => listBranchBlacklist(),
  });

  const { data: workersData } = useQuery({
    queryKey: ['workers'],
    queryFn: () => listWorkers(),
  });

  const createMut = useMutation({
    mutationFn: () =>
      createBranchBlacklist({
        worker_id: workerId,
        pattern,
        description: description || undefined,
      }),
    onSuccess: () => {
      toast.success('Rule created');
      qc.invalidateQueries({ queryKey: ['blacklist-branches'] });
      setShowAdd(false);
      setWorkerId('');
      setPattern('');
      setDescription('');
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create rule'),
  });

  const updateMut = useMutation({
    mutationFn: ({
      id,
      data,
    }: {
      id: string;
      data: { pattern?: string; description?: string; enabled?: boolean };
    }) => updateBranchBlacklist(id, data),
    onSuccess: () => {
      toast.success('Rule updated');
      qc.invalidateQueries({ queryKey: ['blacklist-branches'] });
      setEditEntry(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update rule'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteBranchBlacklist(id),
    onSuccess: () => {
      toast.success('Rule deleted');
      qc.invalidateQueries({ queryKey: ['blacklist-branches'] });
      setDeleteId(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to delete rule'),
  });

  const workers = workersData?.data ?? [];
  const entries = data?.entries ?? [];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-slate-400">
          {entries.length} rule{entries.length !== 1 ? 's' : ''} defined
        </p>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Add Rule
          </button>
        )}
      </div>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400 text-sm">
          Failed to load branch blacklist.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
          <table className="w-full" aria-label="Branch blacklist rules">
            <thead>
              <tr className="border-b border-slate-700">
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Worker</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Pattern</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Description</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
                {canManage && (
                  <th className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase text-center">Actions</th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800">
              {entries.map((e) => (
                <tr key={e.id}>
                  <td className="px-4 py-3 text-sm text-slate-200 font-mono truncate max-w-xs">{e.worker_id}</td>
                  <td className="px-4 py-3 text-sm text-slate-200 font-mono truncate max-w-xs">{e.pattern}</td>
                  <td className="px-4 py-3 text-sm text-slate-400 max-w-xs truncate">{e.description ?? '—'}</td>
                  <td className="px-4 py-3">
                    {canManage ? (
                      <button
                        onClick={() =>
                          updateMut.mutate({ id: e.id, data: { enabled: !e.enabled } })
                        }
                        className={`text-xs px-2 py-0.5 rounded border transition-colors focus:outline-none focus:ring-1 ${
                          e.enabled
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20'
                            : 'bg-slate-700 text-slate-400 border-slate-600 hover:bg-slate-600'
                        }`}
                        aria-label={`Toggle rule ${e.id}`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </button>
                    ) : (
                      <span
                        className={`text-xs px-2 py-0.5 rounded border ${
                          e.enabled
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-slate-700 text-slate-400 border-slate-600'
                        }`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-sm text-slate-500">
                    <TimeAgo date={e.created_at} />
                  </td>
                  {canManage && (
                    <td className="px-4 py-3 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <button
                          onClick={() => setEditEntry(e)}
                          className="text-xs text-blue-400 hover:text-blue-300 focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                        >
                          Edit
                        </button>
                        <button
                          onClick={() => setDeleteId(e.id)}
                          className="text-xs text-red-400 hover:text-red-300 focus:outline-none focus:ring-1 focus:ring-red-500 rounded"
                        >
                          Delete
                        </button>
                      </div>
                    </td>
                  )}
                </tr>
              ))}
              {!entries.length && (
                <tr>
                  <td colSpan={canManage ? 6 : 5} className="px-4 py-8 text-center">
                    <EmptyState
                      message="No branch blacklist rules"
                      description="Add patterns to restrict branches on specific workers."
                    />
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {/* Add rule modal */}
      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-white mb-4">Add Branch Rule</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-slate-300 mb-1">Worker</label>
                {workers.length > 0 ? (
                  <select
                    value={workerId}
                    onChange={(e) => setWorkerId(e.target.value)}
                    className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-blue-500"
                  >
                    <option value="">Select a worker...</option>
                    {workers.map((w) => (
                      <option key={w.worker_id} value={w.worker_id}>
                        {w.worker_id}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    value={workerId}
                    onChange={(e) => setWorkerId(e.target.value)}
                    className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="worker-id"
                  />
                )}
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Pattern (regex)</label>
                <input
                  value={pattern}
                  onChange={(e) => setPattern(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  placeholder="^release/.*"
                />
                <RegexPreview pattern={pattern} />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  rows={2}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
                  placeholder="Why this branch pattern is blocked on this worker..."
                />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAdd(false)}
                className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Cancel
              </button>
              <button
                onClick={() => createMut.mutate()}
                disabled={!workerId || !pattern || createMut.isPending}
                className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {editEntry && (
        <EditModal
          entry={editEntry}
          onClose={() => setEditEntry(null)}
          onSave={(d) => updateMut.mutate({ id: editEntry.id, data: d })}
          isPending={updateMut.isPending}
        />
      )}

      <ConfirmDialog
        open={deleteId !== null}
        title="Delete Rule"
        message="This blacklist rule will be permanently removed."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteId && deleteMut.mutate(deleteId)}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

type Tab = 'commands' | 'branches';

export default function BlacklistPage() {
  const [activeTab, setActiveTab] = useState<Tab>('commands');
  const { canManageRepos } = usePermission();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-white">Blacklist</h2>
          <p className="text-sm text-slate-400 mt-1">
            Manage forbidden command and branch patterns for CI runs.
          </p>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-slate-700">
        <nav className="flex gap-1" aria-label="Blacklist tabs">
          {(['commands', 'branches'] as Tab[]).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2.5 text-sm font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 rounded-t-lg ${
                activeTab === tab
                  ? 'text-blue-400 border-b-2 border-blue-500 -mb-px'
                  : 'text-slate-400 hover:text-white'
              }`}
              aria-selected={activeTab === tab}
              role="tab"
            >
              {tab === 'commands' ? 'Command Rules' : 'Branch Rules'}
            </button>
          ))}
        </nav>
      </div>

      {activeTab === 'commands' && <CommandBlacklistTab canManage={canManageRepos} />}
      {activeTab === 'branches' && <BranchBlacklistTab canManage={canManageRepos} />}
    </div>
  );
}
