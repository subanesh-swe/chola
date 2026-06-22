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
      <label className="block text-sm text-secondary mb-1">{label}</label>
      <div className="flex flex-wrap gap-1 p-2 bg-input border border-border rounded-lg min-h-[42px]">
        {tags.map((t) => (
          <span
            key={t}
            className="flex items-center gap-1 px-2 py-0.5 bg-surface-2 text-secondary rounded text-xs"
          >
            {t}
            <button
              type="button"
              onClick={() => removeTag(t)}
              className="text-muted hover:text-primary focus:outline-none"
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
          className="flex-1 min-w-[120px] bg-transparent text-primary text-sm outline-none placeholder:text-disabled"
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
      <label className="block text-sm text-secondary mb-1">{label}</label>
      {entries.length > 0 && (
        <div className="mb-2 space-y-1">
          {entries.map(([k, v]) => (
            <div key={k} className="flex items-center gap-2 text-xs">
              <code className="bg-surface-2 px-2 py-1 rounded text-secondary font-mono">{k}</code>
              <span className="text-muted">=</span>
              <code className="bg-surface-2 px-2 py-1 rounded text-muted font-mono flex-1 truncate">
                {v}
              </code>
              <button
                type="button"
                onClick={() => remove(k)}
                className="text-danger hover:text-danger focus:outline-none"
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
          className="flex-1 px-2 py-1.5 text-xs bg-input border border-border rounded text-primary font-mono focus:outline-none focus:ring-1 focus:ring-accent"
          placeholder="KEY"
        />
        <span className="text-muted self-center">=</span>
        <input
          value={val}
          onChange={(e) => setVal(e.target.value)}
          className="flex-1 px-2 py-1.5 text-xs bg-input border border-border rounded text-primary font-mono focus:outline-none focus:ring-1 focus:ring-accent"
          placeholder="value"
          onKeyDown={(e) => e.key === 'Enter' && add()}
        />
        <button
          type="button"
          onClick={add}
          disabled={!key.trim()}
          className="px-2 py-1 text-xs bg-accent-soft text-accent-text border border-accent/30 rounded hover:opacity-80 focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-50"
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
      <div className="bg-surface border border-border rounded-xl p-6 max-w-lg w-full my-4">
        <h3 className="text-lg font-semibold text-primary mb-4">
          {isEdit ? 'Edit Label Group' : 'Create Label Group'}
        </h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm text-secondary mb-1">Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
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
            <label className="block text-sm text-secondary mb-1">Pre-script</label>
            <textarea
              value={preScript}
              onChange={(e) => setPreScript(e.target.value)}
              rows={3}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm font-mono focus:outline-none focus:ring-2 focus:ring-accent resize-none"
              placeholder="#!/bin/bash&#10;echo 'Setup...'"
            />
          </div>

          <div>
            <label className="block text-sm text-secondary mb-1">Max concurrent jobs (0 = unlimited)</label>
            <input
              type="number"
              min="0"
              value={maxConcurrent}
              onChange={(e) => setMaxConcurrent(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            />
          </div>

          <div>
            <label className="block text-sm text-secondary mb-1">Priority (0 = default, higher = preferred)</label>
            <input
              type="number"
              min="0"
              value={priority}
              onChange={(e) => setPriority(e.target.value)}
              className="w-full px-3 py-2 bg-input border border-border rounded-lg text-primary text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            />
          </div>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="w-4 h-4 rounded accent-accent"
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
            onClick={submit}
            disabled={!name.trim() || isPending}
            className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
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
        {/* item 37: dynamic count badge; skeleton while loading */}
        <h2 className="text-2xl font-bold text-primary">
          Label Groups
          {isLoading
            ? <span className="ml-2 inline-block h-5 w-8 bg-surface-2 rounded animate-pulse align-middle" />
            : <span className="ml-2 text-lg text-muted">({groups.length})</span>
          }
        </h2>
        {canManageRepos && (
          <button
            onClick={() => setShowForm(true)}
            className="px-4 py-2 text-sm bg-accent text-on-accent rounded-lg hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Create Group
          </button>
        )}
      </div>

      <p className="text-sm text-muted">
        Label groups match workers by labels and can apply shared environment variables, pre-scripts,
        and concurrency limits.
      </p>

      {isError && (
        <div role="alert" className="bg-danger-soft border border-danger/30 rounded-lg p-4 text-danger">
          Failed to load label groups. Please try again.
        </div>
      )}

      {isLoading ? (
        <PageSkeleton />
      ) : (
        <div className="space-y-3">
          {groups.length === 0 ? (
            <EmptyState
              title="No label groups"
              description="Create a label group to apply shared configuration to matched workers."
            />
          ) : (
            groups.map((g) => (
              <div
                key={g.id}
                className="bg-surface border border-border rounded-xl p-4"
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap mb-2">
                      <h3 className="text-base font-semibold text-primary">{g.name}</h3>
                      <span
                        className={`text-xs px-1.5 py-0.5 rounded border ${
                          g.enabled
                            ? 'bg-success-soft text-success border-success/30'
                            : 'bg-surface-2 text-muted border-border'
                        }`}
                      >
                        {g.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                      {g.priority > 0 && (
                        <span className="text-xs px-1.5 py-0.5 rounded border bg-accent-soft text-accent-text border-accent/30">
                          P:{g.priority}
                        </span>
                      )}
                    </div>

                    {g.match_labels.length > 0 && (
                      <div className="flex flex-wrap gap-1 mb-2">
                        <span className="text-xs text-muted self-center mr-1">Labels:</span>
                        {g.match_labels.map((l) => (
                          <span
                            key={l}
                            className="text-xs px-1.5 py-0.5 bg-accent-soft text-accent-text border border-accent/30 rounded"
                          >
                            {l}
                          </span>
                        ))}
                      </div>
                    )}

                    {g.capabilities.length > 0 && (
                      <div className="flex flex-wrap gap-1 mb-2">
                        <span className="text-xs text-muted self-center mr-1">Capabilities:</span>
                        {g.capabilities.map((c) => (
                          <span
                            key={c}
                            className="text-xs px-1.5 py-0.5 bg-pending-soft text-pending border border-pending/30 rounded"
                          >
                            {c}
                          </span>
                        ))}
                      </div>
                    )}

                    <div className="flex flex-wrap gap-4 text-xs text-muted mt-1">
                      {/* item 38: append " jobs" suffix */}
                      {g.max_concurrent_jobs > 0 && (
                        <span>Max concurrent: {g.max_concurrent_jobs} jobs</span>
                      )}
                      {g.env_vars && Object.keys(g.env_vars).length > 0 && (
                        <span>{Object.keys(g.env_vars).length} env var{Object.keys(g.env_vars).length !== 1 ? 's' : ''}</span>
                      )}
                      {/* item 39: show line count for pre-script */}
                      {g.pre_script && (
                        <span>Pre-script ({g.pre_script.split('\n').filter(Boolean).length} lines)</span>
                      )}
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
                            ? 'bg-surface-2 text-secondary border-border hover:bg-surface-hover focus:ring-border'
                            : 'bg-success-soft text-success border-success/30 hover:bg-success-soft focus:ring-success'
                        }`}
                      >
                        {g.enabled ? 'Disable' : 'Enable'}
                      </button>
                      <button
                        onClick={() => setEditing(g)}
                        className="px-2 py-1 text-xs bg-accent-soft text-accent-text border border-accent/30 rounded hover:opacity-80 focus:outline-none focus:ring-1 focus:ring-accent"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => setDeleteId(g.id)}
                        className="px-2 py-1 text-xs bg-danger-soft text-danger border border-danger/30 rounded hover:bg-danger-soft focus:outline-none focus:ring-1 focus:ring-danger"
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
