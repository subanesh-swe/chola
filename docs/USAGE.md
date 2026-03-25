# rust-ci-orchestrator — Usage Guide

> Example commands for every workflow. All commands assume you're in the project root.

---

## Prerequisites

```bash
# Start infrastructure (if using local dev)
docker compose up -d postgres redis opensearch   # or however you run them

# Run database migrations
just migrate

# Build all binaries
just build
```

---

## 1. Start the Controller

```bash
# Terminal 1
just controller

# Or manually:
cargo run -p ci-controller -- --config config/controller.example.yaml
```

The controller starts:
- gRPC server on `0.0.0.0:50051`
- HTTP health server on `0.0.0.0:8080`

Verify health:
```bash
curl http://localhost:8080/health/live
# {"status":"ok"}

curl http://localhost:8080/health/ready
# {"status":"ok"}
```

---

## 2. Start Workers

```bash
# Terminal 2 — Worker 1
just worker worker-1

# Terminal 3 — Worker 2
just worker worker-2

# Or manually:
cargo run -p ci-worker -- --config config/worker-1.example.yaml
cargo run -p ci-worker -- --config config/worker-2.example.yaml
```

Each worker starts:
- gRPC client connecting to controller
- HTTP health server on `0.0.0.0:8081`

Verify worker health:
```bash
curl http://localhost:8081/health/live
# {"status":"ok"}
```

---

## 3. Legacy: Submit a Single Job

For quick one-off commands (backward compatible):

```bash
# Submit with explicit job ID
just submit job-001 "echo hello world"

# Submit with auto-generated job ID
just submit-auto "echo hello && sleep 5 && echo done"

# Longer commands with pipes and variables
just submit-auto "echo 'started at $(date)' && ls -la && echo 'done'"

# Or directly:
cargo run -p ci-job-runner -- \
    --controller http://localhost:50051 \
    submit \
    --job-id my-job-001 \
    -- \
    'echo hello world'
```

The job-runner will:
1. Submit the job
2. Stream logs in real-time
3. Exit when the job completes
4. Handle Ctrl+C (sends cancel, waits for cleanup)

---

## 4. Multi-Stage Pipeline (Jenkins Workflow)

This is the primary use case — reserve a worker, run stages, check status.

### Step 1: Reserve a Worker

```bash
# Reserve for a repo with specific stages
just reserve my-service "build,test,push-docker-image"

# With full options
just reserve-full my-service "git@github.com:org/my-service.git" main abc123 "build,test,push-docker-image"

# Direct CLI
cargo run -p ci-job-runner -- \
    --controller http://localhost:50051 \
    reserve \
    --repo my-service \
    --repo-url "git@github.com:org/my-service.git" \
    --branch main \
    --commit abc123 \
    --stages "build,test,push-docker-image"
```

Output:
```
job_group_id=a1b2c3d4-e5f6-... worker=worker-1 stages=build,test,push-docker-image
```

Save the `job_group_id` for the next steps.

### Step 2: Run Stages

```bash
# Run build stage
just run-stage a1b2c3d4-e5f6-... build

# Run test stage (can run in parallel if configured)
just run-stage a1b2c3d4-e5f6-... test

# Run push stage
just run-stage a1b2c3d4-e5f6-... push-docker-image

# With explicit job ID
just run-stage-with-id a1b2c3d4-e5f6-... my-build-job build

# Direct CLI
cargo run -p ci-job-runner -- \
    --controller http://localhost:50051 \
    run \
    --job-group-id a1b2c3d4-e5f6-... \
    --stage build
```

Each `run` command:
1. Submits the stage to the reserved worker
2. Streams logs in real-time to stdout
3. Exits with the stage's exit code
4. Handles Ctrl+C (cancels stage, runs post_script, then exits)

### Step 3: Check Status

```bash
just status a1b2c3d4-e5f6-...

# Direct CLI
cargo run -p ci-job-runner -- \
    --controller http://localhost:50051 \
    status \
    --job-group-id a1b2c3d4-e5f6-...
```

Output:
```
Job Group: a1b2c3d4-e5f6-...
State:     running
Worker:    worker-1
------------------------------------------------------------
STAGE                JOB ID                               STATE        EXIT
------------------------------------------------------------
build                b1b2c3d4-...                         success      0
test                 c1c2c3d4-...                         running      0
push-docker-image    (not started)                        queued       0
------------------------------------------------------------
```

---

## 5. Watch Logs

```bash
# Watch all stages in a group
just logs-group a1b2c3d4-e5f6-...

# Watch a specific stage
just logs-stage a1b2c3d4-e5f6-... build

# Watch a specific job ID
just logs-job b1b2c3d4-...

# Direct CLI
cargo run -p ci-job-runner -- \
    --controller http://localhost:50051 \
    logs \
    --job-group-id a1b2c3d4-e5f6-... \
    --stage build
```

Press Ctrl+C to stop watching (does NOT cancel the job).

---

## 6. Cancel

