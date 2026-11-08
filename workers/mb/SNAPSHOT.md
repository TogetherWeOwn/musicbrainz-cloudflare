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
8 MiB and parts at 256 MiB. Lookup buckets use the first four MBID hex
digits. Search keys use FNV-1a buckets and the pinned folding table.

The release search experiment fetches at most four word keys, retains
2,000 word postings and bounded title postings. These limits are not proof
of exhaustive candidate completeness or official Lucene semantics. Its
scores are not official WS/2 relevance. Artist/recording search and the
SQL adapter's relationship/media routes are not implemented here.

Only `MB_BACKEND=r2snapshot` selects this adapter. It would require an
explicit `MB_SNAPSHOT` binding and `MB_SNAPSHOT_PREFIX`; neither is active.
The native fake-store tests exercise index, range/decompression bounds,
folding, lookup and search behavior without an R2 account or real dump.
See [ops](../../docs/ops.md) before any future environment is introduced.
