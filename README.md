# Rust Axum React Starter Kit

Production-like Rust full-stack starter for containerized development environments.

```bash
docker compose up --build
```

The default development topology is intentionally split:

- `frontend`: Vite React TypeScript with HMR, exposed on port 5173.
- `web`: Axum API, GraphQL, security middleware, hot reloaded by `cargo watch`.
- `worker`: Redis-backed background jobs and recurring monitor checks.
- `setup`: runs SQLx migrations and seeds demo data.
- `postgres`: low-memory PostgreSQL tuned for many apps per host.
- `redis`: cache, sessions, rate limits, and jobs.

Hot reload defaults are optimized for development feedback loops:

- Vite uses native file watching by default and can switch to polling with `VITE_USE_POLLING=true` for remote filesystems.
- Backend watchers only watch Rust sources, workspace crates, manifests, and migrations.
- The web watcher uses a shorter debounce than the worker so API reloads win the shared Cargo target lock.
- SQLx migrations are loaded from disk at setup time so new migration files work without a stale embedded migrator.
- Worker updates are published through Redis pub/sub and forwarded to `/api/events`, so the React UI refreshes job and monitor state without manual reloads.

## Demo App

The included Uptime Console demonstrates batteries that matter in production:

- GraphQL resource API
- sessions and role-based authorization
- Redis-backed caching and rate limiting
- recurring background checks
- maintenance tasks
- CSP, host, CORS, proxy, and iframe controls via ENV

Demo credentials:

- email: `admin@example.com`
- password: `password`

## Development Quality

The root app owns framework adapters for Axum, SQLx, Redis, GraphQL, jobs, and HTTP security. Pure business rules live in `crates/domain` and are kept free of runtime, database, network, and framework dependencies by `scripts/check-architecture`.

Use one quality gate:

```bash
just check
```

It runs architecture checks, rustfmt, clippy, Rust tests, TypeScript typecheck, Biome, frontend build, dependency audits when the tools are installed, and Docker Compose config validation. Database-backed integration tests are opt-in locally with `RUN_DATABASE_TESTS=1`.

## Security Defaults

The template defaults to iframe-friendly embedding for development:

- `APP_ALLOWED_HOSTS=*`
- `APP_CORS_ALLOWED_ORIGINS=*`
- no `X-Frame-Options`
- `APP_CSP_MODE=off` in development

Tighten with ENV:

```bash
APP_ALLOWED_HOSTS=app.example.com,preview.example.com
APP_FRAME_ANCESTORS="'self' https://*.example.com"
APP_CSP_MODE=enforce
APP_CSP_CONNECT_SRC="'self' https://api.example.com"
APP_CORS_ALLOWED_ORIGINS=https://app.example.com
APP_COOKIE_SAMESITE=none
APP_COOKIE_SECURE=true
```

Monitor checks reject private, loopback, link-local, and localhost targets by default to avoid SSRF-style mistakes. Enable `APP_ALLOW_PRIVATE_MONITOR_URLS=true` only when the app is deployed on a trusted private network and internal checks are intended.

## Fibe And ENV Overrides

The Compose file intentionally uses two variable layers:

- Fibe template variables such as `$$var__app_secret` are resolved before the playground is imported. Use these for launch-time values that must be generated or stored with the template, such as secrets and selected branches.
- Compose ENV interpolation such as `${APP_PORT:-3000}` is resolved by Docker Compose at runtime. If the variable is absent, the value after `:-` is used.

In Fibe playgrounds, `fibe.gg/env_file: env.example` tells Fibe where dynamic services should read default environment values from. After launch, users can override service environment variables from the playground UI and recreate the app to apply them. Keep launch-time template variables small and intentional; keep operational tuning and security hardening switches in ENV so they can be changed later without forking the template.

`APP_PORT` and `VITE_API_ORIGIN` should be changed together. The default Fibe topology exposes the `frontend` service and keeps the Axum `web` service internal on port 3000.

## Local Commands

```bash
cp env.example .env
docker compose up --build
just check
```