```bash
# Cancel an entire job group (all stages)
just cancel-group a1b2c3d4-e5f6-...

# Cancel a specific stage
just cancel-job b1b2c3d4-...

# Direct CLI
cargo run -p ci-job-runner -- \
    --controller http://localhost:50051 \
    cancel \
    --job-group-id a1b2c3d4-e5f6-... \
    --reason "Build no longer needed"
```

When a stage is cancelled:
1. The command process receives SIGINT
2. After 5s grace period, SIGKILL if still alive
3. **Post-script always runs** (cleanup, workspace teardown, etc.)
4. Stage reports CANCELLED only after post_script completes

---

## 7. Pipeline Helper (Reserve + Instructions)

```bash
just pipeline my-service "build,test,push-docker-image"
```

Output:
```
=== Reserving worker for my-service ===
job_group_id=a1b2c3d4-e5f6-... worker=worker-1 stages=build,test,push-docker-image

=== Reserved: a1b2c3d4-e5f6-... ===
Run stages with:
  just run-stage a1b2c3d4-e5f6-... build
  just run-stage a1b2c3d4-e5f6-... test
  just run-stage a1b2c3d4-e5f6-... push-docker-image

Check status with:
  just status a1b2c3d4-e5f6-...
```

---

## 8. Jenkins Integration Example

In a Jenkinsfile:

```groovy
pipeline {
    agent any
    environment {
        CI_CONTROLLER = "http://ci-controller.internal:50051"
        CI_RUNNER = "/usr/local/bin/ci-job-runner"
    }
    stages {
        stage('Reserve Worker') {
            steps {
                script {
                    def output = sh(
                        script: """
                            ${CI_RUNNER} --controller ${CI_CONTROLLER} reserve \
                                --repo ${env.JOB_NAME} \
                                --repo-url ${env.GIT_URL} \
                                --branch ${env.BRANCH_NAME} \
                                --commit ${env.GIT_COMMIT} \
                                --stages "build,test,push-docker-image"
                        """,
                        returnStdout: true
                    ).trim()
                    // Parse: job_group_id=xxx worker=yyy stages=zzz
                    env.JOB_GROUP_ID = output.split(' ')[0].split('=')[1]
                }
            }
        }
        stage('Build') {
            steps {
                sh """
                    ${CI_RUNNER} --controller ${CI_CONTROLLER} run \
                        --job-group-id ${env.JOB_GROUP_ID} \
                        --stage build
                """
            }
        }
        stage('Test') {
            steps {
                sh """
                    ${CI_RUNNER} --controller ${CI_CONTROLLER} run \
                        --job-group-id ${env.JOB_GROUP_ID} \
                        --stage test
                """
            }
        }
        stage('Push Docker Image') {
            steps {
                sh """
                    ${CI_RUNNER} --controller ${CI_CONTROLLER} run \
                        --job-group-id ${env.JOB_GROUP_ID} \
                        --stage push-docker-image
                """
            }
        }
    }
    post {
        failure {
            sh """
                ${CI_RUNNER} --controller ${CI_CONTROLLER} cancel \
                    --job-group-id ${env.JOB_GROUP_ID} \
                    --reason "Jenkins pipeline failed"
            """
        }
        always {
            sh """
                ${CI_RUNNER} --controller ${CI_CONTROLLER} status \
                    --job-group-id ${env.JOB_GROUP_ID}
            """
        }
    }
}
```

---

## 9. Infrastructure Commands

```bash
# Start Vector for log shipping to OpenSearch
just vector

# Run database migrations
just migrate

# Development
just build     # build all binaries
just check     # type-check without linking
just lint      # run clippy
just fmt       # format code
```

---

## 10. Health & Monitoring Endpoints

### Controller (port 8080)

| Endpoint | Description |
|----------|-------------|
| `GET /health/live` | Liveness probe — always returns 200 |
| `GET /health/ready` | Readiness probe — checks deps |
| `GET /metrics` | Prometheus metrics |
| `GET /api/v1/workers` | List connected workers |
| `GET /api/v1/builds` | List active job groups |

### Worker (port 8081)

| Endpoint | Description |
|----------|-------------|
| `GET /health/live` | Liveness probe |
| `GET /health/ready` | Readiness — checks controller connection |
| `GET /metrics` | Prometheus metrics |

```bash
# Quick health check
curl -s http://localhost:8080/health/live | jq .
curl -s http://localhost:8081/health/live | jq .

# Check workers
curl -s http://localhost:8080/api/v1/workers | jq .
```

---

## Quick Reference

```bash
# ── Start everything ──
just controller                    # terminal 1
just worker worker-1               # terminal 2
just worker worker-2               # terminal 3

# ── Single job (quick test) ──
just submit-auto "echo hello"

# ── Multi-stage pipeline ──
just reserve my-repo "build,test"
just run-stage <GROUP_ID> build
just run-stage <GROUP_ID> test
just status <GROUP_ID>

# ── Observe ──
just logs-group <GROUP_ID>
just logs-stage <GROUP_ID> build
curl localhost:8080/metrics
```
