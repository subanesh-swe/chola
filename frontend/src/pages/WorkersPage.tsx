import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listWorkers, drainWorker, undrainWorker } from '../api/workers';
import { StatusBadge } from '../components/ui/StatusBadge';
import { ResourceBar } from '../components/ui/ResourceBar';
import { TimeAgo } from '../components/ui/TimeAgo';
import { usePermission } from '../hooks/usePermission';
import { toast } from 'sonner';

export default function WorkersPage() {
  const qc = useQueryClient();
  const { canManageWorkers } = usePermission();
  const { data, isLoading } = useQuery({ queryKey: ['workers'], queryFn: listWorkers, refetchInterval: 5000 });

  const drainMut = useMutation({
    mutationFn: (id: string) => drainWorker(id),
    onSuccess: () => { toast.success('Worker set to drain'); qc.invalidateQueries({ queryKey: ['workers'] }); },
  });
  const undrainMut = useMutation({
    mutationFn: (id: string) => undrainWorker(id),
    onSuccess: () => { toast.success('Worker undrained'); qc.invalidateQueries({ queryKey: ['workers'] }); },
  });

  const workers = data?.workers ?? [];

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-bold text-white">Workers ({workers.length})</h2>

      {isLoading ? <div className="text-slate-400">Loading...</div> : (
        <div className="grid gap-4">
          {workers.map(w => (
            <div key={w.worker_id} className="bg-slate-900 border border-slate-700 rounded-xl p-4">
              <div className="flex items-center justify-between mb-4">
                <div className="flex items-center gap-3">
                  <StatusBadge status={w.status} size="md" />
                  <div>
                    <p className="text-lg font-semibold text-white">{w.worker_id}</p>
                    <p className="text-sm text-slate-400">{w.hostname} &middot; {w.disk_type} &middot; Docker: {w.docker_enabled ? 'Yes' : 'No'}</p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {w.last_heartbeat && <span className="text-xs text-slate-500">Jobs: {w.last_heartbeat.running_jobs}</span>}
                  {canManageWorkers && w.status === 'Connected' && (
                    <button onClick={() => drainMut.mutate(w.worker_id)}
                      className="px-3 py-1 text-xs bg-yellow-500/20 text-yellow-400 border border-yellow-500/30 rounded-lg hover:bg-yellow-500/30">Drain</button>
                  )}
                  {canManageWorkers && w.status === 'Draining' && (
                    <button onClick={() => undrainMut.mutate(w.worker_id)}
                      className="px-3 py-1 text-xs bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 rounded-lg hover:bg-emerald-500/30">Undrain</button>
                  )}
                </div>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <ResourceBar label="CPU" used={w.last_heartbeat?.used_cpu_percent ?? 0} total={100} unit="%" />
                <ResourceBar label="Memory" used={w.last_heartbeat?.used_memory_mb ?? 0} total={w.total_memory_mb} unit=" MB" />
                <ResourceBar label="Disk" used={w.last_heartbeat?.used_disk_mb ?? 0} total={w.total_disk_mb} unit=" MB" />
              </div>
              <div className="mt-3 flex gap-4 text-xs text-slate-500">
                <span>Types: {w.supported_job_types.join(', ')}</span>
                <span>Registered: <TimeAgo date={w.registered_at} /></span>
                {w.last_heartbeat && <span>Last beat: <TimeAgo date={w.last_heartbeat.timestamp} /></span>}
              </div>
            </div>
          ))}
          {!workers.length && <div className="text-center py-8 text-slate-500">No workers registered</div>}
        </div>
      )}
    </div>
  );
}
