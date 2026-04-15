import { useState, useRef } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listWorkers, drainWorker, undrainWorker, deleteWorker, updateWorkerLabels, registerWorker, regenerateWorkerToken, updateWorkerLimits } from '../api/workers';
import type { RegisterWorkerResponse, RegenerateTokenResponse, WorkerLimitsRequest } from '../api/workers';
import {
  listBranchBlacklist,
  createBranchBlacklist,
  deleteBranchBlacklist,
  updateBranchBlacklist,
} from '../api/blacklist';
import { StatusBadge } from '../components/ui/StatusBadge';
import { ResourceBar } from '../components/ui/ResourceBar';
import { TimeAgo } from '../components/ui/TimeAgo';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';
import type { MutationError, BranchBlacklistEntry, DiskDetail, WorkerSystemInfo, WorkerActiveGroup } from '../types';
import { PageSkeleton } from '../components/ui/PageSkeleton';
import { EmptyState } from '../components/ui/EmptyState';

// ── System Info panel ────────────────────────────────────────────────────────

function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  if (days > 0) return `${days}d ${hours}h`;
  const mins = Math.floor((secs % 3600) / 60);
  return hours > 0 ? `${hours}h ${mins}m` : `${mins}m`;
}

function SystemInfoPanel({ info }: { info: WorkerSystemInfo }) {
  const rows: [string, string][] = [];
  if (info.os_name || info.os_version) rows.push(['OS', `${info.os_name ?? ''} ${info.os_version ?? ''}`.trim()]);
  if (info.kernel_version) rows.push(['Kernel', info.kernel_version]);
  if (info.arch) rows.push(['Arch', info.arch]);
  if (info.cpu_brand) rows.push(['CPU', `${info.cpu_brand}${info.cpu_count ? ` (${info.cpu_count} cores)` : ''}`]);
  if (info.uptime != null) rows.push(['Uptime', formatUptime(info.uptime)]);
  if (info.boot_time != null) rows.push(['Boot', new Date(info.boot_time * 1000).toUTCString()]);

  if (!rows.length) return <p className="text-xs text-slate-600 mt-1">No system info available.</p>;

  return (
    <div className="mt-1 grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-xs">
      {rows.map(([label, value]) => (
        <div key={label} className="contents">
          <span className="text-slate-500 font-medium">{label}</span>
          <span className="text-slate-300 font-mono truncate">{value}</span>
        </div>
      ))}
    </div>
  );
}

// ── Per-disk expandable section ──────────────────────────────────────────────

function getBarColor(percent: number): string {
  if (percent >= 90) return 'bg-red-500';
  if (percent >= 70) return 'bg-yellow-500';
  return 'bg-emerald-500';
}

