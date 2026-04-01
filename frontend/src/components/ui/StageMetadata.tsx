import type { Job } from '../../types';
import { formatDuration } from '../../utils/duration';

interface Props {
  job: Job;
}

export function StageMetadata({ job }: Props) {
  return (
    <dl className="px-4 py-2 text-xs text-slate-500 space-y-1">
      <div className="flex flex-wrap gap-x-6 gap-y-1">
        <div>
          <dt className="inline">Command: </dt>
          <dd className="inline"><code className="text-slate-300">{job.command}</code></dd>
        </div>
        {job.pre_script && (
          <div>
            <dt className="inline">Pre-script: </dt>
            <dd className="inline"><code className="text-slate-300 break-all">{job.pre_script}</code></dd>
          </div>
        )}
        {job.post_script && (
          <div>
            <dt className="inline">Post-script: </dt>
            <dd className="inline"><code className="text-slate-300 break-all">{job.post_script}</code></dd>
          </div>
        )}
        {job.worker_id && (
          <div>
            <dt className="inline">Worker: </dt>
            <dd className="inline text-slate-400">{job.worker_id}</dd>
          </div>
        )}
      </div>
      <div className="flex flex-wrap gap-x-6 gap-y-1">
        {job.exit_code !== null && (
          <div>
            <dt className="inline">Exit: </dt>
            <dd className="inline text-slate-400">{job.exit_code}</dd>
          </div>
        )}
        {job.pre_exit_code !== null && (
          <div>
            <dt className="inline">Pre exit: </dt>
            <dd className="inline text-slate-400">{job.pre_exit_code}</dd>
          </div>
        )}
        {job.post_exit_code !== null && (
          <div>
            <dt className="inline">Post exit: </dt>
            <dd className="inline text-slate-400">{job.post_exit_code}</dd>
          </div>
        )}
        <div>
          <dt className="inline">Duration: </dt>
          <dd className="inline text-slate-400">{formatDuration(job.started_at, job.completed_at)}</dd>
        </div>
      </div>
    </dl>
  );
}
