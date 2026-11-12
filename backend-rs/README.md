# stowarzysz backend (Rust)

The v2 backend: a Cargo workspace that replaces the Django app in `../backend/` at cutover (it is
renamed to `backend/` then). See the port plan for the architecture and the phase list.

```
crates/domain          pure business rules (no I/O, no async)
crates/app             use cases, persistence, events, push, scheduler, configuration
crates/server          HTTP API, SSE, CLI (`stowarzysz`)
migrations/            sqlx migrations (embedded, applied on startup)
.sqlx/                 offline query metadata (commit it; see below)
fixtures/differential/ golden vectors generated from the Python code
```

## Setup

```sh
docker compose up -d db                    # from the repo root; Postgres on localhost:5434
cp .env.example .env                       # APP_ENV=dev: development defaults for every secret
cargo install sqlx-cli --no-default-features --features postgres,rustls
sqlx database create                       # creates stowarzysz2 (migrations run on startup)
```

## Commands (from `backend-rs/`)

```sh
cargo run                                  # = serve: migrate, then API on :8000 + background workers
cargo run -- migrate                       # apply migrations and exit
cargo run -- run-jobs                      # run every background job once (and deliver pushes)
cargo run -- healthcheck                   # exit 0 if /api/health answers (container probe)
cargo test --workspace                     # needs the dev database: one fresh DB per test
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo sqlx prepare --workspace             # after changing a query or a migration; commit .sqlx/
cargo deny check                           # licenses/advisories (cargo install cargo-deny)
uv run --project ../backend python fixtures/differential/generate.py   # regenerate fixtures
```

## How it fits together

- **API:** `GET /api/openapi.json` is the contract; `GET /api/docs` browses it. Every error is an
  RFC 9457 `application/problem+json` document with a Polish `detail` and a stable `code`.
- **Queries** are checked at compile time against the dev database (`sqlx::query!`); builds
  without a database (Docker, CI) use the committed `.sqlx/` metadata (`SQLX_OFFLINE=true`).
- **Side effects are transactional.** A use case runs in a `db::Uow`: SSE events are
  `pg_notify` calls (delivered only on commit, to every process), push notifications are
  `push_outbox` rows (delivered by the push worker with retries), deleted files are
  `media_trash` rows. Nothing is lost if the process dies between commit and delivery.
- **Events** reach clients through one `LISTEN` connection per process feeding an in-process
  hub; each SSE stream filters by audience and is told to `resync` if it may have missed any.
- **Background work** runs in-process (no cron): the scheduler sleeps until the next job is
  due; periodic jobs and due rows are claimed with `FOR UPDATE SKIP LOCKED`, so several
  processes never do the same work twice.
- **Time** comes from `app::clock::Clock` (`FixedClock` in tests) for every business decision.
- **Configuration** is read from the environment (and `.env`); every problem is reported at once
  on startup. Production (no `APP_ENV`) refuses to start without the secrets.
