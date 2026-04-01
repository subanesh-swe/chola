import { useNavigate } from 'react-router-dom';

export type ResultType = 'build' | 'repo' | 'worker';

export interface SearchResultItem {
  type: ResultType;
  id: string;
  title: string;
  subtitle?: string;
  href: string;
}

function BuildIcon() {
  return (
    <svg className="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
    </svg>
  );
}

function RepoIcon() {
  return (
    <svg className="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
    </svg>
  );
}

function WorkerIcon() {
  return (
    <svg className="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
    </svg>
  );
}

const iconMap: Record<ResultType, () => React.ReactElement> = {
  build: BuildIcon,
  repo: RepoIcon,
  worker: WorkerIcon,
};

const colorMap: Record<ResultType, string> = {
  build: 'text-blue-400',
  repo: 'text-emerald-400',
  worker: 'text-amber-400',
};

interface Props {
  item: SearchResultItem;
  onClose: () => void;
}

export function SearchResult({ item, onClose }: Props) {
  const nav = useNavigate();
  const Icon = iconMap[item.type];

  function handleClick() {
    nav(item.href);
    onClose();
  }

  return (
    <button
      onClick={handleClick}
      className="w-full flex items-center gap-3 px-4 py-2.5 text-left hover:bg-slate-700 transition-colors rounded-lg"
    >
      <span className={colorMap[item.type]}><Icon /></span>
      <div className="min-w-0">
        <p className="text-sm text-white truncate">{item.title}</p>
        {item.subtitle && <p className="text-xs text-slate-500 truncate">{item.subtitle}</p>}
      </div>
      <span className="ml-auto text-[10px] text-slate-600 uppercase shrink-0">{item.type}</span>
    </button>
  );
}
