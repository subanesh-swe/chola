import { useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getRepo, listStageConfigs, createStageConfig, deleteStageConfig } from '../api/repos';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';

export default function RepoDetailPage() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const qc = useQueryClient();
  const { canManageRepos } = usePermission();
  const [showAdd, setShowAdd] = useState(false);
  const [stageName, setStageName] = useState('');
  const [command, setCommand] = useState('');

  const { data: repo } = useQuery({ queryKey: ['repo', id], queryFn: () => getRepo(id!), enabled: !!id });
  const { data: stagesData } = useQuery({ queryKey: ['stages', id], queryFn: () => listStageConfigs(id!), enabled: !!id });

  const addStage = useMutation({
    mutationFn: () => createStageConfig(id!, { stage_name: stageName, command }),
    onSuccess: () => { toast.success('Stage created'); qc.invalidateQueries({ queryKey: ['stages', id] }); setShowAdd(false); setStageName(''); setCommand(''); },
    onError: () => toast.error('Failed to create stage'),
  });

  const delStage = useMutation({
    mutationFn: (stageId: string) => deleteStageConfig(id!, stageId),
    onSuccess: () => { toast.success('Stage deleted'); qc.invalidateQueries({ queryKey: ['stages', id] }); },
  });

  const stages = stagesData?.stages ?? [];

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <button onClick={() => nav('/repos')} className="text-slate-400 hover:text-white text-sm">&lt; Repos</button>
        <h2 className="text-2xl font-bold text-white">{repo?.repo_name || 'Loading...'}</h2>
      </div>

      {repo && (
        <div className="bg-slate-900 border border-slate-700 rounded-xl p-4 grid grid-cols-2 md:grid-cols-4 gap-4">
          <div><p className="text-xs text-slate-500">URL</p><p className="text-sm text-slate-200 font-mono truncate">{repo.repo_url}</p></div>
          <div><p className="text-xs text-slate-500">Default Branch</p><p className="text-sm text-slate-200">{repo.default_branch}</p></div>
          <div><p className="text-xs text-slate-500">Enabled</p><p className="text-sm text-slate-200">{repo.enabled ? 'Yes' : 'No'}</p></div>
          <div><p className="text-xs text-slate-500">Stages</p><p className="text-sm text-slate-200">{stages.length}</p></div>
        </div>
      )}

      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-white">Stage Configs</h3>
        {canManageRepos && <button onClick={() => setShowAdd(true)} className="px-3 py-1.5 text-sm bg-blue-600 text-white rounded-lg">Add Stage</button>}
      </div>

      <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
        <table className="w-full">
          <thead><tr className="border-b border-slate-700">
            <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Order</th>
            <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Name</th>
            <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Command</th>
            <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Resources</th>
            <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Type</th>
            <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Timeout</th>
            {canManageRepos && <th className="px-4 py-3 text-xs text-slate-400 uppercase">Actions</th>}
          </tr></thead>
          <tbody className="divide-y divide-slate-800">
            {stages.sort((a, b) => a.execution_order - b.execution_order).map(s => (
              <tr key={s.id}>
                <td className="px-4 py-3 text-sm text-slate-400">{s.execution_order}</td>
                <td className="px-4 py-3 text-sm text-slate-200 font-medium">{s.stage_name}</td>
                <td className="px-4 py-3 text-sm text-slate-400 font-mono truncate max-w-xs">{s.command}</td>
                <td className="px-4 py-3 text-xs text-slate-400">{s.required_cpu}c / {s.required_memory_mb}MB / {s.required_disk_mb}MB</td>
                <td className="px-4 py-3 text-sm text-slate-400">{s.job_type}</td>
                <td className="px-4 py-3 text-sm text-slate-400">{s.max_duration_secs}s</td>
                {canManageRepos && <td className="px-4 py-3 text-center"><button onClick={() => delStage.mutate(s.id)} className="text-xs text-red-400 hover:text-red-300">Delete</button></td>}
              </tr>
            ))}
            {!stages.length && <tr><td colSpan={7} className="px-4 py-8 text-center text-slate-500">No stages configured</td></tr>}
          </tbody>
        </table>
      </div>

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-4">Add Stage</h3>
            <div className="space-y-3">
              <div><label className="block text-sm text-slate-300 mb-1">Stage Name</label><input value={stageName} onChange={e => setStageName(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" placeholder="build" /></div>
              <div><label className="block text-sm text-slate-300 mb-1">Command</label><input value={command} onChange={e => setCommand(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" placeholder="make build" /></div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg">Cancel</button>
              <button onClick={() => addStage.mutate()} disabled={!stageName || !command} className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50">Create</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
