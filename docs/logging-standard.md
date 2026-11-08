# Structured logging standard

The extracted emitter contract is implemented in
`crates/nntune-logging/src/lib.rs`; the MB runtime wires it in
`workers/mb/src/lib.rs`. Existing wire worker labels (`api`, `ingest`,
`mb`, `stream`) are preserved for compatibility, not proof that other
workers are exported or deployed.

Records contain `ts`, `level`, `worker`, `trace_id`, `event` and `fields`.
The MB runtime normalizes `fields.build_sha` before construction: exactly
40 lowercase hexadecimal characters, or `unknown` for absent/invalid input.
The generic `LogRecord` constructor validates fields but does not add or
normalize build identity; callers use `parse_build_sha` for that obligation.
The timestamp is caller-supplied unsigned Unix milliseconds, formatted as
UTC with millisecond precision. `LogRecord::new` accepts only
`0..=253402300799999` (1970-01-01 through 9999-12-31 inclusive) and returns
a non-sensitive error above that domain, without clamping or wrapping.
The direct `unix_millis_to_rfc3339(u64) -> String` helper has the same
supported domain but no error channel; out-of-domain RFC3339 output is not
promised, and pre-1970 dates cannot be represented by the unsigned API.
Levels are debug/info/warn/error. Fields are flat primitives, never nested
request or response bodies.

`x-trace-id` propagation accepts only 32 lowercase hex characters. An
absent or invalid inbound value is replaced with a random ID, not logged.
If randomness is unavailable, trace resolution returns an explicit error;
MB responds with a fixed 503 before cache/repository work, without inventing
or echoing a trace. Valid inbound IDs do not require randomness and survive
response and repository-failure paths. Build identity is not authorization.

Validate secrets and personal data at construction. Credential-shaped
values, personal-data keys, email/IP-shaped values and nested structures
are rejected. Caller-side redaction uses `[REDACTED]`; rejected values must
not be echoed by error paths. Never log authorization/cookie headers,
connection strings, raw SQL, user input bodies or data from a live dump.
`mb.db.failed` uses sanitized stage labels instead of driver messages.

The emitter's retained source tests and independent MB logging fixture
pin the envelope, trace validation, build-SHA normalization and rejection
behavior. The JSON fixture is synthetic, not a production tail capture.
These guards do not establish that arbitrary caller-selected strings are
safe; security review still covers every new log/artifact sink.
