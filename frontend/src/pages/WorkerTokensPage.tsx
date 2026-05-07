import { useState } from 'react';
import { formatNumber } from '../utils/format';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  listWorkerTokens,
  createWorkerToken,
  activateWorkerToken,
  deactivateWorkerToken,
  deleteWorkerToken,
} from '../api/workerTokens';
import type { WorkerToken, CreatedWorkerToken } from '../api/workerTokens';
import { listWorkers } from '../api/workers';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { EmptyState } from '../components/ui/EmptyState';
import { PageSkeleton } from '../components/ui/PageSkeleton';
import { TimeAgo } from '../components/ui/TimeAgo';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';
import type { MutationError } from '../types';

// ── Scope badge ───────────────────────────────────────────────────────────────

function ScopeBadge({ scope }: { scope: string }) {
  const colors: Record<string, string> = {
    shared: 'bg-accent-soft text-accent-text border-accent/30',
    project: 'bg-purple-500/20 text-purple-400 border-purple-500/30',
    team: 'bg-indigo-500/20 text-indigo-400 border-indigo-500/30',
    runner: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  };
  const cls = colors[scope] ?? 'bg-surface-2 text-muted border-border';
  return (
    <span className={`text-xs px-1.5 py-0.5 rounded border ${cls}`}>{scope}</span>
  );
}

// ── Created token modal — shows plaintext once ────────────────────────────────

