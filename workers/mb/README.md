# MusicBrainz legacy read Worker

This is a source extraction, not a production deployment or WS/2 adoption.
Package and crate names retain `nntune-mb` / `nntune_mb` for source and test
compatibility. The required logging path is `../../crates/nntune-logging`.
The workspace lockfile is at the repository root.

## Existing routes

All existing data responses are custom JSON under `/v1/mb`, not official
MusicBrainz wire shapes. `GET /healthz` is binding-free liveness only, not
storage readiness or evidence that an importer has populated anything.

| Legacy GET path | SQL adapter | Snapshot experiment |
|---|---|---|
| `/v1/mb/artist/:mbid` | Artist lookup | Artist lookup |
| `/v1/mb/release/:mbid` | Release lookup | Release lookup |
| `/v1/mb/recording/:mbid` | Recording lookup | Recording lookup |
| `/v1/mb/search?q=&type=&limit=&offset=` | Trigram search for artist/release/recording | Approximate release-only search |
| `/v1/mb/release/:mbid/recordings` | Ordered recordings, capped at 1,000 | Not implemented |
| `/v1/mb/release-group/:mbid` | Release group | Not implemented |
| `/v1/mb/release-group/:mbid/releases` | Group releases, capped at 1,000 | Not implemented |
| `/v1/mb/artist/:mbid/urls` | Curated links | Not implemented |
| `/v1/mb/recording/:mbid/videos` | Stored video links | Not implemented |
| `/v1/mb/recording/:mbid/isrcs` | ISRCs | Not implemented |

Search defaults to 25 results, clamps larger limits to 25, rejects zero,
and accepts at most 100 query characters. Empty queries return
`MB_SEARCH_EMPTY_QUERY`. Offsets above 1,000 short-circuit to an empty
legacy success: this is a known incompatibility with truthful WS/2 count
and completeness requirements, not an adopted WS/2 policy. Scores are
legacy similarity values, not official search scores.

The SQL adapter expects an external schema and `MB_DB` Hyperdrive binding.
Neither a schema nor a database is provisioned here. Success responses use
`MB_CACHE` KV and the Cache API; 400/404/500 failures remain `no-store`.
No request is forwarded to the live MusicBrainz API.

## Deadline and error contracts

Each connect/query operation uses the shared database deadline
`DB_OPERATION_DEADLINE_MS`; timeout is a repository failure, not an empty
success. `mb.db.failed` emits a sanitized stage label (`connect`, `query`,
`decode` or `timeout`), not driver text, SQL or credentials. Public errors
use the generic registry envelope described in
[`docs/api-errors.md`](../../docs/api-errors.md).

Logs follow [`docs/logging-standard.md`](../../docs/logging-standard.md).
Valid `x-trace-id` values propagate; invalid input is replaced rather than
logged. `BUILD_SHA` is normalized to 40 lowercase hexadecimal characters
or the explicit `unknown` sentinel.

## Snapshot runtime contracts

`MB_BACKEND=r2snapshot` selects the retained experimental backend;
unset/other values select SQL. There is no configured destination runtime.
[`SNAPSHOT.md`](SNAPSHOT.md) documents the format and gaps. Source tests
for index bounds, decompression caps, deterministic folding, search and
fake object-store reads remain in `src/snapshot.rs`. Extraction does not
make this backend feature-complete or safe for production adoption.

## Retained replication model

`src/replication.rs` and its synthetic ledger/replay/conflict/SLO tests are
pure contracts, **not** an hourly downloader or applied SQL migration.
P95 source: the recorded per-packet publication-to-application lag
population in `PacketLedger`; fixture timestamps are synthetic. Breach
definition: `ReplicationSloProbe` reports lag over two hours and missing
sequences using its configured bounds. It emits
`mb_replication_slo_breach` structured diagnostics/metrics. This does not
prove an operational feed SLO, durable ledger, refresh cadence or licensed
data path. Production health and the future no-token/optional-hourly
modes require their own implementation and qualification.

## Configuration and tests

`wrangler.toml` is an intentionally unconfigured example with no resource
IDs, routes or automatic build/install command. It must not be deployed.
All environment surfaces disable workers.dev. Example art buckets are
unprovisioned labels; stored `caa_url` values are not rewritten or fetched.

Actions runs `cargo test --locked --workspace --all-targets`, strict clippy,
formatting and a Wasm release build. No live service is needed. Independent
worker tests are retained, including source-wiring assertions; the source
cross-worker API registry test remains in the source repository. See the
[coverage mapping](../../provenance/README.md).
