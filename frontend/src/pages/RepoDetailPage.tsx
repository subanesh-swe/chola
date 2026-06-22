import React, { useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getRepo,
  updateRepo,
  listStageConfigs,
  createStageConfig,
  updateStageConfig,
  deleteStageConfig,
  listWebhooks,
  createWebhook,
  deleteWebhook,
  listWebhookDeliveries,
  listSchedules,
  createSchedule,
  updateSchedule,
  deleteSchedule,
} from '../api/repos';
import type { Schedule } from '../api/repos';
import {
  listCommandBlacklist,
  createCommandBlacklist,
  deleteCommandBlacklist,
  updateCommandBlacklist,
} from '../api/blacklist';
import {
  listScripts,
  createScript,
  updateScript,
  deleteScript,
} from '../api/scripts';
import type { CreateScriptRequest } from '../api/scripts';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { usePermission } from '../hooks/usePermission';
import { TimeAgo } from '../components/ui/TimeAgo';
import { toast } from 'sonner';
import type { Webhook, MutationError, CommandBlacklistEntry, StageScript } from '../types';

// ── Scripts panel ────────────────────────────────────────────────────────────

function ScriptsPanel({ repoId, stageId, canManage }: { repoId: string; stageId: string; canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [scriptType, setScriptType] = useState<'pre' | 'post'>('pre');
  const [scriptScope, setScriptScope] = useState<'worker' | 'master'>('worker');
  const [scriptContent, setScriptContent] = useState('');
  const [workerId, setWorkerId] = useState('');
  const [lockEnabled, setLockEnabled] = useState(false);
  const [lockKey, setLockKey] = useState('');
  const [lockTimeoutSecs, setLockTimeoutSecs] = useState(120);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editType, setEditType] = useState<'pre' | 'post'>('pre');
  const [editScope, setEditScope] = useState<'worker' | 'master'>('worker');
  const [editContent, setEditContent] = useState('');
  const [editWorkerId, setEditWorkerId] = useState('');
  const [editLockEnabled, setEditLockEnabled] = useState(false);
  const [editLockKey, setEditLockKey] = useState('');
  const [editLockTimeoutSecs, setEditLockTimeoutSecs] = useState(120);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ['scripts', repoId, stageId],
    queryFn: () => listScripts(repoId, stageId),
  });

  const scripts: StageScript[] = data?.scripts ?? [];

  const inputCls = 'w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent';

  const resetAddForm = () => {
    setScriptType('pre');
    setScriptScope('worker');
    setScriptContent('');
    setWorkerId('');
    setLockEnabled(false);
    setLockKey('');
    setLockTimeoutSecs(120);
  };

  const startEdit = (s: StageScript) => {
    setEditingId(s.id);
    setEditType(s.script_type);
    setEditScope(s.script_scope);
    setEditContent(s.script);
    setEditWorkerId(s.worker_id ?? '');
    setEditLockEnabled(s.lock_enabled ?? false);
    setEditLockKey(s.lock_key ?? '');
    setEditLockTimeoutSecs(s.lock_timeout_secs ?? 120);
  };

  const cancelEdit = () => setEditingId(null);

  const createMut = useMutation({
    mutationFn: () => {
      const req: CreateScriptRequest = {
        script_type: scriptType,
        script_scope: scriptScope,
        script: scriptContent,
        lock_enabled: lockEnabled,
        lock_key: lockEnabled && lockKey.trim() ? lockKey.trim() : null,
        lock_timeout_secs: lockEnabled ? lockTimeoutSecs : 120,
      };
      if (workerId.trim()) req.worker_id = workerId.trim();
      return createScript(repoId, stageId, req);
    },
    onSuccess: () => {
      toast.success('Script created');
      qc.invalidateQueries({ queryKey: ['scripts', repoId, stageId] });
      setShowAdd(false);
      resetAddForm();
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to create script'),
  });

  const updateMut = useMutation({
    mutationFn: (scriptId: string) =>
      updateScript(repoId, stageId, scriptId, {
        script_type: editType,
        script_scope: editScope,
        script: editContent,
        worker_id: editWorkerId.trim() || undefined,
        lock_enabled: editLockEnabled,
        lock_key: editLockEnabled && editLockKey.trim() ? editLockKey.trim() : null,
        lock_timeout_secs: editLockEnabled ? editLockTimeoutSecs : 120,
      }),
    onSuccess: () => {
      toast.success('Script updated');
      qc.invalidateQueries({ queryKey: ['scripts', repoId, stageId] });
      setEditingId(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to update script'),
  });

  const deleteMut = useMutation({
    mutationFn: (scriptId: string) => deleteScript(repoId, stageId, scriptId),
    onSuccess: () => {
      toast.success('Script deleted');
      qc.invalidateQueries({ queryKey: ['scripts', repoId, stageId] });
      setDeleteId(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to delete script'),
  });

  return (
    <div className="px-4 py-3 bg-surface-2/40 border-t border-border/50">
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs font-semibold text-muted uppercase tracking-wide">Scripts</span>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-2 py-1 text-xs bg-accent text-on-accent rounded hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Add Script
          </button>
        )}
      </div>

      {isLoading ? (
        <p className="text-xs text-disabled italic">Loading scripts...</p>
      ) : scripts.length === 0 ? (
        <p className="text-xs text-disabled italic">No scripts configured for this stage.</p>
      ) : (
        <div className="space-y-2">
          {scripts.map((s) => {
            const isEditing = editingId === s.id;
            if (isEditing) {
              return (
                <div key={s.id} className="bg-surface border border-border rounded-lg p-3 space-y-2">
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="block text-xs text-muted mb-1">Type</label>
                      <select value={editType} onChange={(e) => setEditType(e.target.value as 'pre' | 'post')}
                        className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent">
                        <option value="pre">pre</option>
                        <option value="post">post</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-xs text-muted mb-1">Scope</label>
                      <select value={editScope} onChange={(e) => setEditScope(e.target.value as 'worker' | 'master')}
                        className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent">
                        <option value="worker">worker</option>
                        <option value="master">master</option>
                      </select>
                    </div>
                  </div>
                  <div>
                    <label className="block text-xs text-muted mb-1">Worker ID (optional)</label>
                    <input value={editWorkerId} onChange={(e) => setEditWorkerId(e.target.value)}
                      className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs font-mono focus:outline-none focus:ring-2 focus:ring-accent"
                      placeholder="Leave blank for all workers" />
                  </div>
                  <div>
                    <label className="flex items-center gap-2 cursor-pointer select-none">
                      <input type="checkbox" checked={editLockEnabled} onChange={(e) => setEditLockEnabled(e.target.checked)}
                        className="rounded border-border bg-input accent-accent focus:ring-accent" />
                      <span className="text-xs text-secondary">Enable Lock</span>
                    </label>
                    {editLockEnabled && (
                      <div className="mt-2 space-y-2 pl-1">
                        <div>
                          <label className="block text-xs text-muted mb-1">Lock Key</label>
                          <input value={editLockKey} onChange={(e) => setEditLockKey(e.target.value)}
                            className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs font-mono focus:outline-none focus:ring-2 focus:ring-accent"
                            placeholder="{{WORKER_ID}}-{{REPO_NAME}}-pre" />
                          <p className="text-xs text-disabled mt-0.5">Vars: {'{{WORKER_ID}} {{REPO_NAME}} {{COMMIT_SHA}} {{BRANCH}} {{STAGE_NAME}} {{JOB_GROUP_ID}}'}</p>
                        </div>
                        <div>
                          <label className="block text-xs text-muted mb-1">Timeout (secs)</label>
                          <input type="number" min={1} value={editLockTimeoutSecs} onChange={(e) => setEditLockTimeoutSecs(Number(e.target.value))}
                            className="w-28 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent" />
                        </div>
                      </div>
                    )}
                  </div>
                  <div>
                    <label className="block text-xs text-muted mb-1">Script</label>
                    <textarea value={editContent} onChange={(e) => setEditContent(e.target.value)} rows={4}
                      className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs font-mono focus:outline-none focus:ring-2 focus:ring-accent resize-y"
                      placeholder="#!/bin/bash&#10;echo hello" />
                  </div>
                  <div className="flex gap-2 justify-end">
                    <button onClick={cancelEdit}
                      className="px-2 py-1 text-xs text-muted hover:text-primary focus:outline-none focus:ring-1 focus:ring-border rounded">
                      Cancel
                    </button>
                    <button onClick={() => updateMut.mutate(s.id)} disabled={!editContent || updateMut.isPending}
                      className="px-2 py-1 text-xs bg-success text-white rounded hover:bg-success disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-success">
                      Save
                    </button>
                  </div>
                </div>
              );
            }
            return (
              <div key={s.id} className="bg-surface border border-border rounded-lg p-3">
                <div className="flex items-center gap-2 mb-2">
                  <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${
                    s.script_type === 'pre'
                      ? 'bg-accent-soft text-accent-text border border-accent/30'
                      : 'bg-pending-soft text-pending border border-pending/30'
                  }`}>
                    {s.script_type.toUpperCase()}
                  </span>
                  <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${
                    s.script_scope === 'worker'
                      ? 'bg-success-soft text-success border border-success/30'
                      : 'bg-warning-soft text-warning border border-warning/30'
                  }`}>
                    {s.script_scope.toUpperCase()}
                  </span>
                  {s.worker_id && (
                    <span className="text-xs text-muted font-mono">worker: {s.worker_id}</span>
                  )}
                  {s.lock_enabled ? (
                    <span className="text-xs px-1.5 py-0.5 rounded font-medium bg-warning-soft text-warning border border-warning/30">
                      LOCK {s.lock_key || '(default)'} — {s.lock_timeout_secs}s
                    </span>
                  ) : (
                    <span className="text-xs text-disabled">No lock</span>
                  )}
                  {canManage && (
                    <div className="ml-auto flex gap-2">
                      <button
                        onClick={() => startEdit(s)}
                        disabled={editingId !== null}
                        className="text-xs text-accent-text hover:opacity-80 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-accent rounded"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => setDeleteId(s.id)}
                        disabled={editingId !== null}
                        className="text-xs text-danger hover:text-danger disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-danger rounded"
                      >
                        Delete
                      </button>
                    </div>
                  )}
                </div>
                <pre className="text-xs text-secondary font-mono bg-surface-2 rounded p-2 overflow-x-auto whitespace-pre-wrap break-words max-h-40">
                  {s.script}
                </pre>
              </div>
            );
          })}
        </div>
      )}

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-surface border border-border rounded-xl p-6 max-w-lg w-full">
            <h3 className="text-lg font-semibold text-primary mb-4">Add Script</h3>
            <div className="space-y-3">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-sm text-secondary mb-1">Type</label>
                  <select value={scriptType} onChange={(e) => setScriptType(e.target.value as 'pre' | 'post')} className={inputCls}>
                    <option value="pre">pre (runs before stage)</option>
                    <option value="post">post (runs after stage)</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm text-secondary mb-1">Scope</label>
                  <select value={scriptScope} onChange={(e) => setScriptScope(e.target.value as 'worker' | 'master')} className={inputCls}>
                    <option value="worker">worker (runs on worker node)</option>
                    <option value="master">master (runs on master node)</option>
                  </select>
                </div>
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Worker ID (optional)</label>
                <input value={workerId} onChange={(e) => setWorkerId(e.target.value)} className={inputCls}
                  placeholder="Leave blank to target all workers" />
              </div>
              <div>
                <label className="flex items-center gap-2 cursor-pointer select-none">
                  <input type="checkbox" checked={lockEnabled} onChange={(e) => setLockEnabled(e.target.checked)}
                    className="rounded border-border bg-input accent-accent focus:ring-accent" />
                  <span className="text-sm text-secondary">Enable Lock</span>
                </label>
                {lockEnabled && (
                  <div className="mt-2 space-y-2 pl-1 border-l-2 border-warning/40 pl-3">
                    <div>
                      <label className="block text-sm text-secondary mb-1">Lock Key</label>
                      <input value={lockKey} onChange={(e) => setLockKey(e.target.value)} className={inputCls}
                        placeholder="{{WORKER_ID}}-{{REPO_NAME}}-pre" />
                      <p className="text-xs text-disabled mt-1">
                        Vars: {'{{WORKER_ID}} {{REPO_NAME}} {{COMMIT_SHA}} {{BRANCH}} {{STAGE_NAME}} {{JOB_GROUP_ID}}'}
                      </p>
                    </div>
                    <div>
                      <label className="block text-sm text-secondary mb-1">Timeout (secs)</label>
                      <input type="number" min={1} value={lockTimeoutSecs} onChange={(e) => setLockTimeoutSecs(Number(e.target.value))}
                        className="w-36 px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent" />
                    </div>
                  </div>
                )}
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Script</label>
                <textarea value={scriptContent} onChange={(e) => setScriptContent(e.target.value)} rows={6}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm font-mono focus:outline-none focus:ring-2 focus:ring-accent resize-y"
                  placeholder={'#!/bin/bash\necho "pre-stage hook"'} />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => { setShowAdd(false); resetAddForm(); }}
                className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent">
                Cancel
              </button>
              <button onClick={() => createMut.mutate()} disabled={!scriptContent || createMut.isPending}
                className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent">
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={deleteId !== null}
        title="Delete Script"
        message="This script will be permanently removed."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteId && deleteMut.mutate(deleteId)}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}

