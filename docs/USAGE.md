# Chola CI — Usage Guide

> Example commands for every workflow. All commands assume you're in the project root.

---

## Prerequisites

```bash
# Start infrastructure
# PostgreSQL + Redis must be running (migrations run automatically on controller startup)

# Build all binaries
just build
# Or via nix:
nix build .#ci-controller
nix build .#ci-worker
nix build .#ci-job-runner
```

---

## Running via Nix

```bash
# Controller
nix run .#ci-controller -- --config config/controller.example.yaml

# Worker
CHOLA_TOKEN=chola_wkr_xxx nix run .#ci-worker -- --config config/worker-1.example.yaml

# ci-job-runner — reserve + run stage
CHOLA_TOKEN=chola_svc_xxx nix run .#ci-job-runner -- -C http://localhost:50051 \
    reserve --repo euler-api-txns --commit abc123 --stages gitleaks

CHOLA_TOKEN=chola_svc_xxx nix run .#ci-job-runner -- -C http://localhost:50051 \
    run --job-group-id <GROUP_ID> --stage gitleaks

# Remote (from GitHub, pinned commit)
nix run github:subanesh-swe/chola/<COMMIT>#ci-job-runner -- \
    -C http://controller:50051 reserve --repo my-repo --commit abc123 --stages build,test
```

---

## Environment Variables

Database connection fields can be overridden via env vars. Env wins over config file. The controller logs the source of each field on startup.

| Env Var | Config Field | Description |
|---------|-------------|-------------|
| `CHOLA_DB_HOST` | `storage.postgres.host` | Database host |
| `CHOLA_DB_PORT` | `storage.postgres.port` | Database port (default: 5432) |
| `CHOLA_DB_NAME` | `storage.postgres.database` | Database name |
| `CHOLA_DB_USER` | `storage.postgres.user` | Database user |
| `CHOLA_DB_PASSWORD` | `storage.postgres.password` | Database password |
| `CHOLA_REDIS_HOST` | `redis.host` | Redis host |
| `CHOLA_REDIS_PORT` | `redis.port` | Redis port (default: 6379) |
| `CHOLA_REDIS_PASSWORD` | `redis.password` | Redis password |

```bash
# Example: override credentials for production
export CHOLA_DB_HOST=db.prod.internal
export CHOLA_DB_USER=chola_app
export CHOLA_DB_PASSWORD=real_secret
export CHOLA_REDIS_HOST=redis.prod.internal
export CHOLA_REDIS_PASSWORD=redis_secret
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

## 11. Web Dashboard

The web dashboard provides a UI for viewing builds, workers, repos, and managing users.

### Prerequisites

```bash
# Install frontend dependencies
just frontend-install

# Start the controller (must be running for API proxy)
just controller
```

### Development Mode

```bash
# Start Vite dev server (proxies /api/* to controller:8080)
just frontend-dev
# Opens at http://localhost:3000
```

### Production Build

```bash
just frontend-build
# Output: frontend/dist/ (served by controller via axum)
```

### Default Login

When `auth.enabled = true` in controller config, a default admin user is seeded on startup:

| Field | Value |
|-------|-------|
| Username | `admin` |
| Password | `changeme` |
| Role | `super_admin` |

Change these in `config/controller.example.yaml` under `auth:`.

### User Roles

| Role | Permissions |
|------|-------------|
| `super_admin` | Everything + user management |
| `admin` | Manage repos, cancel jobs, manage workers |
| `operator` | Trigger builds, view all |
| `viewer` | Read-only access |

---

## 12. REST API (Dashboard Backend)

All endpoints under `/api/v1/`. Protected endpoints require `Authorization: Bearer <token>`.

### Authentication

```bash
# Login — returns JWT token
curl -s -X POST http://localhost:8080/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -d '{"username":"admin","password":"changeme"}' | jq .

# Response:
# { "token": "eyJ...", "expires_at": "...", "user": { "id", "username", "role" } }

