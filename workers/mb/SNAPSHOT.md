# Retained snapshot runtime experiment

This extraction retains `src/snapshot.rs` and `src/snapshot_fold.tsv`
with their native synthetic tests. It does not adopt the spike or claim
WS/2, complete ordered media, truthful exhaustive search, coherent data
promotion or operational qualification. Source build/evaluation examples
and live-dump measurement scripts stay in the source repository for a
separately reviewed importer/backend package; no generated dump golden
set or deployed benchmark configuration is published here.

## Minimum runtime format

A prefix identifies immutable objects. Each kind has a fixed 65,536-entry
index: 16 little-endian bytes per entry (`part u32`, `len u32`, `offset u64`).
A zero-length entry is empty. Data parts concatenate raw-DEFLATE blocks
of sorted `key<TAB>JSON` lines. The runtime caps decompressed blocks at
8 MiB and parts at 256 MiB. Compressed ranges are checked before I/O,
with a separate conservative cap of 16 MiB + 1 KiB for the pinned level-6
miniz encoder's static/stored fallback and headers; incompressible DEFLATE
can exceed the plaintext length. The 8 MiB inflated ceiling is unchanged.
Lookup buckets use the first four MBID hex digits. Search keys use FNV-1a
buckets, NFC canonical composition and the pinned folding table. Equivalent
composed/decomposed queries and titles use the same keys and scores. Future
importer integration must reuse these helpers and the validated repository
interfaces; old separately generated keys are not silently assumed compatible.

`SnapshotRepo::new` is fallible and rejects empty prefixes. Its source and
prefix cannot be replaced after construction; `RangeSource` implementations
must keep their objects immutable for the repository lifetime. The cache
belongs to that repository, not the thread or an unverified store label.
It retains at most two 1 MiB indexes, evicts FIFO and releases entries on
drop. Each HTTP spike request creates its own repository, so no index is
retained across requests. Same-repository reads still reuse indexes.

`RangeSource::read_all_exact` requires metadata validation before consuming
an index body. R2 checks the exact index size, validates part/range metadata,
and consumes body streams with an exact retained-byte bound rather than
buffering an unchecked object. Post-read equality checks remain defensive
controls for faulty source implementations.

The release search experiment fetches at most four word keys, rejects a
stored list beyond 2,000 word postings or 1,025 title postings during
bounded deserialization, and validates primary/related UUIDs before use.
Case-insensitive identity deduplication precedes sorting and pagination;
corrupt lists fail as a whole, never silently truncate. Lookup rows also
validate non-null related UUIDs. These limits are not proof
of exhaustive candidate completeness or official Lucene semantics. Its
scores are not official WS/2 relevance. Artist/recording search and the
SQL adapter's relationship/media routes are not implemented here. Direct
unsupported repository calls return an explicit query-stage error, not a
false not-found result. The router classifies unknown paths before acquiring
snapshot bindings, requires a full `/__mb-snap` prefix segment, and renders
known-route configuration failures in the JSON internal-error envelope.
Snapshot responses reuse the shared trace resolver/header and a sanitized
`mb.snapshot.completed` event containing only status and normalized build
identity. Entropy failure returns the fixed 503 before auth/repository/log
work. The completion sink never includes query, row, path or bench-secret
values and remains subject to final Security review.

Only `MB_BACKEND=r2snapshot` selects this adapter. It would require an
explicit `MB_SNAPSHOT` binding and `MB_SNAPSHOT_PREFIX`; neither is active.
The native fake-store tests exercise index, range/decompression bounds,
folding, lookup and search behavior without an R2 account or real dump.
See [ops](../../docs/ops.md) before any future environment is introduced.
