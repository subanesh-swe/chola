import { useState, useEffect, useRef } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { getSettings, updateSetting, SettingItem } from '../api/settings';
import { LoadingSkeleton } from '../components/ui';
import { usePermission } from '../hooks/usePermission';

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

const CATEGORY_ORDER = ['Scheduling', 'Workers', 'Logging', 'Retention', 'Server / Auth'];

// ─── Helpers ─────────────────────────────────────────────────────────────────

function categoryOf(key: string): string {
  if (key.startsWith('scheduling.')) return 'Scheduling';
  if (key.startsWith('workers.')) return 'Workers';
  if (key.startsWith('logging.')) return 'Logging';
  if (key.startsWith('retention.')) return 'Retention';
  return 'Server / Auth';
}

function labelOf(key: string): string {
  const parts = key.split('.');
  return parts[parts.length - 1].replace(/_/g, ' ');
}

// ─── Sub-components ──────────────────────────────────────────────────────────

function SourceBadge({ source }: { source: string }) {
  const styles: Record<string, string> = {
    database: 'bg-blue-900/40 text-blue-400 border-blue-500/30',
    config: 'bg-slate-700 text-slate-400 border-slate-600/30',
    default: 'bg-gray-600/30 text-gray-500 border-gray-500/30',
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
      className={`px-2 py-0.5 rounded text-xs font-medium ${on ? 'bg-emerald-500/20 text-emerald-400' : 'bg-slate-700 text-slate-400'}`}
    >
      {on ? 'enabled' : 'disabled'}
    </span>
  );
}

function ValueDisplay({ setting }: { setting: SettingItem }) {
  if (setting.type === 'bool') return <BoolDisplay value={setting.value} />;
  return <span className="text-sm text-white font-mono">{String(setting.value)}</span>;
}

// ─── View mode rows ───────────────────────────────────────────────────────────

function ViewRow({ setting }: { setting: SettingItem }) {
  return (
    <div className="flex items-center justify-between py-2 border-b border-slate-800 last:border-0 gap-3">
      <div className="flex items-center gap-3 min-w-0">
        <span className="text-sm text-slate-300 capitalize">{labelOf(setting.key)}</span>
        <SourceBadge source={setting.source} />
        {!setting.editable && (
          <span className="text-slate-600" title="Read-only — requires restart">
            &#128274;
          </span>
        )}
      </div>
      <div className="flex-shrink-0">
        <ValueDisplay setting={setting} />
      </div>
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
  const ringClass = changed ? 'ring-2 ring-blue-500/30 border-blue-500/50' : 'border-slate-600';

  if (setting.options) {
    return (
      <select
        value={draftValue}
        onChange={(e) => onChange(setting.key, e.target.value)}
        className={`bg-slate-800 border rounded px-2 py-1 text-sm text-white ${ringClass}`}
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
        className={`bg-slate-800 border rounded px-2 py-1 text-sm text-white ${ringClass}`}
      >
        <option value="true">enabled</option>
        <option value="false">disabled</option>
      </select>
    );
  }

  return (
    <input
      type="number"
      value={draftValue}
      onChange={(e) => onChange(setting.key, e.target.value)}
      min={setting.min}
      max={setting.max}
      className={`bg-slate-800 border rounded px-2 py-1 text-sm text-white w-28 ${ringClass}`}
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
      <div className="flex items-center justify-between py-2 border-b border-slate-800 last:border-0 gap-3 opacity-60">
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-sm text-slate-400 capitalize">{labelOf(setting.key)}</span>
          <SourceBadge source={setting.source} />
          <span className="text-slate-600" title="Read-only — requires restart">
            &#128274;
          </span>
        </div>
        <div className="flex-shrink-0">
          <ValueDisplay setting={setting} />
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between py-2 border-b border-slate-800 last:border-0 gap-3">
      <div className="flex items-center gap-3 min-w-0">
        <span className="text-sm text-slate-300 capitalize">{labelOf(setting.key)}</span>
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
  );
}

// ─── Section ──────────────────────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-slate-900 border border-slate-700 rounded-xl p-5">
      <h3 className="text-sm font-semibold text-slate-400 uppercase tracking-wider mb-4">
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
        className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full"
      >
        <h3 id="review-title" className="text-lg font-semibold text-white mb-1">
          Review Changes
        </h3>
        <p className="text-sm text-slate-400 mb-5">
          {changes.length} setting{changes.length !== 1 ? 's' : ''} changed:
        </p>

        <div className="space-y-4 max-h-80 overflow-y-auto pr-1">
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

        <div className="flex justify-end gap-3 mt-6">
          <button
            onClick={onBack}
            disabled={submitting}
            className="px-4 py-2 text-sm text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          >
            Back to Edit
          </button>
          <button
            onClick={onSubmit}
            disabled={submitting}
            className="px-4 py-2 text-sm text-white bg-blue-600 hover:bg-blue-500 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
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
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-lg w-full">
        <h3 id="result-title" className="text-lg font-semibold text-white mb-1">
          Results
        </h3>
        <p className="text-sm text-slate-400 mb-5">
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
              <div className="flex items-center gap-2">
                <span className="text-base leading-none">
                  {r.status === 'accepted' ? '✅' : '❌'}
                </span>
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
            className="px-4 py-2 text-sm text-white bg-blue-600 hover:bg-blue-500 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
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
      <div className="text-red-400 bg-red-900/20 border border-red-800 rounded p-3 text-sm">
        Failed to load settings.
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
            <h2 className="text-2xl font-bold text-white">System Settings</h2>
            <p className="text-sm text-slate-500 mt-1">
              Runtime-tunable settings. Source shows where the active value comes from.
            </p>
          </div>

          <div className="flex items-center gap-2 flex-shrink-0">
            {mode === 'view' && canEdit && (
              <button
                onClick={enterEdit}
                className="px-3 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-500 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                Edit Settings
              </button>
            )}

            {mode === 'edit' && (
              <>
                <button
                  onClick={cancelEdit}
                  className="px-3 py-2 text-sm text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  Cancel
                </button>
                <button
                  onClick={() => setMode('review')}
                  disabled={changeCount === 0}
                  className="px-3 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-500 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-40 disabled:cursor-not-allowed"
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
