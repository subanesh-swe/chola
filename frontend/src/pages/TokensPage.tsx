import { useState } from 'react';
import WorkerTokensPage from './WorkerTokensPage';

type TabKey = 'worker' | 'runner';

const tabs: { key: TabKey; label: string; description: string }[] = [
  { key: 'worker', label: 'Worker Tokens', description: 'Authenticate workers with the controller. Create from Workers page or here.' },
  { key: 'runner', label: 'Runner Tokens', description: 'Authenticate ci-job-runner and automation scripts. Set CHOLA_TOKEN env var.' },
];

export default function TokensPage() {
  const [activeTab, setActiveTab] = useState<TabKey>('worker');

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-bold text-white">Tokens</h2>

      <div className="flex border-b border-slate-700">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`px-4 py-2.5 text-sm font-medium transition-colors relative ${
              activeTab === tab.key
                ? 'text-blue-400'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            {tab.label}
            {activeTab === tab.key && (
              <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-blue-500" />
            )}
          </button>
        ))}
      </div>

      <p className="text-xs text-slate-500">
        {tabs.find((t) => t.key === activeTab)?.description}
      </p>

      {activeTab === 'worker' && <WorkerTokensPage filterScope="worker" defaultScope="worker" />}
      {activeTab === 'runner' && <WorkerTokensPage filterScope="runner" defaultScope="runner" />}
    </div>
  );
}
