# Chola CI

A distributed CI/CD build orchestrator for on-premises deployments. Workers reserve resources, execute multi-stage pipelines with pre/post scripts, and stream logs in real-time.

## Architecture

```
┌─────────────┐     gRPC      ┌─────────────┐     sh -c      ┌───────────┐
│  Controller  │◄────────────►│   Worker(s)  │──────────────►│  Builds   │
│  (Rust)     │               │   (Rust)     │               │           │
└──────┬──────┘               └──────────────┘               └───────────┘
       │ REST API
┌──────┴──────┐    ┌──────────┐    ┌───────┐
│  Dashboard  │    │ Postgres │    │ Redis │
│  (React)    │    └──────────┘    └───────┘
└─────────────┘
```

**Controller** — schedules jobs, manages workers, serves REST API + gRPC  
**Worker** — executes build commands, streams logs, reports status  
**ci-job-runner** — CLI for Jenkins/scripts to reserve workers and run stages  
**Dashboard** — React frontend for builds, workers, repos, settings

## Quick Start

```bash
# Prerequisites: Rust, PostgreSQL, Redis, Node.js 18+

# Database
psql -U postgres -c "CREATE DATABASE choladb; CREATE USER chola_app WITH PASSWORD 'secret'; GRANT CONNECT, CREATE ON DATABASE choladb TO chola_app;"

# Start services
just watch-controller    # Terminal 1
just watch-worker worker-1  # Terminal 2
just frontend-dev        # Terminal 3 → http://localhost:3000
```

## Build Pipeline

```bash
# Reserve a worker for a multi-stage build
just reserve my-repo abc123 "build,test"

# Run stages sequentially
just run-stage <GROUP_ID> build
just run-stage <GROUP_ID> test
```

## Features

- **Multi-stage pipelines** with DAG dependencies
- **Pre/post scripts** — global (per-repo) and per-stage, worker or controller scope
- **Resource-based scheduling** — CPU/memory/disk allocation, least-loaded worker selection
- **Worker management** — register, drain, delete, label groups, token-based auth
- **Live log streaming** via gRPC + SSE, persisted to disk
- **Reservation timeouts** — idle and stall detection with automatic cleanup
- **Workspace isolation** — per-build directories with git bare mirror caching
- **Web dashboard** — builds, workers, repos, stages, scripts, tokens, settings, analytics
- **Security** — JWT + API keys + per-worker tokens, secret masking in logs
- **Webhooks** — GitHub/GitLab triggers with HMAC verification

## Auth

```bash
# Worker: register in dashboard → get token
token: "chola_wkr_xxx"   # in worker config
# or
CHOLA_TOKEN=chola_wkr_xxx  # env var

# Runner: create at dashboard → Tokens → Runner tab
CHOLA_TOKEN=chola_svc_xxx just reserve ...
```

## Tech Stack

- **Backend:** Rust, tonic (gRPC), axum (REST), sqlx (PostgreSQL), deadpool-redis
- **Frontend:** React 19, TypeScript, Vite, TailwindCSS, TanStack Query, Zustand
- **Infra:** PostgreSQL, Redis, Vector (log shipping), Nix (builds)

## Configuration

See [`config/controller.example.yaml`](config/controller.example.yaml) and [`config/worker-1.example.yaml`](config/worker-1.example.yaml).

Data directory: `$CHOLA_HOME` → `$XDG_DATA_HOME/chola` → `~/.local/share/chola`

## License

Apache License 2.0. See [LICENSE](LICENSE).