# Use token for subsequent requests
TOKEN="eyJ..."

# Get current user
curl -s http://localhost:8080/api/v1/auth/me \
    -H "Authorization: Bearer $TOKEN" | jq .
```

### Dashboard

```bash
# Summary stats
curl -s http://localhost:8080/api/v1/dashboard/summary \
    -H "Authorization: Bearer $TOKEN" | jq .
```

### Repos

```bash
# List repos
curl -s http://localhost:8080/api/v1/repos \
    -H "Authorization: Bearer $TOKEN" | jq .

# Create repo (admin+)
curl -s -X POST http://localhost:8080/api/v1/repos \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Content-Type: application/json' \
    -d '{"repo_name":"my-service","repo_url":"git@github.com:org/my-service.git"}' | jq .

# List stage configs for a repo
curl -s http://localhost:8080/api/v1/repos/<REPO_ID>/stages \
    -H "Authorization: Bearer $TOKEN" | jq .
```

### Builds (Job Groups)

```bash
# List builds (paginated, filterable)
curl -s "http://localhost:8080/api/v1/job-groups?limit=20&offset=0&state=running" \
    -H "Authorization: Bearer $TOKEN" | jq .

# Get build detail with all stages
curl -s http://localhost:8080/api/v1/job-groups/<GROUP_ID> \
    -H "Authorization: Bearer $TOKEN" | jq .

# Cancel build (admin+)
curl -s -X POST http://localhost:8080/api/v1/job-groups/<GROUP_ID>/cancel \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Content-Type: application/json' \
    -d '{"reason":"Cancelled from API"}' | jq .
```

### Workers

```bash
# List workers
curl -s http://localhost:8080/api/v1/workers \
    -H "Authorization: Bearer $TOKEN" | jq .

# Drain worker (admin+)
curl -s -X POST http://localhost:8080/api/v1/workers/<WORKER_ID>/drain \
    -H "Authorization: Bearer $TOKEN" | jq .

# Undrain worker
curl -s -X POST http://localhost:8080/api/v1/workers/<WORKER_ID>/undrain \
    -H "Authorization: Bearer $TOKEN" | jq .
```

### Users (super_admin only)

```bash
# List users
curl -s http://localhost:8080/api/v1/users \
    -H "Authorization: Bearer $TOKEN" | jq .

# Create user
curl -s -X POST http://localhost:8080/api/v1/users \
    -H "Authorization: Bearer $TOKEN" \
    -H 'Content-Type: application/json' \
    -d '{"username":"dev1","password":"secret","display_name":"Developer 1","role":"operator"}' | jq .
```

### Live Log Streaming (SSE)

```bash
# Stream logs for a running job (Server-Sent Events)
curl -N http://localhost:8080/api/v1/jobs/<JOB_ID>/logs/stream \
    -H "Authorization: Bearer $TOKEN"

# Get accumulated logs (paginated)
curl -s http://localhost:8080/api/v1/jobs/<JOB_ID>/logs \
    -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Quick Reference

```bash
# ── Start everything ──
just controller                    # terminal 1 (gRPC:50051 + HTTP:8080)
just worker worker-1               # terminal 2
just worker worker-2               # terminal 3
just frontend-dev                  # terminal 4 (http://localhost:3000)

# ── Single job (quick test) ──
just submit-auto "echo hello"

# ── Multi-stage pipeline ──
just reserve my-repo "build,test"
just run-stage <GROUP_ID> build
just run-stage <GROUP_ID> test
just status <GROUP_ID>

# ── Dashboard ──
just login admin changeme          # get JWT token
just dashboard <TOKEN>             # summary stats
just api-builds <TOKEN>            # list builds
just api-workers <TOKEN>           # list workers

# ── Observe ──
just logs-group <GROUP_ID>
just logs-stage <GROUP_ID> build
curl localhost:8080/metrics
```
