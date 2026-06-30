// Pipeline model + log-section parsing for the build detail view.
//
// A build's jobs each carry a pre_script, command, and post_script that all
// write into ONE stage log, separated by `[PRE]` / `[CMD]` / `[POST]` line
// markers (emitted by the worker's stage_runner). The controller's global
// post-script runs as a synthetic `__cleanup__` job.
//
// This module turns the flat `Job[]` into an ordered, expandable tree:
//   Stage 1 <name>   ▸  [pre]  [cmd]  [post]
//   Stage 2 <name>   ▸  [pre]  [cmd]  [post]
//   Global post      ▸  [cmd]
// and splits a stage log string into its pre/cmd/post sections so the right
// pane can show just the selected leaf's output.

import type { Job, JobState, JobGroup } from '../types';

export type PipelineLeafKind = 'pre' | 'cmd' | 'post';

export interface PipelineLeaf {
  /** Stable id used for selection (`${jobId}:${kind}`). */
  key: string;
  kind: PipelineLeafKind;
  /** Short label: "Pre-script", the stage/command name, or "Post-script". */
  label: string;
  /** Job whose log holds this leaf's output. */
  jobId: string;
  /** Derived status for just this sub-step. */
  state: JobState;
  exitCode: number | null;
}

export type PipelineNodeKind = 'stage' | 'global-post';

export interface PipelineNode {
  key: string;
  /** Display title, e.g. "Stage 1 · vira-ci" or "Global post-script". */
  title: string;
  kind: PipelineNodeKind;
  stageName: string;
  job: Job;
  /** Roll-up status for the whole node (the command's state). */
  state: JobState;
  leaves: PipelineLeaf[];
}

const CLEANUP_MARKER = '__cleanup__';

/** Map a phase exit code to a JobState, given the parent job's lifecycle. */
function leafStateFromExit(exitCode: number | null, parent: Job): JobState {
  // Phase hasn't reported yet → inherit the parent's running/pending state.
  if (exitCode === null || exitCode === undefined) {
    if (parent.state === 'running' || parent.state === 'assigned') return 'running';
    if (parent.state === 'queued') return 'queued';
    // Parent terminal but this phase never recorded a code → unknown.
    return parent.state === 'cancelled' ? 'cancelled' : 'unknown';
  }
  return exitCode === 0 ? 'success' : 'failed';
}

