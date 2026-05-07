import { useState, useEffect, useRef } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { getSettings, updateSetting, SettingItem } from '../api/settings';
import { LoadingSkeleton } from '../components/ui';
import { usePermission } from '../hooks/usePermission';

// ─── Icons (no extra dep) ─────────────────────────────────────────────────────

function CheckCircleIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  );
}

function XCircleIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  );
}

// ─── Types ────────────────────────────────────────────────────────────────────

type Mode = 'view' | 'edit' | 'review' | 'result';

interface SubmitResult {
  key: string;
  status: 'accepted' | 'rejected';
  message?: string;
}

interface MutationError {
  userMessage?: string;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const CATEGORY_ORDER = ['Scheduling', 'Workers', 'Logging', 'Retention', 'Execution', 'Server / Auth'];

const RETENTION_TIER_KEYS = new Set([
  'retention.t1_purge_files_after_days',
  'retention.t2_archive_after_days',
  'retention.t3_delete_archive_after_days',
]);

// ─── Helpers ─────────────────────────────────────────────────────────────────

function categoryOf(key: string): string {
  if (key.startsWith('scheduling.')) return 'Scheduling';
  if (key.startsWith('workers.')) return 'Workers';
  if (key.startsWith('logging.')) return 'Logging';
  if (key.startsWith('retention.')) return 'Retention';
  if (key.startsWith('execution.')) return 'Execution';
  return 'Server / Auth';
}

function labelOf(key: string): string {
  const parts = key.split('.');
  return parts[parts.length - 1].replace(/_/g, ' ');
}

// ─── Sub-components ──────────────────────────────────────────────────────────

function SourceBadge({ source }: { source: string }) {
  const styles: Record<string, string> = {
    database: 'bg-accent-soft text-accent-text border-accent/30',
    config: 'bg-surface-2 text-muted border-border',
    default: 'bg-surface-2/50 text-muted border-border',
  };
  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium border ${styles[source] ?? styles.default}`}
    >
      {source}
    </span>
  );
}

function BoolDisplay({ value }: { value: string | number | boolean }) {
  const on = String(value) === 'true';
  return (
    <span
      className={`px-2 py-0.5 rounded text-xs font-medium ${on ? 'bg-emerald-500/20 text-emerald-400' : 'bg-surface-2 text-muted'}`}
    >
      {on ? 'enabled' : 'disabled'}
    </span>
  );
}

function ValueDisplay({ setting }: { setting: SettingItem }) {
  if (setting.type === 'bool') return <BoolDisplay value={setting.value} />;
  return <span className="text-sm text-primary font-mono">{String(setting.value)}</span>;
}

// Inline hints for specific retention keys shown in both view and edit modes.
function RetentionHint({ settingKey }: { settingKey: string }) {
  if (RETENTION_TIER_KEYS.has(settingKey)) {
    return (
      <span className="block text-[11px] text-amber-500/70 mt-0.5">
        Values must satisfy: T1 &lt; T2 &lt; T3
      </span>
    );
  }
  if (settingKey === 'retention.enable_worker_fanout') {
    return (
      <span className="block text-[11px] text-yellow-500/70 mt-0.5 max-w-xs">
        Only enable this once all workers have been upgraded to a version that handles purge
        directives. Leave OFF during rolling upgrades.
      </span>
    );
  }
  return null;
}

// ─── View mode rows ───────────────────────────────────────────────────────────

function ViewRow({ setting }: { setting: SettingItem }) {
  return (
    <div className="py-2 border-b border-border last:border-0">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-sm text-secondary capitalize">{labelOf(setting.key)}</span>
          <SourceBadge source={setting.source} />
          {!setting.editable && (
            <span className="text-disabled" title="Read-only — requires restart">
              &#128274;
            </span>
          )}
        </div>
        <div className="flex-shrink-0">
          <ValueDisplay setting={setting} />
        </div>
      </div>
      {setting.description && (
        <p className="text-[11px] text-disabled mt-0.5">{setting.description}</p>
      )}
      <RetentionHint settingKey={setting.key} />
    </div>
  );
}

// ─── Edit mode field ──────────────────────────────────────────────────────────

interface EditFieldProps {
  setting: SettingItem;
  draftValue: string;
  onChange: (key: string, value: string) => void;
  changed: boolean;
}

function EditField({ setting, draftValue, onChange, changed }: EditFieldProps) {
  const ringClass = changed ? 'ring-2 ring-accent/30 border-accent/50' : 'border-border';

  if (setting.options) {
    return (
      <select
        value={draftValue}
        onChange={(e) => onChange(setting.key, e.target.value)}
        className={`bg-input border rounded px-2 py-1 text-sm text-primary focus:outline-none focus:ring-2 focus:ring-accent ${ringClass}`}
      >
        {setting.options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    );
  }

  if (setting.type === 'bool') {
    return (
      <select
        value={draftValue}
        onChange={(e) => onChange(setting.key, e.target.value)}
        className={`bg-input border rounded px-2 py-1 text-sm text-primary focus:outline-none focus:ring-2 focus:ring-accent ${ringClass}`}
      >
        <option value="true">enabled</option>
        <option value="false">disabled</option>
      </select>
    );
  }

  if (setting.type === 'path') {
    return (
      <input
        type="text"
        value={draftValue}
        onChange={(e) => onChange(setting.key, e.target.value)}
        className={`bg-slate-800 border rounded px-2 py-1 text-sm text-white w-64 font-mono ${ringClass}`}
        placeholder="/absolute/path"
      />
    );
  }

  return (
    <input
      type="number"
      value={draftValue}
      onChange={(e) => onChange(setting.key, e.target.value)}
      min={setting.min}
      max={setting.max}
      className={`bg-input border rounded px-2 py-1 text-sm text-primary w-28 focus:outline-none focus:ring-2 focus:ring-accent ${ringClass}`}
    />
  );
}

interface EditRowProps {
  setting: SettingItem;
  draftValue: string;
  onChange: (key: string, value: string) => void;
  changed: boolean;
}

function EditRow({ setting, draftValue, onChange, changed }: EditRowProps) {
  if (!setting.editable) {
    return (
      <div className="py-2 border-b border-border last:border-0 opacity-60">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-3 min-w-0">
            <span className="text-sm text-muted capitalize">{labelOf(setting.key)}</span>
            <SourceBadge source={setting.source} />
            <span className="text-disabled" title="Read-only — requires restart">
              &#128274;
            </span>
          </div>
          <div className="flex-shrink-0">
            <ValueDisplay setting={setting} />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="py-2 border-b border-border last:border-0">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0 pt-0.5">
          <span className="text-sm text-secondary capitalize">{labelOf(setting.key)}</span>
          <SourceBadge source={setting.source} />
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          <EditField
            setting={setting}
            draftValue={draftValue}
            onChange={onChange}
            changed={changed}
          />
        </div>
      </div>
      {setting.description && (
        <p className="text-[11px] text-slate-500 mt-0.5">{setting.description}</p>
      )}
      <RetentionHint settingKey={setting.key} />
    </div>
  );
}

// ─── Section ──────────────────────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-surface border border-border rounded-xl p-5">
      <h3 className="text-sm font-semibold text-muted uppercase tracking-wider mb-4">
        {title}
      </h3>
      <div className="space-y-1">{children}</div>
    </div>
  );
}

// ─── Review Modal ─────────────────────────────────────────────────────────────

interface ReviewModalProps {
  changes: Array<{ key: string; oldValue: string; newValue: string }>;
  onBack: () => void;
  onSubmit: () => void;
  submitting: boolean;
}

function ReviewModal({ changes, onBack, onSubmit, submitting }: ReviewModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onBack();
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [onBack]);

  useEffect(() => {
    if (!dialogRef.current) return;
    const el = dialogRef.current;
    const focusable = Array.from(
      el.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      ),
    );
    if (focusable.length) focusable[0].focus();
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="review-title"
    >
      <div
        ref={dialogRef}
        className="bg-surface border border-border rounded-xl p-6 max-w-lg w-full"
      >
        <h3 id="review-title" className="text-lg font-semibold text-primary mb-1">
          Review Changes
        </h3>
        <p className="text-sm text-muted mb-5">
          {changes.length} setting{changes.length !== 1 ? 's' : ''} changed:
        </p>

        {/* item 35: shadow gradient to indicate scrollability */}
        <div className="relative">
          <div className="space-y-4 max-h-80 overflow-y-auto pr-1 pb-2">
            {changes.map(({ key, oldValue, newValue }) => (
              <div key={key} className="bg-slate-800/60 rounded-lg px-4 py-3">
                <p className="text-sm text-slate-300 font-mono mb-1">{key}</p>
                <p className="text-sm">
                  <span className="text-slate-400">{oldValue}</span>
                  <span className="text-slate-500 mx-2">&#8594;</span>
                  <span className="text-blue-400 font-medium">{newValue}</span>
                </p>
              </div>
            ))}
          </div>
          {/* fade indicator at bottom of scroll area */}
          <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-slate-900 to-transparent rounded-b-lg" />
        </div>

        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onBack}
            disabled={submitting}
            className="px-4 py-2 text-sm text-secondary hover:text-primary bg-surface-2 hover:bg-surface-hover rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
          >
            Back to Edit
          </button>
          <button
            onClick={onSubmit}
            disabled={submitting}
            className="px-4 py-2 text-sm text-on-accent bg-accent hover:bg-accent-hover rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
          >
            {submitting ? 'Submitting...' : 'Submit All'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Result Modal ─────────────────────────────────────────────────────────────

interface ResultModalProps {
  results: SubmitResult[];
  onDone: () => void;
}

function ResultModal({ results, onDone }: ResultModalProps) {
  const accepted = results.filter((r) => r.status === 'accepted').length;
  const rejected = results.filter((r) => r.status === 'rejected').length;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="result-title"
    >
      <div className="bg-surface border border-border rounded-xl p-6 max-w-lg w-full">
        <h3 id="result-title" className="text-lg font-semibold text-primary mb-1">
          Results
        </h3>
        <p className="text-sm text-muted mb-5">
          {accepted} accepted
          {rejected > 0 && (
            <span className="text-red-400 ml-1">/ {rejected} rejected</span>
          )}
        </p>

        <div className="space-y-3 max-h-80 overflow-y-auto pr-1">
          {results.map((r) => (
            <div
              key={r.key}
              className={`rounded-lg px-4 py-3 ${r.status === 'accepted' ? 'bg-emerald-900/20 border border-emerald-700/30' : 'bg-red-900/20 border border-red-700/30'}`}
            >
              {/* item 34: replace emoji with icon components */}
              <div className="flex items-center gap-2">
                {r.status === 'accepted'
                  ? <CheckCircleIcon className="w-4 h-4 text-emerald-400 shrink-0" />
                  : <XCircleIcon className="w-4 h-4 text-red-400 shrink-0" />}
                <span className="text-sm font-mono text-slate-200">{r.key}</span>
                <span
                  className={`text-xs ${r.status === 'accepted' ? 'text-emerald-400' : 'text-red-400'}`}
                >
                  {r.status}
                </span>
              </div>
              {r.message && (
                <p className="text-xs text-red-300 mt-1 ml-6">{r.message}</p>
              )}
            </div>
          ))}
        </div>

        <div className="flex justify-end mt-6">
          <button
            onClick={onDone}
            className="px-4 py-2 text-sm text-on-accent bg-accent hover:bg-accent-hover rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Main Page ────────────────────────────────────────────────────────────────

export default function SettingsPage() {
  const qc = useQueryClient();
  const { hasMinRole } = usePermission();
  const canEdit = hasMinRole('admin');

  const [mode, setMode] = useState<Mode>('view');
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [original, setOriginal] = useState<Record<string, string>>({});
  const [results, setResults] = useState<SubmitResult[]>([]);
  const [submitting, setSubmitting] = useState(false);

  const { data, isLoading, isError, refetch } = useQuery({
    queryKey: ['settings'],
    queryFn: getSettings,
  });

  const enterEdit = () => {
    if (!data) return;
    const snap: Record<string, string> = {};
    data.settings
      .filter((s) => s.editable)
      .forEach((s) => {
        snap[s.key] = String(s.value);
      });
    setOriginal(snap);
    setDraft({ ...snap });
    setMode('edit');
  };

  const cancelEdit = () => {
    setDraft({});
    setOriginal({});
    setMode('view');
  };

  const handleFieldChange = (key: string, value: string) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
  };

  const changes = Object.entries(draft).filter(([k, v]) => original[k] !== v);

  const buildChangeList = () =>
    changes.map(([key, newValue]) => ({
      key,
      oldValue: original[key] ?? '',
      newValue,
    }));

  const handleSubmit = async () => {
    setSubmitting(true);
    const res: SubmitResult[] = [];
    for (const [key, value] of changes) {
      try {
        await updateSetting(key, value);
        res.push({ key, status: 'accepted' });
      } catch (err) {
        res.push({
          key,
          status: 'rejected',
          message: (err as MutationError).userMessage ?? 'Failed',
        });
      }
    }
    setResults(res);
    setSubmitting(false);
    setMode('result');
  };

  const handleDone = () => {
    setDraft({});
    setOriginal({});
    setResults([]);
    setMode('view');
    void refetch();
  };

  const invalidateSettings = () => {
    void qc.invalidateQueries({ queryKey: ['settings'] });
  };

  // Keep data fresh after returning to view
  useEffect(() => {
    if (mode === 'view') invalidateSettings();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  if (isLoading) return <LoadingSkeleton />;
  if (isError || !data)
    return (
      // item 36: role="alert" with consistent error styling
      <div role="alert" className="bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400 text-sm">
        <p className="font-semibold">Failed to load settings.</p>
        <p className="text-sm mt-1">Please try again.</p>
      </div>
    );

  const settings = data.settings;
  const grouped: Record<string, SettingItem[]> = {};
  for (const s of settings) {
    const cat = categoryOf(s.key);
    if (!grouped[cat]) grouped[cat] = [];
    grouped[cat].push(s);
  }

  const changeCount = changes.length;

  return (
    <>
      {/* Review modal */}
      {mode === 'review' && (
        <ReviewModal
          changes={buildChangeList()}
          onBack={() => setMode('edit')}
          onSubmit={handleSubmit}
          submitting={submitting}
        />
      )}

      {/* Result modal */}
      {mode === 'result' && (
        <ResultModal results={results} onDone={handleDone} />
      )}

      <div className="space-y-6 max-w-2xl">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-2xl font-bold text-primary">System Settings</h2>
            <p className="text-sm text-muted mt-1">
              Runtime-tunable settings. Source shows where the active value comes from.
            </p>
          </div>

          <div className="flex items-center gap-2 flex-shrink-0">
            {mode === 'view' && canEdit && (
              <button
                onClick={enterEdit}
                className="px-3 py-2 text-sm font-medium text-on-accent bg-accent hover:bg-accent-hover rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
              >
                Edit Settings
              </button>
            )}

            {mode === 'edit' && (
              <>
                <button
                  onClick={cancelEdit}
                  className="px-3 py-2 text-sm text-secondary hover:text-primary bg-surface-2 hover:bg-surface-hover rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent"
                >
                  Cancel
                </button>
                <button
                  onClick={() => setMode('review')}
                  disabled={changeCount === 0}
                  className="px-3 py-2 text-sm font-medium text-on-accent bg-accent hover:bg-accent-hover rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  Review Changes{changeCount > 0 ? ` (${changeCount})` : ''}
                </button>
              </>
            )}
          </div>
        </div>

        {/* Settings groups */}
        {CATEGORY_ORDER.map((cat) => {
          const items = grouped[cat];
          if (!items || items.length === 0) return null;

          return (
            <Section key={cat} title={cat}>
              {items.map((s) =>
                mode === 'edit' ? (
                  <EditRow
                    key={s.key}
                    setting={s}
                    draftValue={s.editable ? (draft[s.key] ?? String(s.value)) : String(s.value)}
                    onChange={handleFieldChange}
                    changed={s.editable && draft[s.key] !== undefined && draft[s.key] !== original[s.key]}
                  />
                ) : (
                  <ViewRow key={s.key} setting={s} />
                ),
              )}
            </Section>
          );
        })}
      </div>
    </>
  );
}
