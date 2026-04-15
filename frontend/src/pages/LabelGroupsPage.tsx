import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  listLabelGroups,
  createLabelGroup,
  updateLabelGroup,
  deleteLabelGroup,
} from '../api/labelGroups';
import type { LabelGroup, LabelGroupRequest } from '../api/labelGroups';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { EmptyState } from '../components/ui/EmptyState';
import { PageSkeleton } from '../components/ui/PageSkeleton';
import { TimeAgo } from '../components/ui/TimeAgo';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';
import type { MutationError } from '../types';

// ── Tag input ─────────────────────────────────────────────────────────────────

function TagInput({
  label,
  tags,
  onChange,
  placeholder,
}: {
  label: string;
  tags: string[];
  onChange: (tags: string[]) => void;
  placeholder?: string;
}) {
  const [input, setInput] = useState('');

  function addTag() {
    const v = input.trim();
    if (v && !tags.includes(v)) {
      onChange([...tags, v]);
    }
    setInput('');
  }

  function removeTag(t: string) {
    onChange(tags.filter((x) => x !== t));
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addTag();
    } else if (e.key === 'Backspace' && !input && tags.length > 0) {
      onChange(tags.slice(0, -1));
    }
  }

  return (
    <div>
      <label className="block text-sm text-slate-300 mb-1">{label}</label>
      <div className="flex flex-wrap gap-1 p-2 bg-slate-800 border border-slate-600 rounded-lg min-h-[42px]">
        {tags.map((t) => (
          <span
            key={t}
            className="flex items-center gap-1 px-2 py-0.5 bg-slate-700 text-slate-200 rounded text-xs"
          >
            {t}
            <button
              type="button"
              onClick={() => removeTag(t)}
              className="text-slate-400 hover:text-white focus:outline-none"
              aria-label={`Remove ${t}`}
            >
              &times;
            </button>
          </span>
        ))}
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          onBlur={addTag}
          className="flex-1 min-w-[120px] bg-transparent text-white text-sm outline-none placeholder-slate-500"
          placeholder={placeholder ?? 'Type and press Enter'}
        />
      </div>
    </div>
  );
}

// ── Key-value editor ──────────────────────────────────────────────────────────