function DiskSection({
  usedDiskMb,
  totalDiskMb,
  allocatedDiskMb,
  diskDetails,
  expanded,
  onToggle,
}: {
  usedDiskMb: number;
  totalDiskMb: number;
  allocatedDiskMb: number;
  diskDetails: DiskDetail[];
  expanded: boolean;
  onToggle: () => void;
}) {
  const totalGb = parseFloat((totalDiskMb / 1024).toFixed(1));
  const reservedGb = parseFloat((allocatedDiskMb / 1024).toFixed(1));
  const usedGb = parseFloat((usedDiskMb / 1024).toFixed(1));
  const reservedPct = totalGb > 0 ? Math.min(Math.max(0, reservedGb / totalGb) * 100, 100) : 0;
  const usedPct = totalGb > 0 ? Math.min(Math.max(0, usedGb / totalGb) * 100, 100) : 0;
  const hasDetails = diskDetails.length > 0;

  return (
    <div className="space-y-1">
      <button
        onClick={hasDetails ? onToggle : undefined}
        className={`w-full text-left ${hasDetails ? 'cursor-pointer' : 'cursor-default'}`}
        type="button"
        aria-expanded={expanded}
      >
        <div className="text-xs text-slate-400 font-medium flex items-center gap-1">
          {hasDetails && (
            <span className="text-[10px]">{expanded ? '\u25BC' : '\u25B6'}</span>
          )}
          Disk ({totalGb} GB)
        </div>
        <div className="mt-1 space-y-1">
          {/* Reserved bar */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-slate-500 w-14 shrink-0">Reserved</span>
            <div className="flex-1 h-2 bg-slate-700 rounded-full overflow-hidden">
              {reservedPct > 0 && (
                <div className="h-full bg-indigo-500 rounded-full transition-all duration-500" style={{ width: `${reservedPct}%` }} />
              )}
            </div>
            <span className="text-[10px] text-slate-400 w-24 text-right shrink-0">{reservedGb} / {totalGb} GB</span>
          </div>
          {/* Usage bar */}
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-slate-500 w-14 shrink-0">Usage</span>
            <div className="flex-1 h-2 bg-slate-700 rounded-full overflow-hidden">
              {usedPct > 0 && (
                <div className={`h-full rounded-full transition-all duration-500 ${getBarColor(usedPct)}`} style={{ width: `${usedPct}%` }} />
              )}
            </div>
            <span className="text-[10px] text-slate-400 w-24 text-right shrink-0">{usedGb} / {totalGb} GB</span>
          </div>
        </div>
      </button>

      {expanded && diskDetails.length > 0 && (
        <div className="ml-4 space-y-1.5 pt-1">
          {diskDetails.map((d) => {
            const pct = d.total_mb > 0 ? Math.min((d.used_mb / d.total_mb) * 100, 100) : 0;
            return (
              <div key={d.mount_point} className="space-y-0.5">
                <div className="flex justify-between text-[11px]">
                  <span className="text-slate-400 font-mono">{d.mount_point}</span>
                  <span className="text-slate-500">
                    {d.used_mb.toLocaleString()} / {d.total_mb.toLocaleString()} MB
                    <span className="ml-1">({pct.toFixed(0)}%)</span>
                    <span className="ml-1.5 text-slate-600">{d.fs_type} {d.device}</span>
                  </span>
                </div>
                <div className="h-1.5 bg-slate-700 rounded-full overflow-hidden">
                  <div
                    className={`h-full rounded-full transition-all duration-500 ${getBarColor(pct)}`}
                    style={{ width: `${pct}%` }}
                  />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Register Worker modal ─────────────────────────────────────────────────────

function RegisterWorkerModal({
  onClose,
  onSuccess,
}: {
  onClose: () => void;
  onSuccess: (result: RegisterWorkerResponse) => void;
}) {
  const [workerId, setWorkerId] = useState('');
  const [hostname, setHostname] = useState('');
  const [description, setDescription] = useState('');
  const [labels, setLabels] = useState<string[]>([]);
  const [labelInput, setLabelInput] = useState('');
  const [regPriority, setRegPriority] = useState('0');
  const [regMaxCpu, setRegMaxCpu] = useState('');
  const [regMaxCpuPct, setRegMaxCpuPct] = useState('');
  const [regMaxMemGb, setRegMaxMemGb] = useState('');
  const [regMaxMemPct, setRegMaxMemPct] = useState('');
  const [regMaxDiskGb, setRegMaxDiskGb] = useState('');
  const [regMaxDiskPct, setRegMaxDiskPct] = useState('');

  const registerMut = useMutation({
    mutationFn: () =>
      registerWorker({
        worker_id: workerId.trim(),
        hostname: hostname.trim(),
        labels: labels.length > 0 ? labels : undefined,
        description: description.trim() || undefined,
        priority: parseInt(regPriority, 10) || 0,
        max_cpu: regMaxCpu !== '' ? parseInt(regMaxCpu, 10) : undefined,
        max_cpu_percent: regMaxCpuPct !== '' ? parseInt(regMaxCpuPct, 10) : undefined,
        max_memory_mb: regMaxMemGb !== '' ? Math.round(parseFloat(regMaxMemGb) * 1024) : undefined,
        max_memory_percent: regMaxMemPct !== '' ? parseInt(regMaxMemPct, 10) : undefined,
        max_disk_mb: regMaxDiskGb !== '' ? Math.round(parseFloat(regMaxDiskGb) * 1024) : undefined,
        max_disk_percent: regMaxDiskPct !== '' ? parseInt(regMaxDiskPct, 10) : undefined,
      }),
    onSuccess: (data) => {
      onSuccess(data);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to register worker'),
  });

  function addLabel() {
    const trimmed = labelInput.trim();
    if (trimmed && !labels.includes(trimmed)) {
      setLabels((prev) => [...prev, trimmed]);
    }
    setLabelInput('');
  }

  function removeLabel(label: string) {
    setLabels((prev) => prev.filter((l) => l !== label));
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
        <h3 className="text-lg font-semibold text-white mb-4">Register Worker</h3>
        <div className="space-y-3">
          <div>
            <label className="block text-sm text-slate-300 mb-1">
              Worker ID <span className="text-red-400">*</span>
            </label>
            <input
              value={workerId}
              onChange={(e) => setWorkerId(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="worker-prod-1"
            />
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">
              Hostname <span className="text-red-400">*</span>
            </label>
            <input
              value={hostname}
              onChange={(e) => setHostname(e.target.value)}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              placeholder="build-server-01.example.com"
            />
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">Labels</label>
            <div className="flex gap-2 mb-1.5">
              <input
                value={labelInput}
                onChange={(e) => setLabelInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    addLabel();
                  }
                }}
                className="flex-1 px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                placeholder="Type and press Enter to add"
              />
              <button
                type="button"
                onClick={addLabel}
                disabled={!labelInput.trim()}
                className="px-3 py-2 text-sm bg-slate-700 text-slate-200 rounded-lg hover:bg-slate-600 disabled:opacity-40 focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Add
              </button>
            </div>
            {labels.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {labels.map((l) => (
                  <span
                    key={l}
                    className="flex items-center gap-1 px-2 py-0.5 bg-blue-600/20 text-blue-300 border border-blue-500/30 rounded text-xs font-mono"
                  >
                    {l}
                    <button
                      type="button"
                      onClick={() => removeLabel(l)}
                      className="text-blue-400 hover:text-red-400 focus:outline-none"
                      aria-label={`Remove label ${l}`}
                    >
                      &times;
                    </button>
                  </span>
                ))}
              </div>
            )}
          </div>
          <div>
            <label className="block text-sm text-slate-300 mb-1">Description</label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
              placeholder="Optional description for this worker"
            />
          </div>
          <div className="pt-2 border-t border-slate-700">
            <p className="text-xs text-slate-500 mb-2 font-medium uppercase tracking-wider">Resource Limits</p>
            <div className="mb-3">
              <label className="block text-xs text-slate-400 mb-1">Priority</label>
              <input
                type="number"
                min="0"
                value={regPriority}
                onChange={(e) => setRegPriority(e.target.value)}
                className="w-28 px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
            </div>
            <div className="space-y-2">
              {/* CPU row */}
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-xs text-slate-400 w-16 shrink-0">Max CPU</span>
                <input
                  type="number"
                  min="1"
                  value={regMaxCpu}
                  onChange={(e) => setRegMaxCpu(e.target.value)}
                  className="w-20 px-2 py-1.5 bg-slate-800 border border-slate-600 rounded text-white text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                  placeholder="no limit"
                />
                <span className="text-xs text-slate-500">cores</span>
                <span className="text-xs text-slate-600">or</span>
                <input
                  type="number"
                  min="1"
                  max="100"
                  value={regMaxCpuPct}
                  onChange={(e) => setRegMaxCpuPct(e.target.value)}
                  className="w-20 px-2 py-1.5 bg-slate-800 border border-slate-600 rounded text-white text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                  placeholder="no limit"
                />
                <span className="text-xs text-slate-500">%</span>
              </div>
              {/* Memory row */}
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-xs text-slate-400 w-16 shrink-0">Max Mem</span>
                <input
                  type="number"
                  min="0"
                  step="0.1"
                  value={regMaxMemGb}
                  onChange={(e) => setRegMaxMemGb(e.target.value)}
                  className="w-20 px-2 py-1.5 bg-slate-800 border border-slate-600 rounded text-white text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                  placeholder="no limit"
                />
                <span className="text-xs text-slate-500">GB</span>
                <span className="text-xs text-slate-600">or</span>
                <input
                  type="number"
                  min="1"
                  max="100"
                  value={regMaxMemPct}
                  onChange={(e) => setRegMaxMemPct(e.target.value)}
                  className="w-20 px-2 py-1.5 bg-slate-800 border border-slate-600 rounded text-white text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                  placeholder="no limit"
                />
                <span className="text-xs text-slate-500">%</span>
              </div>
              {/* Disk row */}
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-xs text-slate-400 w-16 shrink-0">Max Disk</span>
                <input
                  type="number"
                  min="0"
                  step="0.1"
                  value={regMaxDiskGb}
                  onChange={(e) => setRegMaxDiskGb(e.target.value)}
                  className="w-20 px-2 py-1.5 bg-slate-800 border border-slate-600 rounded text-white text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                  placeholder="no limit"
                />
                <span className="text-xs text-slate-500">GB</span>
                <span className="text-xs text-slate-600">or</span>
                <input
                  type="number"
                  min="1"
                  max="100"
                  value={regMaxDiskPct}
                  onChange={(e) => setRegMaxDiskPct(e.target.value)}
                  className="w-20 px-2 py-1.5 bg-slate-800 border border-slate-600 rounded text-white text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                  placeholder="no limit"
                />
                <span className="text-xs text-slate-500">%</span>
              </div>
            </div>
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
            onClick={() => registerMut.mutate()}
            disabled={!workerId.trim() || !hostname.trim() || registerMut.isPending}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            {registerMut.isPending ? 'Registering...' : 'Register'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Token display modal ───────────────────────────────────────────────────────

function TokenDisplayModal({
  result,
  onClose,
}: {
  result: RegisterWorkerResponse;
  onClose: () => void;
}) {
  const tokenRef = useRef<HTMLElement>(null);

  function copyToken() {
    navigator.clipboard.writeText(result.token).then(
      () => toast.success('Token copied to clipboard'),
      () => toast.error('Failed to copy token'),
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full">
        <h3 className="text-lg font-semibold text-white mb-1">Worker Registered</h3>
        <p className="text-sm text-yellow-400 mb-4">
          Save this token now. It will not be shown again.
        </p>
        <div className="relative bg-slate-800 border border-slate-600 rounded-lg p-3 mb-4">
          <code
            ref={tokenRef}
            className="block text-xs font-mono text-emerald-300 break-all pr-16"
          >
            {result.token}
          </code>
          <button
            onClick={copyToken}
            className="absolute top-2 right-2 px-2 py-1 text-xs bg-slate-700 text-slate-300 rounded hover:bg-slate-600 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            Copy
          </button>
        </div>
        <p className="text-xs text-slate-400 mb-1">
          Add to worker config:
        </p>
        <code className="block text-xs font-mono text-slate-300 bg-slate-800 rounded px-3 py-2 mb-2">
          token: {result.token}
        </code>
        <p className="text-xs text-slate-400 mb-1">Or set environment variable:</p>
        <code className="block text-xs font-mono text-slate-300 bg-slate-800 rounded px-3 py-2 mb-4">
          CHOLA_TOKEN={result.token}
        </code>
        <div className="flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Inline Labels Editor ──────────────────────────────────────────────────────

function InlineLabelsEditor({
  workerId,
  currentLabels,
  onSave,
  onCancel,
  isPending,
}: {
  workerId: string;
  currentLabels: string[];
  onSave: (labels: string[]) => void;
  onCancel: () => void;
  isPending: boolean;
}) {
  const [labels, setLabels] = useState<string[]>(currentLabels);
  const [input, setInput] = useState('');

  function addLabel() {
    const t = input.trim();
    if (t && !labels.includes(t)) setLabels((prev) => [...prev, t]);
    setInput('');
  }

  function removeLabel(label: string) {
    setLabels((prev) => prev.filter((l) => l !== label));
  }

  return (
    <div className="mt-2 space-y-2">
      <div className="flex flex-wrap gap-1.5">
        {labels.map((l) => (
          <span
            key={l}
            className="flex items-center gap-1 px-2 py-0.5 bg-blue-600/20 text-blue-300 border border-blue-500/30 rounded text-xs font-mono"
          >
            {l}
            <button
              type="button"
              onClick={() => removeLabel(l)}
              className="text-blue-400 hover:text-red-400 focus:outline-none"
              aria-label={`Remove label ${l}`}
            >
              &times;
            </button>
          </span>
        ))}
      </div>
      <div className="flex gap-2">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') { e.preventDefault(); addLabel(); }
          }}
          placeholder="Type label and press Enter"
          className="flex-1 px-2 py-1 text-xs bg-slate-800 border border-slate-600 rounded text-white font-mono focus:outline-none focus:ring-1 focus:ring-blue-500"
          aria-label={`Add label for worker ${workerId}`}
        />
        <button
          type="button"
          onClick={addLabel}
          disabled={!input.trim()}
          className="px-2 py-1 text-xs bg-slate-700 text-slate-300 rounded hover:bg-slate-600 disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-blue-500"
        >
          Add
        </button>
      </div>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => onSave(labels)}
          disabled={isPending}
          className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-blue-500"
        >
          {isPending ? 'Saving...' : 'Save'}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-1 text-xs bg-slate-700 text-slate-300 rounded hover:bg-slate-600 focus:outline-none focus:ring-1 focus:ring-slate-500"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

// ── Inline Limits Editor ──────────────────────────────────────────────────────

function InlineLimitsEditor({
  workerId,
  initial,
  onSave,
  onCancel,
  isPending,
}: {
  workerId: string;
  initial: WorkerLimitsRequest;
  onSave: (limits: WorkerLimitsRequest) => void;
  onCancel: () => void;
  isPending: boolean;
}) {
  const [priority, setPriority] = useState(String(initial.priority ?? 0));
  const [maxCpu, setMaxCpu] = useState(initial.max_cpu != null ? String(initial.max_cpu) : '');
  const [maxCpuPct, setMaxCpuPct] = useState(initial.max_cpu_percent != null ? String(initial.max_cpu_percent) : '');
  const [maxMemGb, setMaxMemGb] = useState(
    initial.max_memory_mb != null ? String(parseFloat((initial.max_memory_mb / 1024).toFixed(1))) : '',
  );
  const [maxMemPct, setMaxMemPct] = useState(initial.max_memory_percent != null ? String(initial.max_memory_percent) : '');
  const [maxDiskGb, setMaxDiskGb] = useState(
    initial.max_disk_mb != null ? String(parseFloat((initial.max_disk_mb / 1024).toFixed(1))) : '',
  );
  const [maxDiskPct, setMaxDiskPct] = useState(initial.max_disk_percent != null ? String(initial.max_disk_percent) : '');

  function save() {
    onSave({
      priority: parseInt(priority, 10) || 0,
      max_cpu: maxCpu !== '' ? parseInt(maxCpu, 10) : null,
      max_cpu_percent: maxCpuPct !== '' ? parseInt(maxCpuPct, 10) : null,
      max_memory_mb: maxMemGb !== '' ? Math.round(parseFloat(maxMemGb) * 1024) : null,
      max_memory_percent: maxMemPct !== '' ? parseInt(maxMemPct, 10) : null,
      max_disk_mb: maxDiskGb !== '' ? Math.round(parseFloat(maxDiskGb) * 1024) : null,
      max_disk_percent: maxDiskPct !== '' ? parseInt(maxDiskPct, 10) : null,
    });
  }

  const inputCls = 'px-2 py-1 text-xs bg-slate-800 border border-slate-600 rounded text-white font-mono w-20 focus:outline-none focus:ring-1 focus:ring-blue-500';

  return (
    <div className="mt-2 space-y-2" aria-label={`Edit limits for worker ${workerId}`}>
      <div className="mb-1.5">
        <label className="block text-[10px] text-slate-500 mb-0.5">Priority</label>
        <input type="number" min="0" value={priority} onChange={(e) => setPriority(e.target.value)} className={inputCls} />
      </div>
      <div className="space-y-1.5">
        {/* CPU row */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-[10px] text-slate-500 w-12 shrink-0">Max CPU</span>
          <input type="number" min="1" value={maxCpu} onChange={(e) => setMaxCpu(e.target.value)} className={inputCls} placeholder="no limit" />
          <span className="text-[10px] text-slate-500">cores</span>
          <span className="text-[10px] text-slate-600">or</span>
          <input type="number" min="1" max="100" value={maxCpuPct} onChange={(e) => setMaxCpuPct(e.target.value)} className={inputCls} placeholder="no limit" />
          <span className="text-[10px] text-slate-500">%</span>
        </div>
        {/* Memory row */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-[10px] text-slate-500 w-12 shrink-0">Max Mem</span>
          <input type="number" min="0" step="0.1" value={maxMemGb} onChange={(e) => setMaxMemGb(e.target.value)} className={inputCls} placeholder="no limit" />
          <span className="text-[10px] text-slate-500">GB</span>
          <span className="text-[10px] text-slate-600">or</span>
          <input type="number" min="1" max="100" value={maxMemPct} onChange={(e) => setMaxMemPct(e.target.value)} className={inputCls} placeholder="no limit" />
          <span className="text-[10px] text-slate-500">%</span>
        </div>
        {/* Disk row */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-[10px] text-slate-500 w-12 shrink-0">Max Disk</span>
          <input type="number" min="0" step="0.1" value={maxDiskGb} onChange={(e) => setMaxDiskGb(e.target.value)} className={inputCls} placeholder="no limit" />
          <span className="text-[10px] text-slate-500">GB</span>
          <span className="text-[10px] text-slate-600">or</span>
          <input type="number" min="1" max="100" value={maxDiskPct} onChange={(e) => setMaxDiskPct(e.target.value)} className={inputCls} placeholder="no limit" />
          <span className="text-[10px] text-slate-500">%</span>
        </div>
      </div>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={save}
          disabled={isPending}
          className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50 focus:outline-none focus:ring-1 focus:ring-blue-500"
        >
          {isPending ? 'Saving...' : 'Save'}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-1 text-xs bg-slate-700 text-slate-300 rounded hover:bg-slate-600 focus:outline-none focus:ring-1 focus:ring-slate-500"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

// ── Worker Branch Blacklist panel ─────────────────────────────────────────────

function WorkerBranchBlacklist({
  workerId,
  canManage,
}: {
  workerId: string;
  canManage: boolean;
}) {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [pattern, setPattern] = useState('');
  const [description, setDescription] = useState('');
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ['blacklist-branches', workerId],
    queryFn: () => listBranchBlacklist(workerId),
  });

  const createMut = useMutation({
    mutationFn: () =>
      createBranchBlacklist({ worker_id: workerId, pattern, description: description || undefined }),
    onSuccess: () => {
      toast.success('Branch rule created');
      qc.invalidateQueries({ queryKey: ['blacklist-branches', workerId] });
      setShowAdd(false);
      setPattern('');
      setDescription('');
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to create rule'),
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      updateBranchBlacklist(id, { enabled }),
    onSuccess: () => {
      toast.success('Rule updated');
      qc.invalidateQueries({ queryKey: ['blacklist-branches', workerId] });
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update rule'),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteBranchBlacklist(id),
    onSuccess: () => {
      toast.success('Rule deleted');
      qc.invalidateQueries({ queryKey: ['blacklist-branches', workerId] });
      setDeleteId(null);
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to delete rule'),
  });

  const entries: BranchBlacklistEntry[] = data?.entries ?? [];

  return (
    <div className="mt-3 pt-3 border-t border-slate-800">
      <div className="flex items-center justify-between mb-2">
        <p className="text-xs font-semibold text-slate-500 uppercase tracking-wider">
          Branch Blacklist ({entries.length})
        </p>
        {canManage && (
          <button
            onClick={() => setShowAdd(true)}
            className="text-xs px-2 py-1 bg-blue-600/20 text-blue-400 border border-blue-500/30 rounded hover:bg-blue-600/30 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            Add Rule
          </button>
        )}
      </div>

      {isLoading && <p className="text-xs text-slate-500">Loading...</p>}

      {entries.length > 0 && (
        <div className="space-y-1">
          {entries.map((e) => (
            <div key={e.id} className="flex items-center gap-2 text-xs">
              <code className="text-slate-300 bg-slate-800 px-1.5 py-0.5 rounded font-mono flex-1 truncate">
                {e.pattern}
              </code>
              {e.description && (
                <span className="text-slate-500 truncate max-w-[120px]">{e.description}</span>
              )}
              {canManage ? (
                <button
                  onClick={() => toggleMut.mutate({ id: e.id, enabled: !e.enabled })}
                  className={`px-1.5 py-0.5 rounded border shrink-0 focus:outline-none focus:ring-1 ${
                    e.enabled
                      ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/20 focus:ring-emerald-500'
                      : 'bg-slate-700 text-slate-400 border-slate-600 hover:bg-slate-600 focus:ring-slate-500'
                  }`}
                >
                  {e.enabled ? 'On' : 'Off'}
                </button>
              ) : (
                <span
                  className={`px-1.5 py-0.5 rounded border shrink-0 ${
                    e.enabled
                      ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                      : 'bg-slate-700 text-slate-400 border-slate-600'
                  }`}
                >
                  {e.enabled ? 'On' : 'Off'}
                </span>
              )}
              {canManage && (
                <button
                  onClick={() => setDeleteId(e.id)}
                  className="text-red-400 hover:text-red-300 shrink-0 focus:outline-none focus:ring-1 focus:ring-red-500 rounded"
                  aria-label={`Delete rule for pattern ${e.pattern}`}
                >
                  &times;
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {!entries.length && !isLoading && (
        <p className="text-xs text-slate-600">No branch restrictions.</p>
      )}

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full">
            <h3 className="text-lg font-semibold text-white mb-4">
              Add Branch Rule — {workerId}
            </h3>
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-slate-300 mb-1">Pattern (regex)</label>
                <input
                  value={pattern}
                  onChange={(e) => setPattern(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  placeholder="^release/.*"
                />
              </div>
              <div>
                <label className="block text-sm text-slate-300 mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  rows={2}
                  className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
                  placeholder="Why this branch pattern is blocked..."
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
        title="Delete Branch Rule"
        message="This branch blacklist rule will be permanently removed."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteId && deleteMut.mutate(deleteId)}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function WorkersPage() {
  const qc = useQueryClient();
  const { canManageWorkers } = usePermission();
  const [expandedBlacklist, setExpandedBlacklist] = useState<string | null>(null);
  const [expandedDisks, setExpandedDisks] = useState<Set<string>>(new Set());
  const [expandedSysInfo, setExpandedSysInfo] = useState<Set<string>>(new Set());
  const [showRegisterModal, setShowRegisterModal] = useState(false);
  const [tokenResult, setTokenResult] = useState<RegisterWorkerResponse | RegenerateTokenResponse | null>(null);
  const [regenConfirmId, setRegenConfirmId] = useState<string | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [editingLabelsId, setEditingLabelsId] = useState<string | null>(null);
  const [labelDraft, setLabelDraft] = useState<string[]>([]);
  const [editingLimitsId, setEditingLimitsId] = useState<string | null>(null);
  const { data, isLoading, isError } = useQuery({ queryKey: ['workers'], queryFn: () => listWorkers(), refetchInterval: 5000 });

  const drainMut = useMutation({
    mutationFn: (id: string) => drainWorker(id),
    onSuccess: () => { toast.success('Worker set to drain'); qc.invalidateQueries({ queryKey: ['workers'] }); },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to drain worker'),
  });
  const undrainMut = useMutation({
    mutationFn: (id: string) => undrainWorker(id),
    onSuccess: () => { toast.success('Worker undrained'); qc.invalidateQueries({ queryKey: ['workers'] }); },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to undrain worker'),
  });
  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteWorker(id),
    onSuccess: (_, id) => {
      toast.success('Worker deleted');
      setDeleteConfirmId(null);
      qc.invalidateQueries({ queryKey: ['workers'] });
      if (editingLabelsId === id) setEditingLabelsId(null);
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to delete worker'),
  });
  const updateLabelsMut = useMutation({
    mutationFn: ({ id, labels }: { id: string; labels: string[] }) => updateWorkerLabels(id, labels),
    onSuccess: () => {
      toast.success('Labels updated');
      setEditingLabelsId(null);
      qc.invalidateQueries({ queryKey: ['workers'] });
    },
    onError: (err: unknown) => toast.error((err as MutationError).userMessage || 'Failed to update labels'),
  });
  const regenerateTokenMut = useMutation({
    mutationFn: (id: string) => regenerateWorkerToken(id),
    onSuccess: (data) => {
      setRegenConfirmId(null);
      setTokenResult(data);
      qc.invalidateQueries({ queryKey: ['workers'] });
      toast.success(`Token regenerated for ${data.worker_id}`);
    },
    onError: (err: unknown) => {
      setRegenConfirmId(null);
      toast.error((err as MutationError).userMessage || 'Failed to regenerate token');
    },
  });
  const updateLimitsMut = useMutation({
    mutationFn: ({ id, limits }: { id: string; limits: WorkerLimitsRequest }) =>
      updateWorkerLimits(id, limits),
    onSuccess: () => {
      toast.success('Worker limits updated');
      setEditingLimitsId(null);
      qc.invalidateQueries({ queryKey: ['workers'] });
    },
    onError: (err: unknown) =>
      toast.error((err as MutationError).userMessage || 'Failed to update limits'),
  });

  const workers = [...(data?.data ?? [])].sort((a, b) => a.worker_id.localeCompare(b.worker_id));

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">Workers ({workers.length})</h2>
        {canManageWorkers && (
          <button
            onClick={() => setShowRegisterModal(true)}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            Register Worker
          </button>
        )}
      </div>

      {isError && (
        <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400">
          Failed to load workers. Please try again.
        </div>
      )}
      {isLoading ? <PageSkeleton /> : (
        <div className="grid gap-4">
          {workers.map(w => (
            <div key={w.worker_id} className="bg-slate-900 border border-slate-700 rounded-xl p-4">
              <div className="flex items-center justify-between mb-4 flex-wrap gap-2">
                <div className="flex items-center gap-3 min-w-0">
                  <StatusBadge status={w.status} size="md" />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="text-lg font-semibold text-white truncate">{w.worker_id}</p>
                      {w.priority > 0 && (
                        <span className="text-[11px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-400 border-blue-500/30 shrink-0">
                          P:{w.priority}
                        </span>
                      )}
                    </div>
                    <p className="text-sm text-slate-400">{w.hostname} &middot; {w.disk_type} &middot; Docker: {w.docker_enabled ? 'Yes' : 'No'}</p>
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0 flex-wrap justify-end">
                  {w.last_heartbeat && <span className="text-xs text-slate-500">Jobs: {w.last_heartbeat.running_jobs}</span>}
                  {w.status === 'Draining' && (
                    <span className="text-xs text-yellow-400">
                      {(w.active_groups?.length ?? 0) > 0
                        ? `${w.active_groups!.length} reservation${w.active_groups!.length !== 1 ? 's' : ''} remaining`
                        : 'Idle — ready for maintenance'}
                    </span>
                  )}
                  {canManageWorkers && w.status === 'Connected' && (
                    <button
                      onClick={() => drainMut.mutate(w.worker_id)}
                      aria-label={`Drain worker ${w.worker_id}`}
                      className="px-3 py-1 text-xs bg-yellow-500/20 text-yellow-400 border border-yellow-500/30 rounded-lg hover:bg-yellow-500/30 focus:outline-none focus:ring-2 focus:ring-yellow-500"
                    >
                      Drain
                    </button>
                  )}
                  {canManageWorkers && w.status === 'Draining' && (
                    <button
                      onClick={() => undrainMut.mutate(w.worker_id)}
                      aria-label={`Undrain worker ${w.worker_id}`}
                      className="px-3 py-1 text-xs bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 rounded-lg hover:bg-emerald-500/30 focus:outline-none focus:ring-2 focus:ring-emerald-500"
                    >
                      Undrain
                    </button>
                  )}
                  {canManageWorkers && (
                    <button
                      onClick={() => setRegenConfirmId(w.worker_id)}
                      aria-label={`Regenerate token for worker ${w.worker_id}`}
                      className="px-3 py-1 text-xs bg-amber-500/20 text-amber-400 border border-amber-500/30 rounded-lg hover:bg-amber-500/30 focus:outline-none focus:ring-2 focus:ring-amber-500"
                    >
                      Regen Token
                    </button>
                  )}
                  {canManageWorkers && (
                    <button
                      onClick={() => setDeleteConfirmId(w.worker_id)}
                      aria-label={`Delete worker ${w.worker_id}`}
                      className="px-3 py-1 text-xs bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg hover:bg-red-500/30 focus:outline-none focus:ring-2 focus:ring-red-500"
                    >
                      Delete
                    </button>
                  )}
                </div>
              </div>
              {(() => {
                const isOffline = w.status === 'Disconnected';
                const hasLastKnown = isOffline && w.last_heartbeat != null;
                return (
                  <div className="space-y-1">
                    {hasLastKnown && (
                      <p className="text-[11px] text-slate-500 italic mb-1">
                        Last known values (worker offline)
                      </p>
                    )}
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                      <ResourceBar
                        label={hasLastKnown ? 'CPU (last known)' : 'CPU'}
                        total={w.max_cpu ?? w.total_cpu}
                        reserved={w.allocated_cpu}
                        used={w.last_heartbeat?.used_cpu_percent ?? 0}
                        unit="cores"
                        usedIsPercent
                        hardwareTotal={w.max_cpu != null ? w.total_cpu : undefined}
                      />
                      <ResourceBar
                        label={hasLastKnown ? 'Memory (last known)' : 'Memory'}
                        total={parseFloat(((w.max_memory_mb ?? w.total_memory_mb) / 1024).toFixed(1))}
                        reserved={parseFloat((w.allocated_memory_mb / 1024).toFixed(1))}
                        used={parseFloat(((w.last_heartbeat?.used_memory_mb ?? 0) / 1024).toFixed(1))}
                        unit="GB"
                        hardwareTotal={w.max_memory_mb != null ? parseFloat((w.total_memory_mb / 1024).toFixed(1)) : undefined}
                      />
                      <DiskSection
                        usedDiskMb={w.last_heartbeat?.used_disk_mb ?? 0}
                        totalDiskMb={w.max_disk_mb ?? w.total_disk_mb}
                        allocatedDiskMb={w.allocated_disk_mb}
                        diskDetails={w.last_heartbeat?.disk_details ?? w.disk_details ?? []}
                        expanded={expandedDisks.has(w.worker_id)}
                        onToggle={() => setExpandedDisks(prev => {
                          const next = new Set(prev);
                          if (next.has(w.worker_id)) next.delete(w.worker_id);
                          else next.add(w.worker_id);
                          return next;
                        })}
                      />
                    </div>
                  </div>
                );
              })()}
              {(w.allocated_cpu > 0 || w.allocated_memory_mb > 0 || w.allocated_disk_mb > 0) && (
                <div className="mt-3 pt-3 border-t border-slate-800">
                  <div className="flex items-center justify-between mb-2">
                    <p className="text-xs text-slate-500">
                      <span className="text-indigo-400 font-medium">Active reservations</span>
                      {' — '}
                      {w.allocated_cpu > 0 && (
                        <span className="mr-2">{w.allocated_cpu} CPU</span>
                      )}
                      {w.allocated_memory_mb > 0 && (
                        <span className="mr-2">{w.allocated_memory_mb.toLocaleString()} MB RAM</span>
                      )}
                      {w.allocated_disk_mb > 0 && (
                        <span>{w.allocated_disk_mb.toLocaleString()} MB disk</span>
                      )}
                    </p>
                    <a
                      href={`/builds?worker=${encodeURIComponent(w.worker_id)}&state=running,reserved`}
                      className="text-xs text-indigo-400 hover:text-indigo-300 underline shrink-0 ml-4"
                    >
                      View active builds &rarr;
                    </a>
                  </div>
                  {w.active_groups && w.active_groups.length > 0 && (
                    <div className="space-y-1.5">
                      {w.active_groups.map((g: WorkerActiveGroup) => (
                        <a
                          key={g.group_id}
                          href={`/builds/${g.group_id}`}
                          className="flex items-center justify-between px-3 py-2 bg-slate-800/50 border border-slate-700/50 rounded-lg hover:bg-slate-800 transition-colors"
                        >
                          <div className="flex items-center gap-2">
                            <StatusBadge status={g.state} />
                            <span className="text-xs text-slate-300 font-mono">{g.group_id.slice(0, 8)}</span>
                            {g.branch && <span className="text-xs text-slate-500">{g.branch}</span>}
                            {g.commit_sha && <span className="text-xs text-slate-600 font-mono">{g.commit_sha.slice(0, 8)}</span>}
                          </div>
                          <div className="flex items-center gap-3 text-xs text-slate-500">
                            <span>{g.allocated_cpu} CPU, {g.allocated_memory_mb}MB</span>
                            <span>{g.stages_submitted} stage{g.stages_submitted !== 1 ? 's' : ''}</span>
                            <TimeAgo date={g.created_at} />
                          </div>
                        </a>
                      ))}
                    </div>
                  )}
                </div>
              )}
              {/* Labels row with inline editing */}
              <div className="mt-3 pt-3 border-t border-slate-800">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-xs text-slate-500 font-medium">Labels:</span>
                  {(w.labels ?? []).length > 0
                    ? (w.labels ?? []).map((l) => (
                        <span
                          key={l}
                          className="px-2 py-0.5 bg-blue-600/20 text-blue-300 border border-blue-500/30 rounded text-xs font-mono"
                        >
                          {l}
                        </span>
                      ))
                    : <span className="text-xs text-slate-600 italic">none</span>}
                  {canManageWorkers && editingLabelsId !== w.worker_id && (
                    <button
                      type="button"
                      onClick={() => {
                        setEditingLabelsId(w.worker_id);
                        setLabelDraft(w.labels ?? []);
                      }}
                      className="text-xs text-slate-500 hover:text-slate-300 underline focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                    >
                      Edit Labels
                    </button>
                  )}
                </div>
                {editingLabelsId === w.worker_id && (
                  <InlineLabelsEditor
                    workerId={w.worker_id}
                    currentLabels={labelDraft}
                    onSave={(labels) => updateLabelsMut.mutate({ id: w.worker_id, labels })}
                    onCancel={() => setEditingLabelsId(null)}
                    isPending={updateLabelsMut.isPending}
                  />
                )}
              </div>
              {/* Resource limits inline edit */}
              {canManageWorkers && (
                <div className="mt-3 pt-3 border-t border-slate-800">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-xs text-slate-500 font-medium">Limits:</span>
                    {(() => {
                      const cpuParts = [
                        w.max_cpu != null ? `${w.max_cpu} cores` : null,
                        w.max_cpu_percent != null ? `${w.max_cpu_percent}%` : null,
                      ].filter(Boolean);
                      const memParts = [
                        w.max_memory_mb != null ? `${parseFloat((w.max_memory_mb / 1024).toFixed(1))} GB` : null,
                        w.max_memory_percent != null ? `${w.max_memory_percent}%` : null,
                      ].filter(Boolean);
                      const diskParts = [
                        w.max_disk_mb != null ? `${parseFloat((w.max_disk_mb / 1024).toFixed(1))} GB` : null,
                        w.max_disk_percent != null ? `${w.max_disk_percent}%` : null,
                      ].filter(Boolean);
                      const hasAny = cpuParts.length || memParts.length || diskParts.length;
                      return hasAny ? (
                        <>
                          <span className="text-xs text-slate-400">CPU: {cpuParts.length ? cpuParts.join(' / ') : 'none'}</span>
                          <span className="text-xs text-slate-400">Mem: {memParts.length ? memParts.join(' / ') : 'none'}</span>
                          <span className="text-xs text-slate-400">Disk: {diskParts.length ? diskParts.join(' / ') : 'none'}</span>
                        </>
                      ) : (
                        <span className="text-xs text-slate-600 italic">none</span>
                      );
                    })()}
                    {editingLimitsId !== w.worker_id && (
                      <button
                        type="button"
                        onClick={() => setEditingLimitsId(w.worker_id)}
                        className="text-xs text-slate-500 hover:text-slate-300 underline focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                      >
                        Edit Limits
                      </button>
                    )}
                  </div>
                  {editingLimitsId === w.worker_id && (
                    <InlineLimitsEditor
                      workerId={w.worker_id}
                      initial={{ priority: w.priority, max_cpu: w.max_cpu, max_cpu_percent: w.max_cpu_percent, max_memory_mb: w.max_memory_mb, max_memory_percent: w.max_memory_percent, max_disk_mb: w.max_disk_mb, max_disk_percent: w.max_disk_percent }}
                      onSave={(limits) => updateLimitsMut.mutate({ id: w.worker_id, limits })}
                      onCancel={() => setEditingLimitsId(null)}
                      isPending={updateLimitsMut.isPending}
                    />
                  )}
                </div>
              )}
              <div className="mt-3 flex flex-wrap gap-4 text-xs text-slate-500">
                <span>Types: {w.supported_job_types.join(', ')}</span>
                <span>Registered: <TimeAgo date={w.registered_at} /></span>
                {w.last_heartbeat && <span>Last beat: <TimeAgo date={w.last_heartbeat.timestamp} /></span>}
                {w.registration_token_id && (
                  <span>Token: <span className="font-mono text-slate-600">{w.registration_token_id.slice(0, 8)}</span></span>
                )}
                {w.system_info && (
                  <button
                    onClick={() => setExpandedSysInfo(prev => {
                      const next = new Set(prev);
                      if (next.has(w.worker_id)) next.delete(w.worker_id);
                      else next.add(w.worker_id);
                      return next;
                    })}
                    className="text-slate-500 hover:text-slate-300 underline focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                    aria-expanded={expandedSysInfo.has(w.worker_id)}
                  >
                    {expandedSysInfo.has(w.worker_id) ? 'Hide System Info' : 'System Info'}
                  </button>
                )}
                <button
                  onClick={() => setExpandedBlacklist(expandedBlacklist === w.worker_id ? null : w.worker_id)}
                  className="text-slate-500 hover:text-slate-300 underline focus:outline-none focus:ring-1 focus:ring-blue-500 rounded"
                  aria-expanded={expandedBlacklist === w.worker_id}
                >
                  {expandedBlacklist === w.worker_id ? 'Hide' : 'Branch Blacklist'}
                </button>
              </div>
              {expandedSysInfo.has(w.worker_id) && w.system_info && (
                <div className="mt-3 pt-3 border-t border-slate-800">
                  <p className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1">System Info</p>
                  <SystemInfoPanel info={w.system_info} />
                </div>
              )}
              {expandedBlacklist === w.worker_id && (
                <WorkerBranchBlacklist workerId={w.worker_id} canManage={canManageWorkers} />
              )}
            </div>
          ))}
          {!workers.length && <EmptyState message="No workers registered" description="Use Register Worker to add a new worker." />}
        </div>
      )}

      {showRegisterModal && (
        <RegisterWorkerModal
          onClose={() => setShowRegisterModal(false)}
          onSuccess={(result) => {
            setShowRegisterModal(false);
            setTokenResult(result);
            qc.invalidateQueries({ queryKey: ['workers'] });
            toast.success(`Worker ${result.worker_id} registered`);
          }}
        />
      )}

      {tokenResult && (
        <TokenDisplayModal
          result={tokenResult}
          onClose={() => setTokenResult(null)}
        />
      )}

      <ConfirmDialog
        open={regenConfirmId !== null}
        title="Regenerate Worker Token"
        message="This will deactivate the current token and disconnect the worker. A new token will be generated. The worker must reconnect with the new token."
        confirmLabel="Regenerate"
        variant="warning"
        onConfirm={() => regenConfirmId && regenerateTokenMut.mutate(regenConfirmId)}
        onCancel={() => setRegenConfirmId(null)}
      />

      <ConfirmDialog
        open={deleteConfirmId !== null}
        title="Delete Worker"
        message="This will remove the worker and deactivate its token. The worker will be disconnected."
        confirmLabel="Delete"
        variant="danger"
        onConfirm={() => deleteConfirmId && deleteMut.mutate(deleteConfirmId)}
        onCancel={() => setDeleteConfirmId(null)}
      />
    </div>
  );
}
