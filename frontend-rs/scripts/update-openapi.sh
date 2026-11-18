#!/usr/bin/env bash
# Refresh crates/api/openapi.json (the API contract the frontend's types are generated from)
# from backend-rs. Needs no database. Commit the result together with the backend change;
# backend-rs's `openapi_snapshot_is_current` test fails while the two differ.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here/../backend-rs"
SQLX_OFFLINE=true cargo run --quiet -- openapi > "$here/crates/api/openapi.json"
echo "updated crates/api/openapi.json"