function CreatedTokenModal({
  token,
  onClose,
}: {
  token: CreatedWorkerToken;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  function copy() {
    navigator.clipboard.writeText(token.token).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div className="bg-surface border border-border rounded-xl p-6 max-w-lg w-full">
        <div className="flex items-center gap-2 mb-2">
          <span className="text-emerald-400 text-lg font-bold">Token created</span>
        </div>
        <p className="text-sm text-yellow-400 mb-4">
          Copy this token now — it will not be shown again.
        </p>
        <div className="bg-surface-2 border border-border rounded-lg p-3 mb-1 flex items-center gap-2">
          <code className="text-emerald-300 font-mono text-xs break-all flex-1 select-all cursor-text">
            {token.token}
          </code>
          <button
            onClick={copy}
            className="shrink-0 px-2 py-1 text-xs bg-accent-soft text-accent-text border border-accent/30 rounded hover:opacity-80 focus:outline-none focus:ring-1 focus:ring-accent"
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <p className="text-[11px] text-muted mb-4">Click to select all. Save now — you cannot retrieve it later.</p>
        <dl className="text-sm space-y-1 mb-4">
          <div className="flex gap-2">
            <dt className="text-muted w-24">Name</dt>
            <dd className="text-secondary">{token.name}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="text-muted w-24">Scope</dt>
            <dd><ScopeBadge scope={token.scope} /></dd>
          </div>
          {token.expires_at && (
            <div className="flex gap-2">
              <dt className="text-muted w-24">Expires</dt>
              <dd className="text-secondary"><TimeAgo date={token.expires_at} /></dd>
            </div>
          )}
          <div className="flex gap-2">
            <dt className="text-muted w-24">Max uses</dt>
            <dd className="text-secondary">{token.max_uses === 0 ? 'Unlimited' : token.max_uses}</dd>
          </div>
        </dl>
        <div className="bg-surface-2 border border-border rounded-lg p-3 mb-4 text-xs space-y-2">
          {token.scope === 'runner' ? (
            <>
              <p className="text-secondary font-medium">Set environment variable for ci-job-runner:</p>
              <code className="block text-emerald-300 font-mono">
                export CHOLA_TOKEN={token.token}
              </code>
            </>
          ) : (
            <>
              <p className="text-secondary font-medium">Add to worker config file:</p>
              <code className="block text-emerald-300 font-mono">
                token: &quot;{token.token}&quot;
              </code>
              <p className="text-muted pt-1">Or set environment variable:</p>
              <code className="block text-emerald-300 font-mono">
                export CHOLA_TOKEN={token.token}
              </code>
            </>
          )}
        </div>
        <div className="flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm bg-surface-2 text-primary rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Create token modal ────────────────────────────────────────────────────────

interface CreateModalProps {
  onClose: () => void;
  onCreated: (t: CreatedWorkerToken) => void;
  defaultScope?: string;
}

function CreateTokenModal({ onClose, onCreated, defaultScope = 'worker' }: CreateModalProps) {
  const [name, setName] = useState('');
  const [scope, setScope] = useState(defaultScope);
  const [selectedWorker, setSelectedWorker] = useState('');
  const [expiresAt, setExpiresAt] = useState('');
  const [maxUses, setMaxUses] = useState('0');

  // Fetch workers for the dropdown (only when scope is worker)
  const { data: workersData } = useQuery({
    queryKey: ['workers'],
    queryFn: () => listWorkers(),
    enabled: scope === 'worker',
  });
  const workers = workersData?.data ?? [];

  const mut = useMutation({
    mutationFn: () =>
      createWorkerToken({
        name: name || selectedWorker || 'token',
        scope,
        expires_at: expiresAt || undefined,
        max_uses: parseInt(maxUses, 10) || 0,
        worker_id: scope === 'worker' && selectedWorker ? selectedWorker : undefined,
      }),
    onSuccess: (data) => {
      onCreated(data);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create token'),
  });

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="bg-surface border border-border rounded-xl p-6 max-w-md w-full">
        <h3 className="text-lg font-semibold text-primary mb-4">Create Token</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm text-secondary mb-1">Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
              placeholder="e.g. worker-a or jenkins-runner"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-sm text-secondary mb-1">Scope</label>
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            >
              <option value="worker">worker (chola_wkr_)</option>
              <option value="runner">runner (chola_svc_)</option>
              <option value="shared">shared</option>
            </select>
          </div>
          {scope === 'worker' && (
            <div>
              <label className="block text-sm text-secondary mb-1">Worker</label>
              {workers.length > 0 ? (
                <select
                  value={selectedWorker}
                  onChange={(e) => {
                    setSelectedWorker(e.target.value);
                    if (!name) setName(e.target.value);
                  }}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
                >
                  <option value="">— Select registered worker —</option>
                  {workers.map((w: { worker_id: string; hostname: string; status: string }) => (
                    <option key={w.worker_id} value={w.worker_id}>
                      {w.worker_id} ({w.hostname}) — {w.status}
                    </option>
                  ))}
                </select>
              ) : (
                <p className="text-xs text-disabled italic">
                  No workers registered. <a href="/workers" className="text-accent-text underline">Register a worker first</a>.
                </p>
              )}
            </div>
          )}
          <div>
            <label className="block text-sm text-secondary mb-1">Expires at (optional)</label>
            <input
              type="datetime-local"
              value={expiresAt ? expiresAt.slice(0, 16) : ''}
              onChange={(e) =>
                setExpiresAt(e.target.value ? new Date(e.target.value).toISOString() : '')
              }
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            />
            <p className="text-xs text-muted mt-1">Leave blank for no expiry</p>
          </div>
          <div>
            <label className="block text-sm text-secondary mb-1">Max uses (0 = unlimited)</label>
            <input
              type="number"
              min="0"
              value={maxUses}
              onChange={(e) => setMaxUses(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            />
          </div>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={() => mut.mutate()}
            disabled={!name.trim() || mut.isPending}
            className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
          >
            {mut.isPending ? 'Creating...' : 'Create'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

interface WorkerTokensPageProps {
  /** If set, only show tokens with this scope. */
  filterScope?: string;
  /** Default scope when creating a new token. */
  defaultScope?: string;
}

export default function WorkerTokensPage({ filterScope, defaultScope }: WorkerTokensPageProps = {}) {
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();
  const [showCreate, setShowCreate] = useState(false);
  const [createdToken, setCreatedToken] = useState<CreatedWorkerToken | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({
    queryKey: ['worker-tokens'],
    queryFn: listWorkerTokens,
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, active }: { id: string; active: boolean }) =>
      active ? deactivateWorkerToken(id) : activateWorkerToken(id),
    onSuccess: () => {
      toast.success('Token updated');
      qc.invalidateQueries({ queryKey: ['worker-tokens'] });
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update token'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteWorkerToken(id),
    onSuccess: () => {
      toast.success('Token deleted');
      qc.invalidateQueries({ queryKey: ['worker-tokens'] });
      setDeleteId(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to delete token'),
  });

  const allTokens: WorkerToken[] = data?.data ?? [];
  const tokens = filterScope
    ? allTokens.filter((t) =>
        filterScope === 'worker'
          ? t.scope !== 'runner'
          : t.scope === filterScope
      )
    : allTokens;

  const isRunnerView = filterScope === 'runner';
  // 'worker' is a filter alias (not a real scope), default to 'shared' for creates
  const resolvedDefaultScope = defaultScope ?? (filterScope === 'worker' ? 'shared' : filterScope) ?? 'shared';

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-primary">
          {isRunnerView ? 'Runner Tokens' : 'Worker Tokens'}
        </h2>
        {canManageRepos && (
          <button
            onClick={() => setShowCreate(true)}
            className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
          >
            Create Token
          </button>
        )}
      </div>

      <p className="text-sm text-muted">
        {isRunnerView
          ? 'Runner tokens authenticate ci-job-runner and scripts. Set CHOLA_TOKEN env var. Each token is shown only once.'
          : 'Registration tokens allow workers to authenticate and join the pool. Each token is shown only once.'}
      </p>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load tokens. Please try again.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="bg-surface border border-border rounded-xl overflow-hidden">
          {tokens.length === 0 ? (
            <EmptyState
              title={isRunnerView ? 'No runner tokens' : 'No worker tokens'}
              description={
                isRunnerView
                  ? 'Create a runner token for ci-job-runner authentication.'
                  : 'Create a token to allow workers to register.'
              }
            />
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border">
                  <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider">
                    Name
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider">
                    Scope
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider">
                    Uses
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider">
                    Expires
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider">
                    Status
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase tracking-wider">
                    Created
                  </th>
                  {canManageRepos && (
                    <th className="px-4 py-3 text-right text-xs font-semibold text-muted uppercase tracking-wider">
                      Actions
                    </th>
                  )}
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {tokens.map((t) => (
                  <tr key={t.id} className="hover:bg-surface-hover/50 transition-colors">
                    <td className="px-4 py-3 text-primary font-medium">{t.name}</td>
                    <td className="px-4 py-3">
                      <ScopeBadge scope={t.scope} />
                    </td>
                    <td className="px-4 py-3 text-secondary font-mono text-xs">
                      {formatNumber(t.use_count)}
                      {t.max_uses > 0 && ` / ${formatNumber(t.max_uses)}`}
                    </td>
                    <td className="px-4 py-3 text-muted text-xs">
                      {t.expires_at
                        ? <span title={new Date(t.expires_at).toUTCString()}><TimeAgo date={t.expires_at} /></span>
                        : 'Never'}
                    </td>
                    <td className="px-4 py-3">
                      <span
                        className={`text-xs px-1.5 py-0.5 rounded border ${
                          t.active
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-surface-2 text-muted border-border'
                        }`}
                      >
                        {t.active ? 'Active' : 'Inactive'}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-disabled text-xs">
                      <TimeAgo date={t.created_at} />
                      {t.created_by && (
                        <span className="text-disabled"> &middot; by {t.created_by}</span>
                      )}
                    </td>
                    {canManageRepos && (
                      <td className="px-4 py-3 text-right">
                        <div className="flex items-center justify-end gap-2">
                          <button
                            onClick={() => toggleMut.mutate({ id: t.id, active: t.active })}
                            disabled={toggleMut.isPending}
                            className={`px-2 py-1 text-xs rounded border focus:outline-none focus:ring-1 ${
                              t.active
                                ? 'bg-surface-2 text-secondary border-border hover:bg-surface-hover focus:ring-border'
                                : 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20 focus:ring-emerald-500'
                            }`}
                          >
                            {t.active ? 'Deactivate' : 'Activate'}
                          </button>
                          <button
                            onClick={() => setDeleteId(t.id)}
                            className="px-2 py-1 text-xs bg-red-500/10 text-red-400 border border-red-500/30 rounded hover:bg-red-500/20 focus:outline-none focus:ring-1 focus:ring-red-500"
                          >
                            Delete
                          </button>
                        </div>
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      {showCreate && (
        <CreateTokenModal
          onClose={() => setShowCreate(false)}
          onCreated={(t) => {
            setShowCreate(false);
            setCreatedToken(t);
            qc.invalidateQueries({ queryKey: ['worker-tokens'] });
          }}
          defaultScope={resolvedDefaultScope}
        />
      )}

      {createdToken && (
        <CreatedTokenModal token={createdToken} onClose={() => setCreatedToken(null)} />
      )}

      <ConfirmDialog
        open={deleteId !== null}
        title={isRunnerView ? 'Delete Runner Token' : 'Delete Worker Token'}
        message={
          isRunnerView
            ? 'This runner token will be permanently deleted. ci-job-runner instances using it will fail to authenticate.'
            : 'This token will be permanently deleted. Workers using it will not be affected, but new workers cannot register with it.'
        }
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteId && deleteMut.mutate(deleteId)}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}