/** Build the ordered pipeline tree from a group's jobs. */
export function buildPipeline(jobs: Job[]): PipelineNode[] {
  const sorted = [...jobs].sort(
    (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime(),
  );

  const nodes: PipelineNode[] = [];
  let stageIndex = 0;

  for (const job of sorted) {
    const isCleanup = job.stage_name.includes(CLEANUP_MARKER);

    // A pre/post leaf exists if the script body is present OR the phase
    // actually ran (exit code recorded) — the latter keeps leaves visible
    // even when the API doesn't ship the script bodies.
    const hasPre = !!(job.pre_script && job.pre_script.trim()) || job.pre_exit_code != null;
    const hasPost = !!(job.post_script && job.post_script.trim()) || job.post_exit_code != null;

    const leaves: PipelineLeaf[] = [];
    if (!isCleanup && hasPre) {
      leaves.push({
        key: `${job.id}:pre`,
        kind: 'pre',
        label: 'Pre-script',
        jobId: job.id,
        state: leafStateFromExit(job.pre_exit_code, job),
        exitCode: job.pre_exit_code,
      });
    }
    // The command leaf always exists.
    leaves.push({
      key: `${job.id}:cmd`,
      kind: 'cmd',
      label: isCleanup ? 'Cleanup script' : job.stage_name,
      jobId: job.id,
      state: job.state,
      exitCode: job.exit_code,
    });
    if (!isCleanup && hasPost) {
      leaves.push({
        key: `${job.id}:post`,
        kind: 'post',
        label: 'Post-script',
        jobId: job.id,
        state: leafStateFromExit(job.post_exit_code, job),
        exitCode: job.post_exit_code,
      });
    }

    if (isCleanup) {
      nodes.push({
        key: job.id,
        title: 'Global post-script',
        kind: 'global-post',
        stageName: job.stage_name,
        job,
        state: job.state,
        leaves,
      });
    } else {
      stageIndex += 1;
      nodes.push({
        key: job.id,
        title: `Stage ${stageIndex} · ${job.stage_name}`,
        kind: 'stage',
        stageName: job.stage_name,
        job,
        state: job.state,
        leaves,
      });
    }
  }

  return nodes;
}

// ── Parallelism inference (for the Blue-Ocean-style graph) ──────────────────
//
// chola doesn't ship the stage DAG in the build-detail payload, so we infer
// concurrency from execution windows: stages whose [start, end] intervals
// overlap ran in parallel and share a "row" (drawn as side-by-side lanes);
// a stage that starts after everything in the current row has finished
// begins a new sequential row. Each row is one split/join band in the graph.

function startMs(node: PipelineNode): number {
  const s = node.job.started_at ?? node.job.created_at;
  return s ? new Date(s).getTime() : 0;
}

function endMs(node: PipelineNode): number {
  // Still-running stages extend to "now" so they overlap anything after them.
  return node.job.completed_at ? new Date(node.job.completed_at).getTime() : Number.MAX_SAFE_INTEGER;
}

/**
 * Group pipeline nodes into sequential rows of parallel lanes. Row order is
 * by start time; within a row, lanes are stages that overlapped in time.
 */
export function clusterRows(nodes: PipelineNode[]): PipelineNode[][] {
  const sorted = [...nodes].sort((a, b) => startMs(a) - startMs(b));
  const rows: PipelineNode[][] = [];
  let row: PipelineNode[] = [];
  let rowEnd = -Infinity;

  for (const node of sorted) {
    if (row.length === 0 || startMs(node) < rowEnd) {
      row.push(node);
      rowEnd = Math.max(rowEnd, endMs(node));
    } else {
      rows.push(row);
      row = [node];
      rowEnd = endMs(node);
    }
  }
  if (row.length) rows.push(row);
  return rows;
}

export interface LogSections {
  pre: string;
  cmd: string;
  post: string;
}

/**
 * Split a stage log into pre/cmd/post sections using the worker's
 * `[PRE]` / `[CMD]` / `[POST]` line markers. `[LOCK]` and any other lines
 * stay with whichever section is currently open. Lines before the first
 * marker default to `pre` (workspace setup runs under the pre phase).
 */
export function parseLogSections(log: string): LogSections {
  const out: LogSections = { pre: '', cmd: '', post: '' };
  if (!log) return out;

  const buf: Record<PipelineLeafKind, string[]> = { pre: [], cmd: [], post: [] };
  let current: PipelineLeafKind = 'pre';

  for (const line of log.split('\n')) {
    if (line.startsWith('[PRE]')) current = 'pre';
    else if (line.startsWith('[CMD]')) current = 'cmd';
    else if (line.startsWith('[POST]')) current = 'post';
    buf[current].push(line);
  }

  out.pre = buf.pre.join('\n');
  out.cmd = buf.cmd.join('\n');
  out.post = buf.post.join('\n');
  return out;
}

/** Pull just one leaf's slice out of a full stage log. */
export function sectionForLeaf(log: string, kind: PipelineLeafKind): string {
  return parseLogSections(log)[kind];
}

// ── Full vertical model (global pre/post boxes + stages) ────────────────────

/**
 * A selectable step in the graph. A `job` step points at a real job's log
 * section; an `info` step (controller-scope global script) has no captured
 * output and just shows an explanatory message.
 */
export type StepRef =
  | {
      type: 'job';
      key: string;
      jobId: string;
      kind: PipelineLeafKind;
      label: string;
      state: JobState;
      exitCode: number | null;
    }
  | { type: 'info'; key: string; label: string; message: string; state: JobState };

export interface GlobalBox {
  present: boolean;
  /** Sub-steps revealed on expand (worker / controller variants). */
  steps: StepRef[];
}

export interface PipelineModel {
  globalPre: GlobalBox;
  stages: PipelineNode[];
  globalPost: GlobalBox;
}

const isWorker = (scope?: string | null) => scope === 'worker' || scope === 'both';
const isController = (scope?: string | null) => scope === 'controller' || scope === 'both';

/** Convert a stage leaf into a selectable job step. */
export function leafToStep(leaf: PipelineLeaf): StepRef {
  return {
    type: 'job',
    key: leaf.key,
    jobId: leaf.jobId,
    kind: leaf.kind,
    label: leaf.label,
    state: leaf.state,
    exitCode: leaf.exitCode,
  };
}

/** Build the full vertical pipeline model from jobs + the group's repo-level
 *  global scripts. */
export function buildPipelineModel(jobs: Job[], group: JobGroup): PipelineModel {
  const nodes = buildPipeline(jobs);
  const stages = nodes.filter((n) => n.kind === 'stage');
  const cleanup = nodes.find((n) => n.kind === 'global-post') ?? null;
  const firstStage = stages[0] ?? null;

  // Global pre-script: worker-scope is prepended to the first stage's pre
  // phase (no separate job); controller-scope has no captured output.
  const preSteps: StepRef[] = [];
  if (isWorker(group.global_pre_script_scope)) {
    const preLeaf = firstStage?.leaves.find((l) => l.kind === 'pre');
    if (firstStage && preLeaf) {
      preSteps.push({
        type: 'job',
        key: 'global-pre:worker',
        jobId: firstStage.job.id,
        kind: 'pre',
        label: 'Worker',
        state: preLeaf.state,
        exitCode: preLeaf.exitCode,
      });
    }
  }
  if (isController(group.global_pre_script_scope)) {
    preSteps.push({
      type: 'info',
      key: 'global-pre:controller',
      label: 'Controller',
      message: 'Global pre-script runs on the controller; its output is not captured.',
      state: 'unknown',
    });
  }

  // Global post-script: worker-scope is the __cleanup__ job; controller-scope
  // has no captured output.
  const postSteps: StepRef[] = [];
  if (isWorker(group.global_post_script_scope)) {
    if (cleanup) {
      postSteps.push({
        type: 'job',
        key: 'global-post:worker',
        jobId: cleanup.job.id,
        kind: 'cmd',
        label: 'Worker',
        state: cleanup.state,
        exitCode: cleanup.job.exit_code,
      });
    } else {
      postSteps.push({
        type: 'info',
        key: 'global-post:worker',
        label: 'Worker',
        message: 'Global post-script runs on the worker at group completion (no record yet).',
        state: 'queued',
      });
    }
  }
  if (isController(group.global_post_script_scope)) {
    postSteps.push({
      type: 'info',
      key: 'global-post:controller',
      label: 'Controller',
      message: 'Global post-script runs on the controller; its output is not captured.',
      state: 'unknown',
    });
  }

  return {
    globalPre: { present: !!group.global_pre_script, steps: preSteps },
    stages,
    globalPost: { present: !!group.global_post_script || !!cleanup, steps: postSteps },
  };
}
