import { useState } from 'react';
import WorkerTokensPage from './WorkerTokensPage';

type TabKey = 'worker' | 'runner' | 'api';

const tabs: { key: TabKey; label: string; description: string }[] = [
  { key: 'worker', label: 'Worker Tokens', description: 'Registration tokens for workers to authenticate with the controller' },
  { key: 'runner', label: 'Runner Tokens', description: 'Service tokens for ci-job-runner to authenticate gRPC and REST calls' },
  { key: 'api', label: 'API Keys', description: 'Personal API keys for REST API access (manage in Profile)' },
];

export default function TokensPage() {
  const [activeTab, setActiveTab] = useState<TabKey>('worker');

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-bold text-white">Tokens</h2>

      {/* Tab bar */}
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

      {/* Tab description */}
      <p className="text-xs text-slate-500">
        {tabs.find((t) => t.key === activeTab)?.description}
      </p>

      {/* Tab content */}
      {activeTab === 'worker' && <WorkerTokensPage filterScope="worker" />}
      {activeTab === 'runner' && <WorkerTokensPage filterScope="runner" defaultScope="runner" />}
      {activeTab === 'api' && (
        <div className="bg-slate-900 border border-slate-700 rounded-xl p-8 text-center">
          <p className="text-slate-400 text-sm">API keys are managed per-user.</p>
          <a
            href="/profile"
            className="inline-block mt-3 px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700"
          >
            Go to Profile &rarr;
          </a>
        </div>
      )}
    </div>
  );
}
