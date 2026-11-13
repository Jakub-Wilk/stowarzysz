# Load testing

`crates/server/examples/load.rs` drives a running server: HTTP throughput and latency per
endpoint, Server-Sent Events fan-out, and concurrent large uploads. Run it against a release
build with production-like data.

## Setup

1. Import a backup into a scratch database (see `import-django` in the README), then create an
   administrator to sign in as: `stowarzysz create-superuser --username perf`.
2. Optionally multiply the data so slow paths show up.
3. Start the server: `cargo run --release -- serve` with `BIND_ADDR=127.0.0.1:8099`, `RUST_LOG=warn`,
   and `LOGIN_RATE` / `UPLOAD_RATE` raised so the limiters do not get in the way.

```sh
LOAD="cargo run --release --example load -- --base http://127.0.0.1:8099 --user perf --password ..."
$LOAD http --concurrency 64 --seconds 8 /api/polls /api/ledger/entries /api/ledger/balances
$LOAD sse --clients 1000 --events 20 --poll <id of a closed poll>
$LOAD upload --file big.png --concurrency 4
```

On Windows Git Bash, set `MSYS_NO_PATHCONV=1`, or it rewrites `/api/...` into file paths. Measure
the server's memory from outside (`PeakWorkingSet64` on Windows, `VmHWM` in `/proc/<pid>/status`
on Linux).

## Results (November 2026, before cutover)

These were measured on a Ryzen 7 5800X3D with 16 threads and 32 GB, on Windows, with Postgres 17
in Docker Desktop. The dataset was the November 2026 backup multiplied about 100 times: 23.8k
ledger entries, 50.8k obligations, 1.1k polls and 850 pacts.

Docker Desktop's port forwarding costs about 1 ms per database round trip, and it is the
bottleneck. At about 4,500 requests per second on `/api/health`, which runs one `select 1`, the
server uses 1.7 cores, about 0.4 ms of CPU per request. Absolute throughput here is therefore a
floor; on Linux, with the database on the same Docker network, expect much more.

**One request at a time, median latency:**

| Endpoint | Median |
|---|---|
| health | 1.4 ms |
| polls list | 5 ms |
| a ledger page (first or deep cursor) | 10 ms |
| stats for a month | 6 ms |
| admin user list | 21 ms |
| balances | 65 ms |
| all-time stats | 275 ms |

The last two aggregate the whole history, so they grow linearly with it. At the real ledger's
size (about 1% of this dataset) they take about 1 ms and 3 ms.

**64 connections at once** (with a 20-connection pool): the list and detail endpoints served
500 to 1,100 requests per second, with medians of 55 to 120 ms. That time is mostly spent queueing
for a database connection. Errors: none.

**Server-Sent Events:** 1,000 streams were open and ready in 1.6 s. Twenty broadcasts reached all
1,000 clients, 20,000 of 20,000 delivered. Delivery latency was 18 ms at the median and 40 ms at
p99. Each stream costs one authentication query, then nothing per event: they share one in-memory
hub.

**Uploads:** a 50 MB PNG is the worst case, random noise of about 16 megapixels. Each one in
processing peaks at about 400 MB. The peak is about 400 MB × `IMAGE_CONCURRENCY` (default 2,
measured at 407 MB and 792 MB), however many uploads arrive at once; the rest wait their turn.
Memory returns to about 20 MB afterwards, and nothing is left in `MEDIA_ROOT/.tmp`. Under the HTTP
load alone the server peaked at 90 MB.

## What the measurements changed

- **Balances** used to sort every obligation to pick each goods item's label, which spilled to
  temporary files. Now it does one hash aggregate per pair of people and fetches the labels
  separately from goods rows only. Migration 0009 adds a narrow index, so reading the entries'
  status skips the wide entry rows. 255 ms → 65 ms at 100× size.
- **Stats** evaluated two correlated subqueries per entry twice. Now each entry's total is looked
  up once through a `LATERAL` join: for a month that is a few hundred index lookups, 0.5 ms.
- **Generic plans.** sqlx reuses prepared statements, and Postgres soon plans them generically,
  without knowing the parameters. A filter written `$1 is null or col >= $1` then cannot use an
  index. Stats now pass concrete date bounds, and every keyset list compares against a real
  cursor (`Cursor::start()` for the first page). Deep pages are as fast as the first, and stats
  for a month went from 310 to 740 requests per second under load.
- **`IMAGE_CONCURRENCY`** makes the upload memory budget configurable. It was fixed at 2; use 1 on
  a small server.

`tests/failures.rs` checks that a failure partway through a change leaves nothing behind. It
injects the failure with a trigger, after the service has consumed an upload, queued a file for
deletion, published an event and queued a push. In every case no event is delivered, no push is
queued, and no file is lost or orphaned.
