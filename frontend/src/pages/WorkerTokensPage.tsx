import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  listWorkerTokens,
  createWorkerToken,
  activateWorkerToken,
  deactivateWorkerToken,
  deleteWorkerToken,
} from '../api/workerTokens';
import type { WorkerToken, CreatedWorkerToken } from '../api/workerTokens';
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
    shared: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
    project: 'bg-purple-500/20 text-purple-400 border-purple-500/30',
    team: 'bg-indigo-500/20 text-indigo-400 border-indigo-500/30',
    runner: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  };
  const cls = colors[scope] ?? 'bg-slate-700 text-slate-400 border-slate-600';
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
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full">
        <div className="flex items-center gap-2 mb-2">
          <span className="text-emerald-400 text-lg font-bold">Token created</span>
        </div>
        <p className="text-sm text-yellow-400 mb-4">
          Copy this token now — it will not be shown again.
        </p>
        <div className="bg-slate-800 border border-slate-600 rounded-lg p-3 mb-4 flex items-center gap-2">
          <code className="text-emerald-300 font-mono text-xs break-all flex-1 select-all">
            {token.token}
          </code>
          <button
            onClick={copy}
            className="shrink-0 px-2 py-1 text-xs bg-blue-600/20 text-blue-400 border border-blue-500/30 rounded hover:bg-blue-600/30 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <dl className="text-sm space-y-1 mb-4">
          <div className="flex gap-2">
            <dt className="text-slate-500 w-24">Name</dt>
            <dd className="text-slate-200">{token.name}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="text-slate-500 w-24">Scope</dt>
            <dd><ScopeBadge scope={token.scope} /></dd>
          </div>
          {token.expires_at && (
            <div className="flex gap-2">
              <dt className="text-slate-500 w-24">Expires</dt>
              <dd className="text-slate-200"><TimeAgo date={token.expires_at} /></dd>
            </div>
          )}
          <div className="flex gap-2">
            <dt className="text-slate-500 w-24">Max uses</dt>
            <dd className="text-slate-200">{token.max_uses === 0 ? 'Unlimited' : token.max_uses}</dd>
          </div>
        </dl>
        {token.scope === 'runner' && (
          <div className="bg-slate-800 border border-slate-700 rounded-lg p-3 mb-4 text-xs space-y-2">
            <p className="text-slate-300 font-medium">Use this token with ci-job-runner:</p>
            <code className="block text-emerald-300 font-mono">
              --auth-token {token.token}
            </code>
            <p className="text-slate-400 pt-1">Or set environment variable:</p>
            <code className="block text-emerald-300 font-mono">
              export CHOLA_AUTH_TOKEN={token.token}
            </code>
          </div>
        )}
        <div className="flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm bg-slate-700 text-white rounded-lg hover:bg-slate-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
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

function CreateTokenModal({ onClose, onCreated, defaultScope = 'shared' }: CreateModalProps) {
  const [name, setName] = useState('');
  const [scope, setScope] = useState(defaultScope);
  const [expiresAt, setExpiresAt] = useState('');
  const [maxUses, setMaxUses] = useState('0');

  const mut = useMutation({
    mutationFn: () =>
      createWorkerToken({
        name,
        scope,
        expires_at: expiresAt || undefined,
        max_uses: parseInt(maxUses, 10) || 0,
      }),
    onSuccess: (data) => {
      onCreated(data);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create token'),
  });

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
        <h3 className="text-lg font-semibold text-white mb-4">
          {defaultScope === 'runner' ? 'Create Runner Token' : 'Create Worker Token'}
        </h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm text-slate-300 mb-1">Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder={defaultScope === 'runner' ? 'ci-job-runner-prod' : 'prod-workers'}
              autoFocus
            />
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">Scope</label>
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              {defaultScope === 'runner' ? (
                <option value="runner">runner</option>
              ) : (
                <>
                  <option value="shared">shared</option>
                  <option value="project">project</option>
                  <option value="team">team</option>
                </>
              )}
            </select>
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">Expires at (optional)</label>
            <input
              type="datetime-local"
              value={expiresAt ? expiresAt.slice(0, 16) : ''}
              onChange={(e) =>
                setExpiresAt(e.target.value ? new Date(e.target.value).toISOString() : '')
              }
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <p className="text-xs text-slate-500 mt-1">Leave blank for no expiry</p>
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">Max uses (0 = unlimited)</label>
            <input
              type="number"
              min="0"
              value={maxUses}
              onChange={(e) => setMaxUses(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Cancel
          </button>
          <button
            onClick={() => mut.mutate()}
            disabled={!name.trim() || mut.isPending}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
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
        <h2 className="text-2xl font-bold text-white">
          {isRunnerView ? 'Runner Tokens' : 'Worker Tokens'}
        </h2>
        {canManageRepos && (
          <button
            onClick={() => setShowCreate(true)}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Create Token
          </button>
        )}
      </div>

      <p className="text-sm text-slate-400">
        {isRunnerView
          ? 'Runner tokens authenticate ci-job-runner with the controller. Use --auth-token or CHOLA_AUTH_TOKEN. Each token is shown only once.'
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
        <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
          {tokens.length === 0 ? (
            <EmptyState
              message={isRunnerView ? 'No runner tokens' : 'No worker tokens'}
              description={
                isRunnerView
                  ? 'Create a runner token for ci-job-runner authentication.'
                  : 'Create a token to allow workers to register.'
              }
            />
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-slate-700">
                  <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wider">
                    Name
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wider">
                    Scope
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wider">
                    Uses
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wider">
                    Expires
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wider">
                    Status
                  </th>
                  <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wider">
                    Created
                  </th>
                  {canManageRepos && (
                    <th className="px-4 py-3 text-right text-xs font-semibold text-slate-500 uppercase tracking-wider">
                      Actions
                    </th>
                  )}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800">
                {tokens.map((t) => (
                  <tr key={t.id} className="hover:bg-slate-800/50 transition-colors">
                    <td className="px-4 py-3 text-white font-medium">{t.name}</td>
                    <td className="px-4 py-3">
                      <ScopeBadge scope={t.scope} />
                    </td>
                    <td className="px-4 py-3 text-slate-300 font-mono text-xs">
                      {t.use_count}
                      {t.max_uses > 0 && ` / ${t.max_uses}`}
                    </td>
                    <td className="px-4 py-3 text-slate-400 text-xs">
                      {t.expires_at ? <TimeAgo date={t.expires_at} /> : 'Never'}
                    </td>
                    <td className="px-4 py-3">
                      <span
                        className={`text-xs px-1.5 py-0.5 rounded border ${
                          t.active
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-slate-700 text-slate-400 border-slate-600'
                        }`}
                      >
                        {t.active ? 'Active' : 'Inactive'}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-slate-500 text-xs">
                      <TimeAgo date={t.created_at} />
                      {t.created_by && (
                        <span className="ml-1 text-slate-600">by {t.created_by}</span>
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
                                ? 'bg-slate-700 text-slate-300 border-slate-600 hover:bg-slate-600 focus:ring-slate-500'
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
