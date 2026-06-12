# Agent Instructions

This repository is a generic Rust Axum React starter. Keep it platform-agnostic and reusable.

## Architecture

- Keep `crates/domain` pure business logic. It must not depend on Axum, SQLx, Redis, Tokio, tracing, GraphQL, HTTP clients, environment variables, or filesystem/runtime side effects.
- Put framework adapters in the root app modules:
  - `src/web.rs`: Axum routing and HTTP surface.
  - `src/graphql.rs`: GraphQL schema and request mapping.
  - `src/db.rs`: SQLx migrations, pools, and seed data.
  - `src/jobs.rs`: worker runtime and job adapters.
  - `src/security.rs`: HTTP middleware and headers.
- Prefer small functions that return typed data or `Result<T>` over hidden global state.
- Do not add brand-specific copy, domains, tokens, or platform internals to seed data, docs, examples, logs, user agents, or UI text.

## Security

- Treat every ID from clients as untrusted. Check authorization first, then resource existence, and return generic `not found` or `not authorized` errors.
- Do not expose SQL, Redis, network, filesystem, or backtrace details through API responses.
- Monitor URLs must stay protected against SSRF by default. Private/internal targets should require an explicit environment switch.
- Keep CSP, CORS, host, iframe, cookie, and proxy behavior configurable by environment variables.

## Testing And Checks

- Run `just check` before handoff.
- Add unit tests in `crates/domain` for pure business rules.
- Add integration tests in `tests/` for HTTP, GraphQL, database, Redis, jobs, and security behavior.
- Tests should avoid real network calls unless an explicit opt-in environment variable is set.
- Keep generated directories out of the repository: `target`, `frontend/node_modules`, and `frontend/dist`.
