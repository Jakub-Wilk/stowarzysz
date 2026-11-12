# stowarzysz backend (Rust)

The v2 backend: a Cargo workspace that replaces the Django app in `../backend/` at cutover (it is
renamed to `backend/` then). See the port plan for the architecture and the phase list.

```
crates/domain   pure business rules (no I/O, no async)
crates/app      use cases, persistence, integrations, configuration
crates/server   HTTP API, SSE, CLI (`stowarzysz`)
migrations/     sqlx migrations (embedded, run at startup)
```

## Commands (from `backend-rs/`)

```sh
cp .env.example .env                       # APP_ENV=dev: development defaults for every secret
cargo run                                  # = cargo run -- serve; API on :8000
cargo run -- healthcheck                   # exit 0 if /api/health answers
cargo test --workspace
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo deny check                           # licenses/advisories (cargo install cargo-deny)
```

- OpenAPI document: `GET /api/openapi.json`; interactive docs: `GET /api/docs`.
- Every error is an RFC 9457 `application/problem+json` document with a Polish `detail` and a
  stable `code`.
- Configuration is read from the environment (and `.env`); every problem is reported at once on
  startup. Production (no `APP_ENV`) refuses to start without the secrets.
