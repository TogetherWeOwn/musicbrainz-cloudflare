# nntune-logging

Retained MIT structured-log emitter, build identity normalization and
trace-ID propagation helper. The package name is kept for compatible
source paths, not new registry publication.

The normative standalone contract is
[`docs/logging-standard.md`](../../docs/logging-standard.md). Native and
Wasm targets use the same rendering logic. `getrandom` uses Web Crypto
on Wasm and the OS RNG on native targets. Callers supply timestamps and
perform the actual output; this crate does not import a Worker runtime.

Actions runs the inline tests plus `tests/build_sha_normalization.rs` and
`tests/secret_value_shapes.rs`, and strict clippy across all targets.
Original worker labels remain in the shared contract; they do not imply
other product workers or consumers have migrated.
