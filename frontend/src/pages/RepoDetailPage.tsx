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
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editType, setEditType] = useState<'pre' | 'post'>('pre');
  const [editScope, setEditScope] = useState<'worker' | 'master'>('worker');
  const [editContent, setEditContent] = useState('');
  const [editWorkerId, setEditWorkerId] = useState('');
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ['scripts', repoId, stageId],
    queryFn: () => listScripts(repoId, stageId),
  });

  const scripts: StageScript[] = data?.scripts ?? [];

  const inputCls = 'w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500';

  const resetAddForm = () => {
    setScriptType('pre');
    setScriptScope('worker');
    setScriptContent('');
    setWorkerId('');
  };

  const startEdit = (s: StageScript) => {
    setEditingId(s.id);
    setEditType(s.script_type);
    setEditScope(s.script_scope);
    setEditContent(s.script);
    setEditWorkerId(s.worker_id ?? '');
  };

  const cancelEdit = () => setEditingId(null);

  const createMut = useMutation({
    mutationFn: () => {
      const req: CreateScriptRequest = {
        script_type: scriptType,
        script_scope: scriptScope,
        script: scriptContent,
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
    <div className="px-4 py-3 bg-slate-800/40 border-t border-slate-700/50">
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs font-semibold text-slate-400 uppercase tracking-wide">Scripts</span>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-2 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Add Script
          </button>
        )}
      </div>

      {isLoading ? (
        <p className="text-xs text-slate-500 italic">Loading scripts...</p>
      ) : scripts.length === 0 ? (
        <p className="text-xs text-slate-500 italic">No scripts configured for this stage.</p>
      ) : (
        <div className="space-y-2">
          {scripts.map((s) => {
            const isEditing = editingId === s.id;
            if (isEditing) {
              return (
                <div key={s.id} className="bg-slate-900 border border-slate-600 rounded-lg p-3 space-y-2">
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="block text-xs text-slate-400 mb-1">Type</label>
                      <select value={editType} onChange={(e) => setEditType(e.target.value as 'pre' | 'post')}
                        className="w-full px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500">
                        <option value="pre">pre</option>
                        <option value="post">post</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-xs text-slate-400 mb-1">Scope</label>
                      <select value={editScope} onChange={(e) => setEditScope(e.target.value as 'worker' | 'master')}
                        className="w-full px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500">
                        <option value="worker">worker</option>
                        <option value="master">master</option>
                      </select>
                    </div>
                  </div>
                  <div>
                    <label className="block text-xs text-slate-400 mb-1">Worker ID (optional)</label>
                    <input value={editWorkerId} onChange={(e) => setEditWorkerId(e.target.value)}
                      className="w-full px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
                      placeholder="Leave blank for all workers" />
                  </div>
                  <div>
                    <label className="block text-xs text-slate-400 mb-1">Script</label>
                    <textarea value={editContent} onChange={(e) => setEditContent(e.target.value)} rows={4}
                      className="w-full px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 resize-y"
                      placeholder="#!/bin/bash&#10;echo hello" />
                  </div>
                  <div className="flex gap-2 justify-end">
                    <button onClick={cancelEdit}
                      className="px-2 py-1 text-xs text-slate-400 hover:text-slate-200 focus:outline-none focus:ring-1 focus:ring-slate-500 rounded">
                      Cancel
                    </button>
                    <button onClick={() => updateMut.mutate(s.id)} disabled={!editContent || updateMut.isPending}
                      className="px-2 py-1 text-xs bg-green-700 text-white rounded hover:bg-green-600 disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-green-500">
                      Save
                    </button>
                  </div>
                </div>
              );
            }
            return (
              <div key={s.id} className="bg-slate-900 border border-slate-700 rounded-lg p-3">
                <div className="flex items-center gap-2 mb-2">
                  <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${
                    s.script_type === 'pre'
                      ? 'bg-blue-900/40 text-blue-300 border border-blue-700/50'
                      : 'bg-purple-900/40 text-purple-300 border border-purple-700/50'
                  }`}>
                    {s.script_type.toUpperCase()}
                  </span>
                  <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${
                    s.script_scope === 'worker'
                      ? 'bg-emerald-900/40 text-emerald-300 border border-emerald-700/50'
                      : 'bg-amber-900/40 text-amber-300 border border-amber-700/50'
                  }`}>
                    {s.script_scope.toUpperCase()}
                  </span>
                  {s.worker_id && (
                    <span className="text-xs text-slate-500 font-mono">worker: {s.worker_id}</span>
                  )}
                  {canManage && (
                    <div className="ml-auto flex gap-2">
                      <button
                        onClick={() => startEdit(s)}
                        disabled={editingId !== null}
                        className="text-xs text-blue-400 hover:text-blue-300 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => setDeleteId(s.id)}
                        disabled={editingId !== null}
                        className="text-xs text-red-400 hover:text-red-300 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-red-500 rounded"
                      >
                        Delete
                      </button>
                    </div>
                  )}
                </div>
                <pre className="text-xs text-slate-300 font-mono bg-slate-800 rounded p-2 overflow-x-auto whitespace-pre-wrap break-words max-h-40">
                  {s.script}
                </pre>
              </div>
            );
          })}
        </div>
      )}

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full">
            <h3 className="text-lg font-semibold text-white mb-4">Add Script</h3>
            <div className="space-y-3">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-sm text-slate-300 mb-1">Type</label>
                  <select value={scriptType} onChange={(e) => setScriptType(e.target.value as 'pre' | 'post')} className={inputCls}>
                    <option value="pre">pre (runs before stage)</option>
                    <option value="post">post (runs after stage)</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm text-slate-300 mb-1">Scope</label>
                  <select value={scriptScope} onChange={(e) => setScriptScope(e.target.value as 'worker' | 'master')} className={inputCls}>
                    <option value="worker">worker (runs on worker node)</option>
                    <option value="master">master (runs on master node)</option>
                  </select>
                </div>
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Worker ID (optional)</label>
                <input value={workerId} onChange={(e) => setWorkerId(e.target.value)} className={inputCls}
                  placeholder="Leave blank to target all workers" />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Script</label>
                <textarea value={scriptContent} onChange={(e) => setScriptContent(e.target.value)} rows={6}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 resize-y"
                  placeholder={'#!/bin/bash\necho "pre-stage hook"'} />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => { setShowAdd(false); resetAddForm(); }}
                className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700 focus:outline-none focus:ring-2 focus:ring-blue-500">
                Cancel
              </button>
              <button onClick={() => createMut.mutate()} disabled={!scriptContent || createMut.isPending}
                className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500">
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
    <div className="bg-slate-800/50 border border-slate-700 rounded-lg px-4 py-3">
      <p className="text-xs text-slate-500 mb-1.5 uppercase font-semibold">Resource Requirements (max across stages)</p>
      <div className="flex gap-6 text-sm">
        <span className="text-slate-300">CPU: <span className="text-white font-medium">{maxCpu} cores</span></span>
        <span className="text-slate-300">Memory: <span className="text-white font-medium">{formatResourceValue(maxMem)}</span></span>
        <span className="text-slate-300">Disk: <span className="text-white font-medium">{formatResourceValue(maxDisk)}</span></span>
        <span className="text-slate-300">Max Duration: <span className="text-white font-medium">{formatDurationSecs(maxDuration)}</span></span>
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
  const inputCls = "w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500";

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-white">Stage Configs</h3>
        {canManage && (
          <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500">
            Add Stage
          </button>
        )}
      </div>

      <ResourceSummary stages={stages} />

      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        <table className="w-full" aria-label="Stage configurations">
          <thead>
            <tr className="border-b border-slate-700">
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Order</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Name</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Command</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Mode</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">CPU</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Memory</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Disk</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Type</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Timeout</th>
              <th className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase">Scripts</th>
              {canManage && <th className="px-4 py-3 text-xs text-slate-400 uppercase">Actions</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800">
            {stages.map(s => {
              const isEditing = editingStageId === s.id;
              if (isEditing) {
                return (
                  <tr key={s.id} className="bg-slate-800/40">
                    <td className="px-4 py-2 text-sm text-slate-400">{s.execution_order}</td>
                    <td className="px-4 py-2 text-sm text-slate-200 font-medium">{s.stage_name}</td>
                    <td className="px-4 py-2">
                      <textarea
                        value={editCommand}
                        onChange={e => setEditCommand(e.target.value)}
                        rows={2}
                        className="w-full px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
                        placeholder={editCommandMode === 'required' ? 'user-provided' : 'make build'}
                      />
                    </td>
                    <td className="px-4 py-2">
                      <select
                        value={editCommandMode}
                        onChange={e => setEditCommandMode(e.target.value)}
                        className="w-full px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
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
                        className="w-16 px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
                      />
                    </td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0}
                        value={editMemoryMb}
                        onChange={e => setEditMemoryMb(Number(e.target.value))}
                        className="w-20 px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
                      />
                    </td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0}
                        value={editDiskMb}
                        onChange={e => setEditDiskMb(Number(e.target.value))}
                        className="w-20 px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
                      />
                    </td>
                    <td className="px-4 py-2 text-sm text-slate-400">{s.job_type}</td>
                    <td className="px-4 py-2">
                      <input
                        type="number" min={0} max={86400}
                        value={editDurationSecs}
                        onChange={e => setEditDurationSecs(Number(e.target.value))}
                        className="w-20 px-2 py-1 bg-slate-800 border border-slate-600 rounded text-white text-xs focus:outline-none focus:ring-2 focus:ring-blue-500"
                      />
                    </td>
                    <td className="px-4 py-2"></td>
                    <td className="px-4 py-2 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <button
                          onClick={() => updateStage.mutate(s.id)}
                          disabled={updateStage.isPending}
                          className="text-xs text-green-400 hover:text-green-300 disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-green-500 rounded"
                        >
                          Save
                        </button>
                        <button
                          onClick={cancelEdit}
                          className="text-xs text-slate-400 hover:text-slate-300 focus:outline-none focus:ring-1 focus:ring-slate-500 rounded"
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
                  <tr className={isExpanded ? 'bg-slate-800/20' : undefined}>
                    <td className="px-4 py-3 text-sm text-slate-400">{s.execution_order}</td>
                    <td className="px-4 py-3 text-sm text-slate-200 font-medium">{s.stage_name}</td>
                    <td className="px-4 py-3 text-sm text-slate-400 font-mono truncate max-w-xs">{s.command || <span className="italic text-slate-600">user-provided</span>}</td>
                    <td className="px-4 py-3 text-xs text-slate-400">{s.command_mode ?? 'fixed'}</td>
                    <td className="px-4 py-3 text-xs text-slate-400">{s.required_cpu}c</td>
                    <td className="px-4 py-3 text-xs text-slate-400">{formatResourceValue(s.required_memory_mb)}</td>
                    <td className="px-4 py-3 text-xs text-slate-400">{formatResourceValue(s.required_disk_mb)}</td>
                    <td className="px-4 py-3 text-sm text-slate-400">{s.job_type}</td>
                    <td className="px-4 py-3 text-sm text-slate-400">{formatDurationSecs(s.max_duration_secs)}</td>
                    <td className="px-4 py-3 text-center">
                      <button
                        onClick={() => setExpandedStageId(isExpanded ? null : s.id)}
                        aria-expanded={isExpanded}
                        aria-label={`${isExpanded ? 'Collapse' : 'Expand'} scripts for ${s.stage_name}`}
                        className="text-slate-400 hover:text-white focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
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
                            className="text-xs text-blue-400 hover:text-blue-300 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                          >
                            Edit
                          </button>
                          <button
                            onClick={() => delStage.mutate(s.id)}
                            disabled={editingStageId !== null}
                            className="text-xs text-red-400 hover:text-red-300 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-red-500 rounded"
                          >
                            Delete
                          </button>
                        </div>
                      </td>
                    )}
                  </tr>
                  {isExpanded && (
                    <tr>
                      <td colSpan={colSpanTotal} className="p-0 border-b border-slate-700/50">
                        <ScriptsPanel repoId={repoId} stageId={s.id} canManage={canManage} />
                      </td>
                    </tr>
                  )}
                </React.Fragment>
              );
            })}
            {!stages.length && (
              <tr><td colSpan={canManage ? 11 : 10} className="px-4 py-8 text-center text-slate-500">No stages configured</td></tr>
            )}
          </tbody>
        </table>
      </div>

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-4">Add Stage</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-slate-300 mb-1">Stage Name</label>
                <input value={stageName} onChange={e => setStageName(e.target.value)} className={inputCls} placeholder="build" />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Command Mode</label>
                <select value={commandMode} onChange={e => setCommandMode(e.target.value)} className={inputCls}>
                  <option value="fixed">Fixed (always use configured command)</option>
                  <option value="optional">Optional (user can override)</option>
                  <option value="required">Required (user must provide)</option>
                </select>
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Command {commandMode === 'required' ? '(optional)' : ''}</label>
                <input value={command} onChange={e => setCommand(e.target.value)} className={inputCls} placeholder={commandMode === 'required' ? 'User will provide at runtime' : 'make build'} />
                {commandMode === 'optional' && <p className="text-xs text-slate-500 mt-1">User can override this command at submission time</p>}
                {commandMode === 'required' && <p className="text-xs text-slate-500 mt-1">User must provide a command when submitting this stage</p>}
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-sm text-slate-300 mb-1">CPU (cores)</label>
                  <input type="number" min={0} max={1024} value={requiredCpu} onChange={e => setRequiredCpu(Number(e.target.value))} className={inputCls} />
                </div>
                <div>
                  <label className="block text-sm text-slate-300 mb-1">Memory (MB)</label>
                  <input type="number" min={0} value={requiredMemoryMb} onChange={e => setRequiredMemoryMb(Number(e.target.value))} className={inputCls} />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-sm text-slate-300 mb-1">Disk (MB)</label>
                  <input type="number" min={0} value={requiredDiskMb} onChange={e => setRequiredDiskMb(Number(e.target.value))} className={inputCls} />
                </div>
                <div>
                  <label className="block text-sm text-slate-300 mb-1">Timeout (seconds)</label>
                  <input type="number" min={0} max={86400} value={maxDurationSecs} onChange={e => setMaxDurationSecs(Number(e.target.value))} className={inputCls} />
                </div>
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => { setShowAdd(false); resetForm(); }} className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700">Cancel</button>
              <button onClick={() => addStage.mutate()} disabled={!stageName || (commandMode !== 'required' && !command)} className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700">Create</button>
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

  if (isLoading) return <div className="px-6 py-4 text-sm text-slate-400">Loading deliveries…</div>;
  if (isError) return <div className="px-6 py-4 text-sm text-slate-500">No delivery history available.</div>;

  const deliveries = data?.deliveries ?? [];

  if (!deliveries.length) {
    return <div className="px-6 py-4 text-sm text-slate-500">No deliveries yet.</div>;
  }

  return (
    <table className="w-full" aria-label="Webhook delivery history">
      <thead>
        <tr className="border-b border-slate-700/50">
          <th className="px-6 py-2 text-left text-xs font-semibold text-slate-500 uppercase">Delivered</th>
          <th className="px-6 py-2 text-left text-xs font-semibold text-slate-500 uppercase">Event</th>
          <th className="px-6 py-2 text-left text-xs font-semibold text-slate-500 uppercase">Status</th>
          <th className="px-6 py-2 text-left text-xs font-semibold text-slate-500 uppercase">Response Time</th>
        </tr>
      </thead>
      <tbody className="divide-y divide-slate-800">
        {deliveries.map(d => (
          <tr key={d.id}>
            <td className="px-6 py-2 text-sm"><TimeAgo date={d.delivered_at} className="text-slate-500" /></td>
            <td className="px-6 py-2 text-sm text-slate-300">{d.event}</td>
            <td className="px-6 py-2 text-sm">
              <span className={d.success ? 'text-green-400' : 'text-red-400'}>
                {d.status_code}
              </span>
            </td>
            <td className="px-6 py-2 text-sm text-slate-400">{d.response_time_ms}ms</td>
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
    <div className="border-b border-slate-800 last:border-0">
      <div className="px-4 py-3 flex items-center gap-3">
        <button
          onClick={() => setExpanded(e => !e)}
          aria-expanded={expanded}
          aria-label={`${expanded ? 'Collapse' : 'Expand'} webhook ${webhook.provider} delivery history`}
          className="text-slate-400 hover:text-white focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
        >
          <svg className={`w-4 h-4 transition-transform ${expanded ? 'rotate-90' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
        <span className="text-sm font-medium text-slate-200 capitalize">{webhook.provider}</span>
        <div className="flex gap-1 flex-wrap">
          {webhook.events.map(e => (
            <span key={e} className="text-xs px-1.5 py-0.5 bg-slate-700 text-slate-300 rounded">{e}</span>
          ))}
        </div>
        <span className={`ml-auto text-xs px-2 py-0.5 rounded ${webhook.enabled ? 'text-green-400 bg-green-500/10' : 'text-slate-500 bg-slate-700'}`}>
          {webhook.enabled ? 'active' : 'disabled'}
        </span>
        {canManage && (
          <button onClick={onDelete} className="text-xs text-red-400 hover:text-red-300 focus:outline-none focus:ring-1 focus:ring-red-500 rounded ml-2">
            Delete
          </button>
        )}
      </div>
      {expanded && (
        <div className="bg-slate-800/30">
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
        <h3 className="text-lg font-semibold text-white">Webhooks</h3>
        {canManage && (
          <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500">
            Add Webhook
          </button>
        )}
      </div>

      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        {webhooks.length === 0 ? (
          <div className="px-4 py-8 text-center text-slate-500">No webhooks configured</div>
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
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-4">Add Webhook</h3>
            <div>
              <label className="block text-sm text-slate-300 mb-1">Provider</label>
              <select value={provider} onChange={e => setProvider(e.target.value as 'github' | 'gitlab')} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-blue-500">
                <option value="github">GitHub</option>
                <option value="gitlab">GitLab</option>
              </select>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700">Cancel</button>
              <button onClick={() => addWebhook.mutate()} disabled={addWebhook.isPending} className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700">Create</button>
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
        <h3 className="text-lg font-semibold text-white">Command Blacklist</h3>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Add Rule
          </button>
        )}
      </div>

      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        <table className="w-full" aria-label="Repo command blacklist">
          <thead>
            <tr className="border-b border-slate-700">
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Pattern</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Description</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
              {canManage && (
                <th className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase text-center">Actions</th>
              )}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800">
            {entries.map((e) => (
              <tr key={e.id}>
                <td className="px-4 py-3 text-sm text-slate-200 font-mono truncate max-w-xs">{e.pattern}</td>
                <td className="px-4 py-3 text-sm text-slate-400">{e.description ?? '—'}</td>
                <td className="px-4 py-3">
                  {canManage ? (
                    <button
                      onClick={() => toggleMut.mutate({ id: e.id, enabled: !e.enabled })}
                      className={`text-xs px-2 py-0.5 rounded border transition-colors focus:outline-none focus:ring-1 ${
                        e.enabled
                          ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20'
                          : 'bg-slate-700 text-slate-400 border-slate-600 hover:bg-slate-600'
                      }`}
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
                {canManage && (
                  <td className="px-4 py-3 text-center">
                    <button
                      onClick={() => setDeleteId(e.id)}
                      className="text-xs text-red-400 hover:text-red-300 focus:outline-none focus:ring-1 focus:ring-red-500 rounded"
                    >
                      Delete
                    </button>
                  </td>
                )}
              </tr>
            ))}
            {!entries.length && (
              <tr>
                <td colSpan={canManage ? 4 : 3} className="px-4 py-6 text-center text-slate-500 text-sm">
                  No blacklist rules for this repo.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

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
        <h3 className="text-lg font-semibold text-white">Cron Schedules</h3>
        {canManage && (
          <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700">
            Add Schedule
          </button>
        )}
      </div>

      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-slate-700 text-left text-xs text-slate-400 uppercase">
              <th className="px-4 py-3">Interval</th>
              <th className="px-4 py-3">Branch</th>
              <th className="px-4 py-3">Stages</th>
              <th className="px-4 py-3">Status</th>
              <th className="px-4 py-3">Next Run</th>
              {canManage && <th className="px-4 py-3">Actions</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800">
            {schedules.map((s) => (
              <tr key={s.id} className="hover:bg-slate-800/50">
                <td className="px-4 py-3 text-slate-200 font-mono">{formatInterval(s.interval_secs)}</td>
                <td className="px-4 py-3 text-slate-300">{s.branch}</td>
                <td className="px-4 py-3 text-slate-300">
                  {s.stages.map((st) => (
                    <span key={st} className="inline-block px-2 py-0.5 text-xs bg-slate-700 rounded mr-1 mb-1">{st}</span>
                  ))}
                </td>
                <td className="px-4 py-3">
                  {canManage ? (
                    <button
                      onClick={() => toggleMut.mutate({ id: s.id, enabled: !s.enabled })}
                      className={`px-2 py-0.5 text-xs rounded font-medium ${s.enabled ? 'bg-emerald-900/40 text-emerald-400' : 'bg-slate-700 text-slate-400'}`}
                    >
                      {s.enabled ? 'Enabled' : 'Disabled'}
                    </button>
                  ) : (
                    <span className={`px-2 py-0.5 text-xs rounded font-medium ${s.enabled ? 'bg-emerald-900/40 text-emerald-400' : 'bg-slate-700 text-slate-400'}`}>
                      {s.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                  )}
                </td>
                <td className="px-4 py-3 text-slate-400 text-xs">
                  <TimeAgo date={s.next_run_at} />
                </td>
                {canManage && (
                  <td className="px-4 py-3">
                    <button onClick={() => setDeleteId(s.id)} className="text-xs text-red-400 hover:text-red-300">Delete</button>
                  </td>
                )}
              </tr>
            ))}
            {!schedules.length && (
              <tr>
                <td colSpan={canManage ? 6 : 5} className="px-4 py-6 text-center text-slate-500 text-sm">
                  No cron schedules configured.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-white mb-4">Add Cron Schedule</h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-slate-300 mb-1">Interval (minutes)</label>
                <input type="number" min={1} value={intervalMins} onChange={(e) => setIntervalMins(Number(e.target.value))}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Branch</label>
                <input value={branch} onChange={(e) => setBranch(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="main" />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Stages</label>
                {availableStages.length > 0 ? (
                  <div className="space-y-1.5 max-h-40 overflow-y-auto bg-slate-800 border border-slate-600 rounded-lg p-2">
                    {availableStages.map((name) => (
                      <label key={name} className="flex items-center gap-2 px-2 py-1 rounded hover:bg-slate-700 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={selectedStages.includes(name)}
                          onChange={(e) => {
                            if (e.target.checked) setSelectedStages((prev) => [...prev, name]);
                            else setSelectedStages((prev) => prev.filter((s) => s !== name));
                          }}
                          className="rounded border-slate-500 bg-slate-700 text-blue-500 focus:ring-blue-500"
                        />
                        <span className="text-sm text-slate-200 font-mono">{name}</span>
                      </label>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-slate-500 italic">No stages configured for this repo. Add stages first.</p>
                )}
                {selectedStages.length > 0 && (
                  <p className="mt-1 text-xs text-slate-400">Selected: {selectedStages.join(', ')}</p>
                )}
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700">Cancel</button>
              <button onClick={() => createMut.mutate()} disabled={selectedStages.length === 0 || createMut.isPending}
                className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700">Create</button>
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
  const [postScript, setPostScript] = useState<string>('');
  const [postScope, setPostScope] = useState<string>('worker');
  const [editing, setEditing] = useState(false);

  // Sync local state when repo data loads
  React.useEffect(() => {
    if (repo) {
      setPreScript(repo.global_pre_script ?? '');
      setPreScope(repo.global_pre_script_scope ?? 'worker');
      setPostScript(repo.global_post_script ?? '');
      setPostScope(repo.global_post_script_scope ?? 'worker');
    }
  }, [repo]);

  const saveMut = useMutation({
    mutationFn: () =>
      updateRepo(repoId, {
        global_pre_script: preScript || null,
        global_pre_script_scope: preScope,
        global_post_script: postScript || null,
        global_post_script_scope: postScope,
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
      worker: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30',
      master: 'bg-amber-500/10 text-amber-400 border-amber-500/30',
      both: 'bg-blue-500/10 text-blue-400 border-blue-500/30',
    };
    return (
      <span className={`text-xs px-2 py-0.5 rounded border ${colors[scope] || colors.worker}`}>
        {scope.toUpperCase()}
      </span>
    );
  };

  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
      <div className="flex items-center justify-between px-4 py-3 border-b border-slate-700">
        <h3 className="text-sm font-semibold text-slate-200">Global Scripts</h3>
        <div className="flex items-center gap-2">
          <span className="text-xs text-slate-500">Runs before first stage / after last stage</span>
          {canManage && !editing && (
            <button
              onClick={() => setEditing(true)}
              className="text-xs px-3 py-1 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
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
            <span className="text-xs px-2 py-0.5 rounded border bg-blue-500/10 text-blue-400 border-blue-500/30">PRE</span>
            {editing ? (
              <select
                value={preScope}
                onChange={(e) => setPreScope(e.target.value)}
                className="text-xs bg-slate-800 border border-slate-600 rounded px-2 py-1 text-slate-200"
              >
                <option value="worker">Worker</option>
                <option value="master">Controller</option>
                <option value="both">Both</option>
              </select>
            ) : (
              scopeBadge(preScope)
            )}
            <span className="text-xs text-slate-500">Runs before first stage of every build</span>
          </div>
          {editing ? (
            <textarea
              value={preScript}
              onChange={(e) => setPreScript(e.target.value)}
              rows={6}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-xs focus:outline-none focus:ring-2 focus:ring-blue-500 resize-y"
              placeholder="#!/bin/bash&#10;set -e&#10;# Workspace setup script..."
            />
          ) : preScript ? (
            <pre className="px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-xs text-slate-300 font-mono overflow-x-auto max-h-40 overflow-y-auto whitespace-pre-wrap">
              {preScript}
            </pre>
          ) : (
            <p className="text-xs text-slate-500 italic">No global pre-script configured</p>
          )}
        </div>

        {/* Post-script */}
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="text-xs px-2 py-0.5 rounded border bg-purple-500/10 text-purple-400 border-purple-500/30">POST</span>
            {editing ? (
              <select
                value={postScope}
                onChange={(e) => setPostScope(e.target.value)}
                className="text-xs bg-slate-800 border border-slate-600 rounded px-2 py-1 text-slate-200"
              >
                <option value="worker">Worker</option>
                <option value="master">Controller</option>
                <option value="both">Both</option>
              </select>
            ) : (
              scopeBadge(postScope)
            )}
            <span className="text-xs text-slate-500">Runs after last stage of every build</span>
          </div>
          {editing ? (
            <textarea
              value={postScript}
              onChange={(e) => setPostScript(e.target.value)}
              rows={4}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-xs focus:outline-none focus:ring-2 focus:ring-blue-500 resize-y"
              placeholder="#!/bin/bash&#10;# Cleanup, notifications..."
            />
          ) : postScript ? (
            <pre className="px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-xs text-slate-300 font-mono overflow-x-auto max-h-40 overflow-y-auto whitespace-pre-wrap">
              {postScript}
            </pre>
          ) : (
            <p className="text-xs text-slate-500 italic">No global post-script configured</p>
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
                setPostScript(repo?.global_post_script ?? '');
                setPostScope(repo?.global_post_script_scope ?? 'worker');
              }}
              className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg hover:bg-slate-700"
            >
              Cancel
            </button>
            <button
              onClick={() => saveMut.mutate()}
              disabled={saveMut.isPending}
              className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700"
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
        <button onClick={() => nav('/repos')} className="text-slate-400 hover:text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 rounded">
          &lt; Repos
        </button>
        <h2 className="text-2xl font-bold text-white">{repo?.repo_name || 'Loading...'}</h2>
      </div>

      {repo && (
        <div className="bg-slate-900 border border-slate-700 rounded-xl p-4 grid grid-cols-2 md:grid-cols-4 gap-4">
          <div><p className="text-xs text-slate-500">URL</p><p className="text-sm text-slate-200 font-mono truncate">{repo.repo_url}</p></div>
          <div><p className="text-xs text-slate-500">Default Branch</p><p className="text-sm text-slate-200">{repo.default_branch}</p></div>
          <div>
            <p className="text-xs text-slate-500">Status</p>
            {canManageRepos ? (
              <button
                onClick={() => toggleEnabled.mutate()}
                disabled={toggleEnabled.isPending}
                className={`mt-1 px-3 py-1 text-xs rounded-full font-medium transition-colors ${
                  repo.enabled
                    ? 'bg-emerald-900/40 text-emerald-400 hover:bg-emerald-900/60'
                    : 'bg-red-900/40 text-red-400 hover:bg-red-900/60'
                } disabled:opacity-50`}
              >
                {repo.enabled ? 'Enabled' : 'Disabled'}
              </button>
            ) : (
              <span className={`inline-block mt-1 px-3 py-1 text-xs rounded-full font-medium ${repo.enabled ? 'bg-emerald-900/40 text-emerald-400' : 'bg-red-900/40 text-red-400'}`}>
                {repo.enabled ? 'Enabled' : 'Disabled'}
              </span>
            )}
          </div>
          <div><p className="text-xs text-slate-500">Stages</p><p className="text-sm text-slate-200">{stageCount}</p></div>
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
