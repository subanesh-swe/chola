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
      <label className="block text-xs text-muted">Test sample input</label>
      <input
        value={sample}
        onChange={(e) => setSample(e.target.value)}
        placeholder="Type a command or branch to test..."
        className="w-full px-2 py-1.5 text-xs bg-input border border-border rounded text-primary focus:outline-none focus:ring-1 focus:ring-accent"
      />
      {error && <p className="text-xs text-danger">{error}</p>}
      {matches === true && <p className="text-xs text-success">Pattern matches</p>}
      {matches === false && <p className="text-xs text-muted">No match</p>}
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
      <div className="bg-surface border border-border rounded-xl p-6 max-w-md w-full">
        <h3 className="text-lg font-semibold text-primary mb-4">Edit Rule</h3>
        <div className="space-y-3">
          <div>
            <label className="block text-sm text-secondary mb-1">Pattern (regex)</label>
            <input
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
              placeholder="^rm -rf.*"
            />
            <RegexPreview pattern={pattern} />
          </div>
          <div>
            <label className="block text-sm text-secondary mb-1">Description</label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent resize-none"
              placeholder="Why this pattern is blocked..."
            />
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="w-4 h-4 rounded border-border bg-input accent-accent focus:ring-accent"
            />
            <span className="text-sm text-secondary">Enabled</span>
          </label>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Cancel
          </button>
          <button
            onClick={() => onSave({ pattern, description, enabled })}
            disabled={!pattern || isPending}
            className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
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
        <p className="text-sm text-muted">
          {entries.length} rule{entries.length !== 1 ? 's' : ''} defined
        </p>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-3 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Add Rule
          </button>
        )}
      </div>

      {isError && (
        <div role="alert" className="bg-danger-soft border border-danger/30 rounded-lg p-4 text-danger text-sm">
          Failed to load command blacklist.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="bg-surface border border-border rounded-xl overflow-hidden">
          <table className="w-full" aria-label="Command blacklist rules">
            <thead>
              <tr className="border-b border-border">
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Pattern</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Scope</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Description</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Status</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Created</th>
                {canManage && (
                  <th className="px-4 py-3 text-xs font-semibold text-muted uppercase text-center">Actions</th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {entries.map((e) => (
                <tr key={e.id}>
                  <td className="px-4 py-3 text-sm text-secondary font-mono max-w-xs truncate" title={e.pattern}>{e.pattern}</td>
                  <td className="px-4 py-3 text-sm text-muted">{scopeLabel(e)}</td>
                  <td className="px-4 py-3 text-sm text-muted max-w-xs truncate" title={e.description ?? undefined}>{e.description ?? '—'}</td>
                  <td className="px-4 py-3">
                    {canManage ? (
                      <button
                        onClick={() =>
                          updateMut.mutate({ id: e.id, data: { enabled: !e.enabled } })
                        }
                        className={`text-xs px-2 py-0.5 rounded border transition-colors focus:outline-none focus:ring-1 ${
                          e.enabled
                            ? 'bg-success-soft text-success border-success/30 hover:bg-success-soft'
                            : 'bg-surface-2 text-muted border-border hover:bg-surface-hover'
                        }`}
                        aria-label={`Toggle rule ${e.id}`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </button>
                    ) : (
                      <span
                        className={`text-xs px-2 py-0.5 rounded border ${
                          e.enabled
                            ? 'bg-success-soft text-success border-success/30'
                            : 'bg-surface-2 text-muted border-border'
                        }`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-sm text-disabled">
                    <TimeAgo date={e.created_at} />
                  </td>
                  {canManage && (
                    <td className="px-4 py-3 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <button
                          onClick={() => setEditEntry(e)}
                          className="text-xs text-accent-text hover:opacity-80 focus:outline-none focus:ring-1 focus:ring-accent rounded"
                        >
                          Edit
                        </button>
                        <button
                          onClick={() => setDeleteId(e.id)}
                          className="text-xs text-danger hover:text-danger focus:outline-none focus:ring-1 focus:ring-danger rounded"
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
                      title="No command blacklist rules"
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
          <div className="bg-surface border border-border rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-primary mb-4">Add Command Rule</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-secondary mb-1">Pattern (regex)</label>
                <input
                  value={pattern}
                  onChange={(e) => setPattern(e.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
                  placeholder="^rm -rf.*"
                />
                <RegexPreview pattern={pattern} />
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Scope — Repo (optional)</label>
                <select
                  value={repoId}
                  onChange={(e) => setRepoId(e.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary focus:outline-none focus:ring-2 focus:ring-accent"
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
                <label className="block text-sm text-secondary mb-1">Stage Config ID (optional)</label>
                <input
                  value={stageId}
                  onChange={(e) => setStageId(e.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
                  placeholder="UUID of stage config"
                />
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  rows={2}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent resize-none"
                  placeholder="Why this pattern is blocked..."
                />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAdd(false)}
                className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent"
              >
                Cancel
              </button>
              <button
                onClick={() => createMut.mutate()}
                disabled={!pattern || createMut.isPending}
                className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
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
        <p className="text-sm text-muted">
          {entries.length} rule{entries.length !== 1 ? 's' : ''} defined
        </p>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-3 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Add Rule
          </button>
        )}
      </div>

      {isError && (
        <div role="alert" className="bg-danger-soft border border-danger/30 rounded-lg p-4 text-danger text-sm">
          Failed to load branch blacklist.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="bg-surface border border-border rounded-xl overflow-hidden">
          <table className="w-full" aria-label="Branch blacklist rules">
            <thead>
              <tr className="border-b border-border">
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Worker</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Pattern</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Description</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Status</th>
                <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Created</th>
                {canManage && (
                  <th className="px-4 py-3 text-xs font-semibold text-muted uppercase text-center">Actions</th>
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {entries.map((e) => (
                <tr key={e.id}>
                  <td className="px-4 py-3 text-sm text-secondary font-mono truncate max-w-xs" title={e.worker_id}>{e.worker_id}</td>
                  <td className="px-4 py-3 text-sm text-secondary font-mono truncate max-w-xs" title={e.pattern}>{e.pattern}</td>
                  <td className="px-4 py-3 text-sm text-muted max-w-xs truncate" title={e.description ?? undefined}>{e.description ?? '—'}</td>
                  <td className="px-4 py-3">
                    {canManage ? (
                      <button
                        onClick={() =>
                          updateMut.mutate({ id: e.id, data: { enabled: !e.enabled } })
                        }
                        className={`text-xs px-2 py-0.5 rounded border transition-colors focus:outline-none focus:ring-1 ${
                          e.enabled
                            ? 'bg-success-soft text-success border-success/30 hover:bg-success-soft'
                            : 'bg-surface-2 text-muted border-border hover:bg-surface-hover'
                        }`}
                        aria-label={`Toggle rule ${e.id}`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </button>
                    ) : (
                      <span
                        className={`text-xs px-2 py-0.5 rounded border ${
                          e.enabled
                            ? 'bg-success-soft text-success border-success/30'
                            : 'bg-surface-2 text-muted border-border'
                        }`}
                      >
                        {e.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-sm text-disabled">
                    <TimeAgo date={e.created_at} />
                  </td>
                  {canManage && (
                    <td className="px-4 py-3 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <button
                          onClick={() => setEditEntry(e)}
                          className="text-xs text-accent-text hover:opacity-80 focus:outline-none focus:ring-1 focus:ring-accent rounded"
                        >
                          Edit
                        </button>
                        <button
                          onClick={() => setDeleteId(e.id)}
                          className="text-xs text-danger hover:text-danger focus:outline-none focus:ring-1 focus:ring-danger rounded"
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
                      title="No branch blacklist rules"
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
          <div className="bg-surface border border-border rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-primary mb-4">Add Branch Rule</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-secondary mb-1">Worker</label>
                {workers.length > 0 ? (
                  <select
                    value={workerId}
                    onChange={(e) => setWorkerId(e.target.value)}
                    className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary focus:outline-none focus:ring-2 focus:ring-accent"
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
                    className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
                    placeholder="worker-id"
                  />
                )}
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Pattern (regex)</label>
                <input
                  value={pattern}
                  onChange={(e) => setPattern(e.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
                  placeholder="^release/.*"
                />
                <RegexPreview pattern={pattern} />
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  rows={2}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent resize-none"
                  placeholder="Why this branch pattern is blocked on this worker..."
                />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button
                onClick={() => setShowAdd(false)}
                className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent"
              >
                Cancel
              </button>
              <button
                onClick={() => createMut.mutate()}
                disabled={!workerId || !pattern || createMut.isPending}
                className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
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
          <h2 className="text-2xl font-bold text-primary">Blacklist</h2>
          <p className="text-sm text-muted mt-1">
            Manage forbidden command and branch patterns for CI runs.
          </p>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-border">
        <nav className="flex gap-1" aria-label="Blacklist tabs">
          {(['commands', 'branches'] as Tab[]).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2.5 text-sm font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-accent rounded-t-lg ${
                activeTab === tab
                  ? 'text-accent-text border-b-2 border-accent -mb-px'
                  : 'text-muted hover:text-primary'
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