function KvEditor({
  label,
  value,
  onChange,
}: {
  label: string;
  value: Record<string, string>;
  onChange: (v: Record<string, string>) => void;
}) {
  const [key, setKey] = useState('');
  const [val, setVal] = useState('');
  const entries = Object.entries(value);

  function add() {
    if (key.trim()) {
      onChange({ ...value, [key.trim()]: val });
      setKey('');
      setVal('');
    }
  }

  function remove(k: string) {
    const next = { ...value };
    delete next[k];
    onChange(next);
  }

  return (
    <div>
      <label className="block text-sm text-slate-300 mb-1">{label}</label>
      {entries.length > 0 && (
        <div className="mb-2 space-y-1">
          {entries.map(([k, v]) => (
            <div key={k} className="flex items-center gap-2 text-xs">
              <code className="bg-slate-800 px-2 py-1 rounded text-slate-300 font-mono">{k}</code>
              <span className="text-slate-500">=</span>
              <code className="bg-slate-800 px-2 py-1 rounded text-slate-400 font-mono flex-1 truncate">
                {v}
              </code>
              <button
                type="button"
                onClick={() => remove(k)}
                className="text-red-400 hover:text-red-300 focus:outline-none"
                aria-label={`Remove ${k}`}
              >
                &times;
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="flex gap-2">
        <input
          value={key}
          onChange={(e) => setKey(e.target.value)}
          className="flex-1 px-2 py-1.5 text-xs bg-slate-800 border border-slate-600 rounded text-white font-mono focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="KEY"
        />
        <span className="text-slate-500 self-center">=</span>
        <input
          value={val}
          onChange={(e) => setVal(e.target.value)}
          className="flex-1 px-2 py-1.5 text-xs bg-slate-800 border border-slate-600 rounded text-white font-mono focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="value"
          onKeyDown={(e) => e.key === 'Enter' && add()}
        />
        <button
          type="button"
          onClick={add}
          disabled={!key.trim()}
          className="px-2 py-1 text-xs bg-blue-600/20 text-blue-400 border border-blue-500/30 rounded hover:bg-blue-600/30 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:opacity-50"
        >
          Add
        </button>
      </div>
    </div>
  );
}

// ── Form modal ────────────────────────────────────────────────────────────────

interface FormModalProps {
  initial?: LabelGroup;
  onClose: () => void;
  onSaved: () => void;
}

function LabelGroupFormModal({ initial, onClose, onSaved }: FormModalProps) {
  const [name, setName] = useState(initial?.name ?? '');
  const [matchLabels, setMatchLabels] = useState<string[]>(initial?.match_labels ?? []);
  const [capabilities, setCapabilities] = useState<string[]>(initial?.capabilities ?? []);
  const [envVars, setEnvVars] = useState<Record<string, string>>(
    (initial?.env_vars as Record<string, string>) ?? {},
  );
  const [preScript, setPreScript] = useState(initial?.pre_script ?? '');
  const [maxConcurrent, setMaxConcurrent] = useState(
    String(initial?.max_concurrent_jobs ?? '0'),
  );
  const [priority, setPriority] = useState(String(initial?.priority ?? '0'));
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const isEdit = !!initial;

  const createMut = useMutation({
    mutationFn: (data: LabelGroupRequest) => createLabelGroup(data),
    onSuccess: () => { toast.success('Label group created'); onSaved(); },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create'),
  });

  const updateMut = useMutation({
    mutationFn: (data: LabelGroupRequest) => updateLabelGroup(initial!.id, data),
    onSuccess: () => { toast.success('Label group updated'); onSaved(); },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update'),
  });

  function submit() {
    const data: LabelGroupRequest = {
      name,
      match_labels: matchLabels,
      capabilities,
      env_vars: Object.keys(envVars).length > 0 ? envVars : undefined,
      pre_script: preScript || undefined,
      max_concurrent_jobs: parseInt(maxConcurrent, 10) || 0,
      priority: parseInt(priority, 10) || 0,
      enabled,
    };
    if (isEdit) updateMut.mutate(data);
    else createMut.mutate(data);
  }

  const isPending = createMut.isPending || updateMut.isPending;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 overflow-y-auto">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full my-4">
        <h3 className="text-lg font-semibold text-white mb-4">
          {isEdit ? 'Edit Label Group' : 'Create Label Group'}
        </h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm text-slate-300 mb-1">Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="gpu-workers"
              autoFocus
            />
          </div>

          <TagInput
            label="Match Labels"
            tags={matchLabels}
            onChange={setMatchLabels}
            placeholder="gpu, high-mem..."
          />

          <TagInput
            label="Capabilities"
            tags={capabilities}
            onChange={setCapabilities}
            placeholder="docker, cuda..."
          />

          <KvEditor
            label="Environment Variables"
            value={envVars}
            onChange={setEnvVars}
          />

          <div>
            <label className="block text-sm text-slate-300 mb-1">Pre-script</label>
            <textarea
              value={preScript}
              onChange={(e) => setPreScript(e.target.value)}
              rows={3}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
              placeholder="#!/bin/bash&#10;echo 'Setup...'"
            />
          </div>

          <div>
            <label className="block text-sm text-slate-300 mb-1">Max concurrent jobs (0 = unlimited)</label>
            <input
              type="number"
              min="0"
              value={maxConcurrent}
              onChange={(e) => setMaxConcurrent(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>

          <div>
            <label className="block text-sm text-slate-300 mb-1">Priority (0 = default, higher = preferred)</label>
            <input
              type="number"
              min="0"
              value={priority}
              onChange={(e) => setPriority(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="w-4 h-4 rounded accent-blue-600"
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
            onClick={submit}
            disabled={!name.trim() || isPending}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            {isPending ? 'Saving...' : isEdit ? 'Update' : 'Create'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function LabelGroupsPage() {
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<LabelGroup | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({
    queryKey: ['label-groups'],
    queryFn: listLabelGroups,
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      updateLabelGroup(id, { enabled: !enabled }),
    onSuccess: () => {
      toast.success('Label group updated');
      qc.invalidateQueries({ queryKey: ['label-groups'] });
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteLabelGroup(id),
    onSuccess: () => {
      toast.success('Label group deleted');
      qc.invalidateQueries({ queryKey: ['label-groups'] });
      setDeleteId(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to delete'),
  });

  const groups: LabelGroup[] = data?.data ?? [];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">Label Groups</h2>
        {canManageRepos && (
          <button
            onClick={() => setShowForm(true)}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Create Group
          </button>
        )}
      </div>

      <p className="text-sm text-slate-400">
        Label groups match workers by labels and can apply shared environment variables, pre-scripts,
        and concurrency limits.
      </p>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load label groups. Please try again.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="space-y-3">
          {groups.length === 0 ? (
            <EmptyState
              message="No label groups"
              description="Create a label group to apply shared configuration to matched workers."
            />
          ) : (
            groups.map((g) => (
              <div
                key={g.id}
                className="bg-slate-900 border border-slate-700 rounded-xl p-4"
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap mb-2">
                      <h3 className="text-base font-semibold text-white">{g.name}</h3>
                      <span
                        className={`text-xs px-1.5 py-0.5 rounded border ${
                          g.enabled
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-slate-700 text-slate-400 border-slate-600'
                        }`}
                      >
                        {g.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                      {g.priority > 0 && (
                        <span className="text-xs px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-400 border-blue-500/30">
                          P:{g.priority}
                        </span>
                      )}
                    </div>

                    {g.match_labels.length > 0 && (
                      <div className="flex flex-wrap gap-1 mb-2">
                        <span className="text-xs text-slate-500 self-center mr-1">Labels:</span>
                        {g.match_labels.map((l) => (
                          <span
                            key={l}
                            className="text-xs px-1.5 py-0.5 bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 rounded"
                          >
                            {l}
                          </span>
                        ))}
                      </div>
                    )}

                    {g.capabilities.length > 0 && (
                      <div className="flex flex-wrap gap-1 mb-2">
                        <span className="text-xs text-slate-500 self-center mr-1">Capabilities:</span>
                        {g.capabilities.map((c) => (
                          <span
                            key={c}
                            className="text-xs px-1.5 py-0.5 bg-purple-500/10 text-purple-400 border border-purple-500/30 rounded"
                          >
                            {c}
                          </span>
                        ))}
                      </div>
                    )}

                    <div className="flex flex-wrap gap-4 text-xs text-slate-500 mt-1">
                      {g.max_concurrent_jobs > 0 && (
                        <span>Max concurrent: {g.max_concurrent_jobs}</span>
                      )}
                      {g.env_vars && Object.keys(g.env_vars).length > 0 && (
                        <span>{Object.keys(g.env_vars).length} env var{Object.keys(g.env_vars).length !== 1 ? 's' : ''}</span>
                      )}
                      {g.pre_script && <span>Has pre-script</span>}
                      <span>Updated <TimeAgo date={g.updated_at} /></span>
                    </div>
                  </div>

                  {canManageRepos && (
                    <div className="flex items-center gap-2 shrink-0">
                      <button
                        onClick={() => toggleMut.mutate({ id: g.id, enabled: g.enabled })}
                        disabled={toggleMut.isPending}
                        className={`px-2 py-1 text-xs rounded border focus:outline-none focus:ring-1 ${
                          g.enabled
                            ? 'bg-slate-700 text-slate-300 border-slate-600 hover:bg-slate-600 focus:ring-slate-500'
                            : 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20 focus:ring-emerald-500'
                        }`}
                      >
                        {g.enabled ? 'Disable' : 'Enable'}
                      </button>
                      <button
                        onClick={() => setEditing(g)}
                        className="px-2 py-1 text-xs bg-blue-600/10 text-blue-400 border border-blue-500/30 rounded hover:bg-blue-600/20 focus:outline-none focus:ring-1 focus:ring-blue-500"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => setDeleteId(g.id)}
                        className="px-2 py-1 text-xs bg-red-500/10 text-red-400 border border-red-500/30 rounded hover:bg-red-500/20 focus:outline-none focus:ring-1 focus:ring-red-500"
                      >
                        Delete
                      </button>
                    </div>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {(showForm || editing) && (
        <LabelGroupFormModal
          initial={editing ?? undefined}
          onClose={() => { setShowForm(false); setEditing(null); }}
          onSaved={() => {
            setShowForm(false);
            setEditing(null);
            qc.invalidateQueries({ queryKey: ['label-groups'] });
          }}
        />
      )}

      <ConfirmDialog
        open={deleteId !== null}
        title="Delete Label Group"
        message="This label group will be permanently deleted."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteId && deleteMut.mutate(deleteId)}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}