// ── Stage section ─────────────────────────────────────────────────────────────

function formatResourceValue(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}

function formatDurationSecs(secs: number): string {
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  if (secs >= 60) return `${Math.floor(secs / 60)}m`;
  return `${secs}s`;
}

function ResourceSummary({ stages }: { stages: { required_cpu: number; required_memory_mb: number; required_disk_mb: number; max_duration_secs: number }[] }) {
  if (!stages.length) return null;
  const maxCpu = Math.max(...stages.map(s => s.required_cpu));
  const maxMem = Math.max(...stages.map(s => s.required_memory_mb));
  const maxDisk = Math.max(...stages.map(s => s.required_disk_mb));
  const maxDuration = Math.max(...stages.map(s => s.max_duration_secs));

  if (maxCpu === 0 && maxMem === 0 && maxDisk === 0) return null;

  return (
    <div className="bg-surface-2/50 border border-border rounded-lg px-4 py-3">
      <p className="text-xs text-disabled mb-1.5 uppercase font-semibold">Resource Requirements (max across stages)</p>
      <div className="flex gap-6 text-sm">
        <span className="text-secondary">CPU: <span className="text-primary font-medium">{maxCpu} cores</span></span>
        <span className="text-secondary">Memory: <span className="text-primary font-medium">{formatResourceValue(maxMem)}</span></span>
        <span className="text-secondary">Disk: <span className="text-primary font-medium">{formatResourceValue(maxDisk)}</span></span>
        <span className="text-secondary">Max Duration: <span className="text-primary font-medium">{formatDurationSecs(maxDuration)}</span></span>
      </div>
    </div>
  );
}

function StageSection({ repoId, canManage }: { repoId: string; canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [expandedStageId, setExpandedStageId] = useState<string | null>(null);
  const [stageName, setStageName] = useState('');
  const [command, setCommand] = useState('');
  const [commandMode, setCommandMode] = useState('fixed');
  const [requiredCpu, setRequiredCpu] = useState(1);
  const [requiredMemoryMb, setRequiredMemoryMb] = useState(512);
  const [requiredDiskMb, setRequiredDiskMb] = useState(256);
  const [maxDurationSecs, setMaxDurationSecs] = useState(3600);

  // Edit mode state
  const [editingStageId, setEditingStageId] = useState<string | null>(null);
  const [editCommand, setEditCommand] = useState('');
  const [editCommandMode, setEditCommandMode] = useState('fixed');
  const [editCpu, setEditCpu] = useState(1);
  const [editMemoryMb, setEditMemoryMb] = useState(512);
  const [editDiskMb, setEditDiskMb] = useState(256);
  const [editDurationSecs, setEditDurationSecs] = useState(3600);

  const { data: stagesData } = useQuery({
    queryKey: ['stages', repoId],
    queryFn: () => listStageConfigs(repoId),
  });

  const resetForm = () => {
    setStageName('');
    setCommand('');
    setCommandMode('fixed');
    setRequiredCpu(1);
    setRequiredMemoryMb(512);
    setRequiredDiskMb(256);
    setMaxDurationSecs(3600);
  };

  const startEdit = (s: import('../types').StageConfig) => {
    setEditingStageId(s.id);
    setEditCommand(s.command ?? '');
    setEditCommandMode(s.command_mode ?? 'fixed');
    setEditCpu(s.required_cpu);
    setEditMemoryMb(s.required_memory_mb);
    setEditDiskMb(s.required_disk_mb);
    setEditDurationSecs(s.max_duration_secs);
  };

  const cancelEdit = () => setEditingStageId(null);

  const addStage = useMutation({
    mutationFn: () => createStageConfig(repoId, {
      stage_name: stageName,
      command: commandMode === 'required' && !command ? undefined : command,
      command_mode: commandMode,
      required_cpu: requiredCpu,
      required_memory_mb: requiredMemoryMb,
      required_disk_mb: requiredDiskMb,
      max_duration_secs: maxDurationSecs,
    }),
    onSuccess: () => {
      toast.success('Stage created');
      qc.invalidateQueries({ queryKey: ['stages', repoId] });
      setShowAdd(false);
      resetForm();
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to create stage'),
  });

  const delStage = useMutation({
    mutationFn: (stageId: string) => deleteStageConfig(repoId, stageId),
    onSuccess: () => { toast.success('Stage deleted'); qc.invalidateQueries({ queryKey: ['stages', repoId] }); },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to delete stage'),
  });

  const updateStage = useMutation({
    mutationFn: (stageId: string) => updateStageConfig(repoId, stageId, {
      command: editCommandMode === 'required' && !editCommand ? undefined : editCommand || undefined,
      command_mode: editCommandMode,
      required_cpu: editCpu,
      required_memory_mb: editMemoryMb,
      required_disk_mb: editDiskMb,
      max_duration_secs: editDurationSecs,
    }),
    onSuccess: () => {
      toast.success('Stage updated');
      qc.invalidateQueries({ queryKey: ['stages', repoId] });
      setEditingStageId(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to update stage'),
  });

  const stages = (stagesData?.stages ?? []).sort((a, b) => a.execution_order - b.execution_order);
  const inputCls = "w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent";

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-primary">Stage Configs</h3>
        {canManage && (
          <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent">
            Add Stage
          </button>
        )}
      </div>

      <ResourceSummary stages={stages} />

      <div className="bg-surface border border-border rounded-xl overflow-hidden">
        <table className="w-full" aria-label="Stage configurations">
          <thead>
            <tr className="border-b border-border">
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Order</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Name</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Command</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Mode</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">CPU</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Memory</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Disk</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Type</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Timeout</th>
              <th className="px-4 py-3 text-xs font-semibold text-muted uppercase">Scripts</th>
              {canManage && <th className="px-4 py-3 text-xs text-muted uppercase">Actions</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {stages.map(s => {
              const isEditing = editingStageId === s.id;
              if (isEditing) {
                return (
                  <tr key={s.id} className="bg-surface-2/40">
                    <td className="px-4 py-2 text-sm text-muted">{s.execution_order}</td>
                    <td className="px-4 py-2 text-sm text-secondary font-medium">{s.stage_name}</td>
                    <td className="px-4 py-2">
                      <textarea
                        value={editCommand}
                        onChange={e => setEditCommand(e.target.value)}
                        rows={2}
                        className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs font-mono focus:outline-none focus:ring-2 focus:ring-accent resize-none"
                        placeholder={editCommandMode === 'required' ? 'user-provided' : 'make build'}
                      />
                    </td>
                    <td className="px-4 py-2">
                      <select
                        value={editCommandMode}
                        onChange={e => setEditCommandMode(e.target.value)}
                        className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent"
                      >
                        <option value="fixed">fixed</option>
                        <option value="optional">optional</option>
                        <option value="required">required</option>
                      </select>
                    </td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0} max={1024}
                        value={editCpu}
                        onChange={e => setEditCpu(Number(e.target.value))}
                        className="w-16 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent"
                      />
                    </td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0}
                        value={editMemoryMb}
                        onChange={e => setEditMemoryMb(Number(e.target.value))}
                        className="w-20 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent"
                      />
                    </td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0}
                        value={editDiskMb}
                        onChange={e => setEditDiskMb(Number(e.target.value))}
                        className="w-20 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent"
                      />
                    </td>
                    <td className="px-4 py-2 text-sm text-muted">{s.job_type}</td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0} max={86400}
                        value={editDurationSecs}
                        onChange={e => setEditDurationSecs(Number(e.target.value))}
                        className="w-20 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent"
                      />
                    </td>
                    <td className="px-4 py-2"></td>
                    <td className="px-4 py-2 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <button
                          onClick={() => updateStage.mutate(s.id)}
                          disabled={updateStage.isPending}
                          className="text-xs text-success hover:text-success disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-success rounded"
                        >
                          Save
                        </button>
                        <button
                          onClick={cancelEdit}
                          className="text-xs text-muted hover:text-primary focus:outline-none focus:ring-1 focus:ring-border rounded"
                        >
                          Cancel
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              }
              const isExpanded = expandedStageId === s.id;
              const colSpanTotal = canManage ? 11 : 10;
              return (
                <React.Fragment key={s.id}>
                  <tr className={isExpanded ? 'bg-surface-2/20' : undefined}>
                    <td className="px-4 py-3 text-sm text-muted">{s.execution_order}</td>
                    <td className="px-4 py-3 text-sm text-secondary font-medium">{s.stage_name}</td>
                    <td className="px-4 py-3 text-sm text-muted font-mono truncate max-w-xs">{s.command || <span className="italic text-disabled">user-provided</span>}</td>
                    <td className="px-4 py-3 text-xs text-muted">{s.command_mode ?? 'fixed'}</td>
                    <td className="px-4 py-3 text-xs text-muted">{s.required_cpu}c</td>
                    <td className="px-4 py-3 text-xs text-muted">{formatResourceValue(s.required_memory_mb)}</td>
                    <td className="px-4 py-3 text-xs text-muted">{formatResourceValue(s.required_disk_mb)}</td>
                    <td className="px-4 py-3 text-sm text-muted">{s.job_type}</td>
                    <td className="px-4 py-3 text-sm text-muted">{formatDurationSecs(s.max_duration_secs)}</td>
                    <td className="px-4 py-3 text-center">
                      <button
                        onClick={() => setExpandedStageId(isExpanded ? null : s.id)}
                        aria-expanded={isExpanded}
                        aria-label={`${isExpanded ? 'Collapse' : 'Expand'} scripts for ${s.stage_name}`}
                        className="text-muted hover:text-primary focus:outline-none focus:ring-1 focus:ring-accent rounded"
                      >
                        <svg className={`w-4 h-4 transition-transform ${isExpanded ? 'rotate-90' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                        </svg>
                      </button>
                    </td>
                    {canManage && (
                      <td className="px-4 py-3 text-center">
                        <div className="flex items-center justify-center gap-2">
                          <button
                            onClick={() => startEdit(s)}
                            disabled={editingStageId !== null}
                            className="text-xs text-accent-text hover:opacity-80 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-accent rounded"
                          >
                            Edit
                          </button>
                          <button
                            onClick={() => delStage.mutate(s.id)}
                            disabled={editingStageId !== null}
                            className="text-xs text-danger hover:text-danger disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-danger rounded"
                          >
                            Delete
                          </button>
                        </div>
                      </td>
                    )}
                  </tr>
                  {isExpanded && (
                    <tr>
                      <td colSpan={colSpanTotal} className="p-0 border-b border-border/50">
                        <ScriptsPanel repoId={repoId} stageId={s.id} canManage={canManage} />
                      </td>
                    </tr>
                  )}
                </React.Fragment>
              );
            })}
            {!stages.length && (
              <tr><td colSpan={canManage ? 11 : 10} className="px-4 py-8 text-center text-disabled">No stages configured</td></tr>
            )}
          </tbody>
        </table>
      </div>

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-surface border border-border rounded-xl p-6 max-w-lg w-full mx-4">
            <h3 className="text-lg font-semibold text-primary mb-4">Add Stage</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-secondary mb-1">Stage Name</label>
                <input value={stageName} onChange={e => setStageName(e.target.value)} className={inputCls} placeholder="build" />
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Command Mode</label>
                <select value={commandMode} onChange={e => setCommandMode(e.target.value)} className={inputCls}>
                  <option value="fixed">Fixed (always use configured command)</option>
                  <option value="optional">Optional (user can override)</option>
                  <option value="required">Required (user must provide)</option>
                </select>
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Command {commandMode === 'required' ? '(optional)' : ''}</label>
                <input value={command} onChange={e => setCommand(e.target.value)} className={inputCls} placeholder={commandMode === 'required' ? 'User will provide at runtime' : 'make build'} />
                {commandMode === 'optional' && <p className="text-xs text-disabled mt-1">User can override this command at submission time</p>}
                {commandMode === 'required' && <p className="text-xs text-disabled mt-1">User must provide a command when submitting this stage</p>}
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-sm text-secondary mb-1">CPU (cores)</label>
                  <input type="number" min={0} max={1024} value={requiredCpu} onChange={e => setRequiredCpu(Number(e.target.value))} className={inputCls} />
                </div>
                <div>
                  <label className="block text-sm text-secondary mb-1">Memory (MB)</label>
                  <input type="number" min={0} value={requiredMemoryMb} onChange={e => setRequiredMemoryMb(Number(e.target.value))} className={inputCls} />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-sm text-secondary mb-1">Disk (MB)</label>
                  <input type="number" min={0} value={requiredDiskMb} onChange={e => setRequiredDiskMb(Number(e.target.value))} className={inputCls} />
                </div>
                <div>
                  <label className="block text-sm text-secondary mb-1">Timeout (seconds)</label>
                  <input type="number" min={0} max={86400} value={maxDurationSecs} onChange={e => setMaxDurationSecs(Number(e.target.value))} className={inputCls} />
                </div>
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => { setShowAdd(false); resetForm(); }} className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">Cancel</button>
              <button onClick={() => addStage.mutate()} disabled={!stageName || (commandMode !== 'required' && !command)} className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">Create</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Webhook delivery panel ────────────────────────────────────────────────────

function DeliveryPanel({ repoId, webhookId }: { repoId: string; webhookId: string }) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ['webhook-deliveries', repoId, webhookId],
    queryFn: () => listWebhookDeliveries(repoId, webhookId),
  });

  if (isLoading) return <div className="px-6 py-4 text-sm text-muted">Loading deliveries…</div>;
  if (isError) return <div className="px-6 py-4 text-sm text-disabled">No delivery history available.</div>;

  const deliveries = data?.deliveries ?? [];

  if (!deliveries.length) {
    return <div className="px-6 py-4 text-sm text-disabled">No deliveries yet.</div>;
  }

  return (
    <table className="w-full" aria-label="Webhook delivery history">
      <thead>
        <tr className="border-b border-border/50">
          <th className="px-6 py-2 text-left text-xs font-semibold text-muted uppercase">Delivered</th>
          <th className="px-6 py-2 text-left text-xs font-semibold text-muted uppercase">Event</th>
          <th className="px-6 py-2 text-left text-xs font-semibold text-muted uppercase">Status</th>
          <th className="px-6 py-2 text-left text-xs font-semibold text-muted uppercase">Response Time</th>
        </tr>
      </thead>
      <tbody className="divide-y divide-border">
        {deliveries.map(d => (
          <tr key={d.id}>
            <td className="px-6 py-2 text-sm"><TimeAgo date={d.delivered_at} className="text-disabled" /></td>
            <td className="px-6 py-2 text-sm text-secondary">{d.event}</td>
            <td className="px-6 py-2 text-sm">
              <span className={d.success ? 'text-success' : 'text-danger'}>
                {d.status_code}
              </span>
            </td>
            <td className="px-6 py-2 text-sm text-muted">{d.response_time_ms}ms</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── Webhook row ───────────────────────────────────────────────────────────────

function WebhookRow({ repoId, webhook, canManage, onDelete }: {
  repoId: string;
  webhook: Webhook;
  canManage: boolean;
  onDelete: () => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border-b border-border last:border-0">
      <div className="px-4 py-3 flex items-center gap-3">
        <button
          onClick={() => setExpanded(e => !e)}
          aria-expanded={expanded}
          aria-label={`${expanded ? 'Collapse' : 'Expand'} webhook ${webhook.provider} delivery history`}
          className="text-muted hover:text-primary focus:outline-none focus:ring-1 focus:ring-accent rounded"
        >
          <svg className={`w-4 h-4 transition-transform ${expanded ? 'rotate-90' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
        <span className="text-sm font-medium text-secondary capitalize">{webhook.provider}</span>
        <div className="flex gap-1 flex-wrap">
          {webhook.events.map(e => (
            <span key={e} className="text-xs px-1.5 py-0.5 bg-surface-2 text-secondary rounded">{e}</span>
          ))}
        </div>
        <span className={`ml-auto text-xs px-2 py-0.5 rounded ${webhook.enabled ? 'text-success bg-success-soft' : 'text-disabled bg-surface-2'}`}>
          {webhook.enabled ? 'active' : 'disabled'}
        </span>
        {canManage && (
          <button onClick={onDelete} className="text-xs text-danger hover:text-danger focus:outline-none focus:ring-1 focus:ring-danger rounded ml-2">
            Delete
          </button>
        )}
      </div>
      {expanded && (
        <div className="bg-surface-2/30">
          <DeliveryPanel repoId={repoId} webhookId={webhook.id} />
        </div>
      )}
    </div>
  );
}

// ── Webhook section ───────────────────────────────────────────────────────────

function WebhookSection({ repoId, canManage }: { repoId: string; canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [provider, setProvider] = useState<'github' | 'gitlab'>('github');

  const { data } = useQuery({
    queryKey: ['webhooks', repoId],
    queryFn: () => listWebhooks(repoId),
  });

  const addWebhook = useMutation({
    mutationFn: () => createWebhook(repoId, { provider, events: ['push'] }),
    onSuccess: () => {
      toast.success('Webhook created');
      qc.invalidateQueries({ queryKey: ['webhooks', repoId] });
      setShowAdd(false);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to create webhook'),
  });

  const delWebhook = useMutation({
    mutationFn: (webhookId: string) => deleteWebhook(repoId, webhookId),
    onSuccess: () => { toast.success('Webhook deleted'); qc.invalidateQueries({ queryKey: ['webhooks', repoId] }); },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to delete webhook'),
  });

  const webhooks = data?.webhooks ?? [];

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-primary">Webhooks</h3>
        {canManage && (
          <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent">
            Add Webhook
          </button>
        )}
      </div>

      <div className="bg-surface border border-border rounded-xl overflow-hidden">
        {webhooks.length === 0 ? (
          <div className="px-4 py-8 text-center text-disabled">No webhooks configured</div>
        ) : (
          webhooks.map(w => (
            <WebhookRow
              key={w.id}
              repoId={repoId}
              webhook={w}
              canManage={canManage}
              onDelete={() => delWebhook.mutate(w.id)}
            />
          ))
        )}
      </div>

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-surface border border-border rounded-xl p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold text-primary mb-4">Add Webhook</h3>
            <div>
              <label className="block text-sm text-secondary mb-1">Provider</label>
              <select value={provider} onChange={e => setProvider(e.target.value as 'github' | 'gitlab')} className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary focus:outline-none focus:ring-2 focus:ring-accent">
                <option value="github">GitHub</option>
                <option value="gitlab">GitLab</option>
              </select>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">Cancel</button>
              <button onClick={() => addWebhook.mutate()} disabled={addWebhook.isPending} className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">Create</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Command Blacklist Section ─────────────────────────────────────────────────

function RepoBlacklistSection({ repoId, canManage }: { repoId: string; canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [pattern, setPattern] = useState('');
  const [description, setDescription] = useState('');
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data } = useQuery({
    queryKey: ['blacklist-commands', repoId],
    queryFn: () => listCommandBlacklist(repoId),
  });

  const createMut = useMutation({
    mutationFn: () =>
      createCommandBlacklist({ repo_id: repoId, pattern, description: description || undefined }),
    onSuccess: () => {
      toast.success('Blacklist rule created');
      qc.invalidateQueries({ queryKey: ['blacklist-commands', repoId] });
      setShowAdd(false);
      setPattern('');
      setDescription('');
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create rule'),
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      updateCommandBlacklist(id, { enabled }),
    onSuccess: () => {
      toast.success('Rule updated');
      qc.invalidateQueries({ queryKey: ['blacklist-commands', repoId] });
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update rule'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteCommandBlacklist(id),
    onSuccess: () => {
      toast.success('Rule deleted');
      qc.invalidateQueries({ queryKey: ['blacklist-commands', repoId] });
      setDeleteId(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to delete rule'),
  });

  const entries: CommandBlacklistEntry[] = data?.entries ?? [];

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-primary">Command Blacklist</h3>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-3 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Add Rule
          </button>
        )}
      </div>

      <div className="bg-surface border border-border rounded-xl overflow-hidden">
        <table className="w-full" aria-label="Repo command blacklist">
          <thead>
            <tr className="border-b border-border">
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Pattern</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Description</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-muted uppercase">Status</th>
              {canManage && (
                <th className="px-4 py-3 text-xs font-semibold text-muted uppercase text-center">Actions</th>
              )}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {entries.map((e) => (
              <tr key={e.id}>
                <td className="px-4 py-3 text-sm text-secondary font-mono truncate max-w-xs">{e.pattern}</td>
                <td className="px-4 py-3 text-sm text-muted">{e.description ?? '—'}</td>
                <td className="px-4 py-3">
                  {canManage ? (
                    <button
                      onClick={() => toggleMut.mutate({ id: e.id, enabled: !e.enabled })}
                      className={`text-xs px-2 py-0.5 rounded border transition-colors focus:outline-none focus:ring-1 ${
                        e.enabled
                          ? 'bg-success-soft text-success border-success/30 hover:bg-success-soft'
                          : 'bg-surface-2 text-muted border-border hover:bg-surface-hover'
                      }`}
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
                {canManage && (
                  <td className="px-4 py-3 text-center">
                    <button
                      onClick={() => setDeleteId(e.id)}
                      className="text-xs text-danger hover:text-danger focus:outline-none focus:ring-1 focus:ring-danger rounded"
                    >
                      Delete
                    </button>
                  </td>
                )}
              </tr>
            ))}
            {!entries.length && (
              <tr>
                <td colSpan={canManage ? 4 : 3} className="px-4 py-6 text-center text-disabled text-sm">
                  No blacklist rules for this repo.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

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
                className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={() => createMut.mutate()}
                disabled={!pattern || createMut.isPending}
                className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
              >
                Create
              </button>
            </div>
          </div>
        </div>
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

// ── Schedule Section ─────────────────────────────────────────────────────────

function ScheduleSection({ repoId, canManage }: { repoId: string; canManage: boolean }) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [intervalMins, setIntervalMins] = useState(60);
  const [selectedStages, setSelectedStages] = useState<string[]>([]);
  const [branch, setBranch] = useState('main');
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data } = useQuery({
    queryKey: ['schedules', repoId],
    queryFn: () => listSchedules(repoId),
  });

  // Fetch available stages for this repo
  const { data: stagesData } = useQuery({
    queryKey: ['stages', repoId],
    queryFn: () => listStageConfigs(repoId),
  });
  const availableStages: string[] = (stagesData?.stages ?? []).map((s: { stage_name: string }) => s.stage_name);

  const createMut = useMutation({
    mutationFn: () =>
      createSchedule(repoId, {
        interval_secs: intervalMins * 60,
        stages: selectedStages,
        branch,
      }),
    onSuccess: () => {
      toast.success('Schedule created');
      qc.invalidateQueries({ queryKey: ['schedules', repoId] });
      setShowAdd(false);
      setSelectedStages([]);
      setIntervalMins(60);
      setBranch('main');
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to create schedule'),
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      updateSchedule(repoId, id, { enabled }),
    onSuccess: () => {
      toast.success('Schedule updated');
      qc.invalidateQueries({ queryKey: ['schedules', repoId] });
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to update'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteSchedule(repoId, id),
    onSuccess: () => {
      toast.success('Schedule deleted');
      qc.invalidateQueries({ queryKey: ['schedules', repoId] });
      setDeleteId(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to delete'),
  });

  const schedules: Schedule[] = data?.schedules ?? [];

  const formatInterval = (secs: number) => {
    if (secs < 3600) return `${Math.round(secs / 60)}m`;
    if (secs < 86400) return `${Math.round(secs / 3600)}h`;
    return `${Math.round(secs / 86400)}d`;
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-primary">Cron Schedules</h3>
        {canManage && (
          <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent">
            Add Schedule
          </button>
        )}
      </div>

      <div className="bg-surface border border-border rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border text-left text-xs text-muted uppercase">
              <th className="px-4 py-3">Interval</th>
              <th className="px-4 py-3">Branch</th>
              <th className="px-4 py-3">Stages</th>
              <th className="px-4 py-3">Status</th>
              <th className="px-4 py-3">Next Run</th>
              {canManage && <th className="px-4 py-3">Actions</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {schedules.map((s) => (
              <tr key={s.id} className="hover:bg-surface-hover/50">
                <td className="px-4 py-3 text-secondary font-mono">{formatInterval(s.interval_secs)}</td>
                <td className="px-4 py-3 text-secondary">{s.branch}</td>
                <td className="px-4 py-3 text-secondary">
                  {s.stages.map((st) => (
                    <span key={st} className="inline-block px-2 py-0.5 text-xs bg-surface-2 text-secondary rounded mr-1 mb-1">{st}</span>
                  ))}
                </td>
                <td className="px-4 py-3">
                  {canManage ? (
                    <button
                      onClick={() => toggleMut.mutate({ id: s.id, enabled: !s.enabled })}
                      className={`px-2 py-0.5 text-xs rounded font-medium ${s.enabled ? 'bg-success-soft text-success' : 'bg-surface-2 text-muted'}`}
                    >
                      {s.enabled ? 'Enabled' : 'Disabled'}
                    </button>
                  ) : (
                    <span className={`px-2 py-0.5 text-xs rounded font-medium ${s.enabled ? 'bg-success-soft text-success' : 'bg-surface-2 text-muted'}`}>
                      {s.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                  )}
                </td>
                <td className="px-4 py-3 text-muted text-xs">
                  <TimeAgo date={s.next_run_at} />
                </td>
                {canManage && (
                  <td className="px-4 py-3">
                    <button onClick={() => setDeleteId(s.id)} className="text-xs text-danger hover:text-danger">Delete</button>
                  </td>
                )}
              </tr>
            ))}
            {!schedules.length && (
              <tr>
                <td colSpan={canManage ? 6 : 5} className="px-4 py-6 text-center text-disabled text-sm">
                  No cron schedules configured.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-surface border border-border rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-primary mb-4">Add Cron Schedule</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-secondary mb-1">Interval (minutes)</label>
                <input type="number" min={1} value={intervalMins} onChange={(e) => setIntervalMins(Number(e.target.value))}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent" />
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Branch</label>
                <input value={branch} onChange={(e) => setBranch(e.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent" placeholder="main" />
              </div>
              <div>
                <label className="block text-sm text-secondary mb-1">Stages</label>
                {availableStages.length > 0 ? (
                  <div className="space-y-1.5 max-h-40 overflow-y-auto bg-input border border-border rounded-lg p-2">
                    {availableStages.map((name) => (
                      <label key={name} className="flex items-center gap-2 px-2 py-1 rounded hover:bg-surface-hover cursor-pointer">
                        <input
                          type="checkbox"
                          checked={selectedStages.includes(name)}
                          onChange={(e) => {
                            if (e.target.checked) setSelectedStages((prev) => [...prev, name]);
                            else setSelectedStages((prev) => prev.filter((s) => s !== name));
                          }}
                          className="rounded border-border bg-input accent-accent focus:ring-accent"
                        />
                        <span className="text-sm text-secondary font-mono">{name}</span>
                      </label>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-disabled italic">No stages configured for this repo. Add stages first.</p>
                )}
                {selectedStages.length > 0 && (
                  <p className="mt-1 text-xs text-muted">Selected: {selectedStages.join(', ')}</p>
                )}
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">Cancel</button>
              <button onClick={() => createMut.mutate()} disabled={selectedStages.length === 0 || createMut.isPending}
                className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">Create</button>
            </div>
          </div>
        </div>
      )}

      <ConfirmDialog open={deleteId !== null} title="Delete Schedule" message="This schedule will be permanently removed."
        confirmLabel="Delete" variant="danger" onConfirm={() => deleteId && deleteMut.mutate(deleteId)} onCancel={() => setDeleteId(null)} />
    </div>
  );
}

// ── Global Scripts Section ───────────────────────────────────────────────────

function GlobalScriptsSection({ repoId, canManage }: { repoId: string; canManage: boolean }) {
  const qc = useQueryClient();
  const { data: repo } = useQuery({ queryKey: ['repo', repoId], queryFn: () => getRepo(repoId) });

  const [preScript, setPreScript] = useState<string>('');
  const [preScope, setPreScope] = useState<string>('worker');
  const [preLockEnabled, setPreLockEnabled] = useState(false);
  const [preLockKey, setPreLockKey] = useState('');
  const [preLockTimeoutSecs, setPreLockTimeoutSecs] = useState(120);
  const [postScript, setPostScript] = useState<string>('');
  const [postScope, setPostScope] = useState<string>('worker');
  const [postLockEnabled, setPostLockEnabled] = useState(false);
  const [postLockKey, setPostLockKey] = useState('');
  const [postLockTimeoutSecs, setPostLockTimeoutSecs] = useState(120);
  const [editing, setEditing] = useState(false);

  // Sync local state when repo data loads
  React.useEffect(() => {
    if (repo) {
      setPreScript(repo.global_pre_script ?? '');
      setPreScope(repo.global_pre_script_scope ?? 'worker');
      setPreLockEnabled(repo.global_pre_script_lock_enabled ?? false);
      setPreLockKey(repo.global_pre_script_lock_key ?? '');
      setPreLockTimeoutSecs(repo.global_pre_script_lock_timeout_secs ?? 120);
      setPostScript(repo.global_post_script ?? '');
      setPostScope(repo.global_post_script_scope ?? 'worker');
      setPostLockEnabled(repo.global_post_script_lock_enabled ?? false);
      setPostLockKey(repo.global_post_script_lock_key ?? '');
      setPostLockTimeoutSecs(repo.global_post_script_lock_timeout_secs ?? 120);
    }
  }, [repo]);

  const saveMut = useMutation({
    mutationFn: () =>
      updateRepo(repoId, {
        global_pre_script: preScript || null,
        global_pre_script_scope: preScope,
        global_pre_script_lock_enabled: preLockEnabled,
        global_pre_script_lock_key: preLockEnabled && preLockKey.trim() ? preLockKey.trim() : null,
        global_pre_script_lock_timeout_secs: preLockEnabled ? preLockTimeoutSecs : 120,
        global_post_script: postScript || null,
        global_post_script_scope: postScope,
        global_post_script_lock_enabled: postLockEnabled,
        global_post_script_lock_key: postLockEnabled && postLockKey.trim() ? postLockKey.trim() : null,
        global_post_script_lock_timeout_secs: postLockEnabled ? postLockTimeoutSecs : 120,
      }),
    onSuccess: () => {
      toast.success('Global scripts saved');
      qc.invalidateQueries({ queryKey: ['repo', repoId] });
      setEditing(false);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to save'),
  });

  const scopeBadge = (scope: string) => {
    const colors: Record<string, string> = {
      worker: 'bg-success-soft text-success border-success/30',
      master: 'bg-warning-soft text-warning border-warning/30',
      both: 'bg-accent-soft text-accent-text border-accent/30',
    };
    return (
      <span className={`text-xs px-2 py-0.5 rounded border ${colors[scope] || colors.worker}`}>
        {scope.toUpperCase()}
      </span>
    );
  };

  return (
    <div className="bg-surface border border-border rounded-xl overflow-hidden">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <h3 className="text-sm font-semibold text-secondary">Global Scripts</h3>
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted">Runs before first stage / after last stage</span>
          {canManage && !editing && (
            <button
              onClick={() => setEditing(true)}
              className="text-xs px-3 py-1 bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
            >
              Edit
            </button>
          )}
        </div>
      </div>

      <div className="p-4 space-y-4">
        {/* Pre-script */}
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="text-xs px-2 py-0.5 rounded border bg-accent-soft text-accent-text border-accent/30">PRE</span>
            {editing ? (
              <select
                value={preScope}
                onChange={(e) => setPreScope(e.target.value)}
                className="text-xs bg-input border border-border rounded px-2 py-1 text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="worker">Worker</option>
                <option value="master">Controller</option>
                <option value="both">Both</option>
              </select>
            ) : (
              scopeBadge(preScope)
            )}
            {!editing && (
              preLockEnabled ? (
                <span className="text-xs px-2 py-0.5 rounded border bg-info-soft text-warning border-warning/30">
                  LOCK {preLockKey || '(default key)'} — {preLockTimeoutSecs}s
                </span>
              ) : (
                <span className="text-xs text-disabled">No lock</span>
              )
            )}
            {editing && (
              <label className="flex items-center gap-1.5 cursor-pointer select-none ml-2">
                <input type="checkbox" checked={preLockEnabled} onChange={(e) => setPreLockEnabled(e.target.checked)}
                  className="rounded border-border bg-input accent-accent focus:ring-accent" />
                <span className="text-xs text-secondary">Lock</span>
              </label>
            )}
            <span className="text-xs text-muted">Runs before first stage of every build</span>
          </div>
          {editing ? (
            <>
              <textarea
                value={preScript}
                onChange={(e) => setPreScript(e.target.value)}
                rows={6}
                className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary font-mono text-xs focus:outline-none focus:ring-2 focus:ring-accent resize-y"
                placeholder="#!/bin/bash&#10;set -e&#10;# Workspace setup script..."
              />
              {preLockEnabled && (
                <div className="mt-2 flex gap-3 flex-wrap border-l-2 border-warning/40 pl-3">
                  <div className="flex-1 min-w-48">
                    <label className="block text-xs text-muted mb-1">Lock Key</label>
                    <input value={preLockKey} onChange={(e) => setPreLockKey(e.target.value)}
                      className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs font-mono focus:outline-none focus:ring-2 focus:ring-accent"
                      placeholder="{{WORKER_ID}}-{{REPO_NAME}}-pre" />
                    <p className="text-xs text-muted mt-0.5">{'{{WORKER_ID}} {{REPO_NAME}} {{COMMIT_SHA}} {{BRANCH}} {{JOB_GROUP_ID}}'}</p>
                  </div>
                  <div>
                    <label className="block text-xs text-muted mb-1">Timeout (secs)</label>
                    <input type="number" min={1} value={preLockTimeoutSecs} onChange={(e) => setPreLockTimeoutSecs(Number(e.target.value))}
                      className="w-28 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent" />
                  </div>
                </div>
              )}
            </>
          ) : preScript ? (
            <pre className="px-3 py-2 bg-surface-2 border border-border rounded-lg text-xs text-secondary font-mono overflow-x-auto max-h-40 overflow-y-auto whitespace-pre-wrap">
              {preScript}
            </pre>
          ) : (
            <p className="text-xs text-disabled italic">No global pre-script configured</p>
          )}
        </div>

        {/* Post-script */}
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="text-xs px-2 py-0.5 rounded border bg-pending-soft text-pending border-pending/30">POST</span>
            {editing ? (
              <select
                value={postScope}
                onChange={(e) => setPostScope(e.target.value)}
                className="text-xs bg-input border border-border rounded px-2 py-1 text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="worker">Worker</option>
                <option value="master">Controller</option>
                <option value="both">Both</option>
              </select>
            ) : (
              scopeBadge(postScope)
            )}
            {!editing && (
              postLockEnabled ? (
                <span className="text-xs px-2 py-0.5 rounded border bg-info-soft text-warning border-warning/30">
                  LOCK {postLockKey || '(default key)'} — {postLockTimeoutSecs}s
                </span>
              ) : (
                <span className="text-xs text-disabled">No lock</span>
              )
            )}
            {editing && (
              <label className="flex items-center gap-1.5 cursor-pointer select-none ml-2">
                <input type="checkbox" checked={postLockEnabled} onChange={(e) => setPostLockEnabled(e.target.checked)}
                  className="rounded border-border bg-input accent-accent focus:ring-accent" />
                <span className="text-xs text-secondary">Lock</span>
              </label>
            )}
            <span className="text-xs text-muted">Runs after last stage of every build</span>
          </div>
          {editing ? (
            <>
              <textarea
                value={postScript}
                onChange={(e) => setPostScript(e.target.value)}
                rows={4}
                className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary font-mono text-xs focus:outline-none focus:ring-2 focus:ring-accent resize-y"
                placeholder="#!/bin/bash&#10;# Cleanup, notifications..."
              />
              {postLockEnabled && (
                <div className="mt-2 flex gap-3 flex-wrap border-l-2 border-warning/40 pl-3">
                  <div className="flex-1 min-w-48">
                    <label className="block text-xs text-muted mb-1">Lock Key</label>
                    <input value={postLockKey} onChange={(e) => setPostLockKey(e.target.value)}
                      className="w-full px-2 py-1 bg-input border border-border rounded text-primary text-xs font-mono focus:outline-none focus:ring-2 focus:ring-accent"
                      placeholder="{{WORKER_ID}}-{{REPO_NAME}}-post" />
                    <p className="text-xs text-muted mt-0.5">{'{{WORKER_ID}} {{REPO_NAME}} {{COMMIT_SHA}} {{BRANCH}} {{JOB_GROUP_ID}}'}</p>
                  </div>
                  <div>
                    <label className="block text-xs text-muted mb-1">Timeout (secs)</label>
                    <input type="number" min={1} value={postLockTimeoutSecs} onChange={(e) => setPostLockTimeoutSecs(Number(e.target.value))}
                      className="w-28 px-2 py-1 bg-input border border-border rounded text-primary text-xs focus:outline-none focus:ring-2 focus:ring-accent" />
                  </div>
                </div>
              )}
            </>
          ) : postScript ? (
            <pre className="px-3 py-2 bg-surface-2 border border-border rounded-lg text-xs text-secondary font-mono overflow-x-auto max-h-40 overflow-y-auto whitespace-pre-wrap">
              {postScript}
            </pre>
          ) : (
            <p className="text-xs text-disabled italic">No global post-script configured</p>
          )}
        </div>

        {/* Save/Cancel buttons */}
        {editing && (
          <div className="flex justify-end gap-3 pt-2">
            <button
              onClick={() => {
                setEditing(false);
                setPreScript(repo?.global_pre_script ?? '');
                setPreScope(repo?.global_pre_script_scope ?? 'worker');
                setPreLockEnabled(repo?.global_pre_script_lock_enabled ?? false);
                setPreLockKey(repo?.global_pre_script_lock_key ?? '');
                setPreLockTimeoutSecs(repo?.global_pre_script_lock_timeout_secs ?? 120);
                setPostScript(repo?.global_post_script ?? '');
                setPostScope(repo?.global_post_script_scope ?? 'worker');
                setPostLockEnabled(repo?.global_post_script_lock_enabled ?? false);
                setPostLockKey(repo?.global_post_script_lock_key ?? '');
                setPostLockTimeoutSecs(repo?.global_post_script_lock_timeout_secs ?? 120);
              }}
              className="px-4 py-2 text-sm text-secondary bg-surface-2 rounded-lg hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={() => saveMut.mutate()}
              disabled={saveMut.isPending}
              className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
            >
              {saveMut.isPending ? 'Saving...' : 'Save'}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function RepoDetailPage() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();

  const { data: repo } = useQuery({ queryKey: ['repo', id], queryFn: () => getRepo(id!), enabled: !!id });
  const { data: stagesData } = useQuery({ queryKey: ['stages', id], queryFn: () => listStageConfigs(id!), enabled: !!id });

  const toggleEnabled = useMutation({
    mutationFn: () => updateRepo(id!, { enabled: !repo?.enabled }),
    onSuccess: () => {
      toast.success(repo?.enabled ? 'Repo disabled' : 'Repo enabled');
      qc.invalidateQueries({ queryKey: ['repo', id] });
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to update repo'),
  });

  const stageCount = stagesData?.stages?.length ?? 0;

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <button onClick={() => nav('/repos')} className="text-muted hover:text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent rounded">
          &lt; Repos
        </button>
        <h2 className="text-2xl font-bold text-primary">{repo?.repo_name || 'Loading...'}</h2>
      </div>

      {repo && (
        <div className="bg-surface border border-border rounded-xl p-4 grid grid-cols-2 md:grid-cols-4 gap-4">
          <div><p className="text-xs text-muted">URL</p><p className="text-sm text-secondary font-mono truncate">{repo.repo_url}</p></div>
          <div><p className="text-xs text-muted">Default Branch</p><p className="text-sm text-secondary">{repo.default_branch}</p></div>
          <div>
            <p className="text-xs text-muted">Status</p>
            {canManageRepos ? (
              <button
                onClick={() => toggleEnabled.mutate()}
                disabled={toggleEnabled.isPending}
                className={`mt-1 px-3 py-1 text-xs rounded-full font-medium transition-colors ${
                  repo.enabled
                    ? 'bg-success-soft text-success hover:opacity-80'
                    : 'bg-danger-soft text-danger hover:opacity-80'
                } disabled:opacity-50`}
              >
                {repo.enabled ? 'Enabled' : 'Disabled'}
              </button>
            ) : (
              <span className={`inline-block mt-1 px-3 py-1 text-xs rounded-full font-medium ${repo.enabled ? 'bg-success-soft text-success' : 'bg-danger-soft text-danger'}`}>
                {repo.enabled ? 'Enabled' : 'Disabled'}
              </span>
            )}
          </div>
          <div><p className="text-xs text-muted">Stages</p><p className="text-sm text-secondary">{stageCount}</p></div>
        </div>
      )}

      {id && <GlobalScriptsSection repoId={id} canManage={canManageRepos} />}
      {id && <StageSection repoId={id} canManage={canManageRepos} />}
      {id && <ScheduleSection repoId={id} canManage={canManageRepos} />}
      {id && <WebhookSection repoId={id} canManage={canManageRepos} />}
      {id && <RepoBlacklistSection repoId={id} canManage={canManageRepos} />}
    </div>
  );
}
