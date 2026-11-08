# Structured logging standard

The extracted emitter contract is implemented in
`crates/nntune-logging/src/lib.rs`; the MB runtime wires it in
`workers/mb/src/lib.rs`. Existing wire worker labels (`api`, `ingest`,
`mb`, `stream`) are preserved for compatibility, not proof that other
workers are exported or deployed.

Records contain `ts`, `level`, `worker`, `trace_id`, `event` and `fields`.
The emitter also renders the normalized `build_sha` field: exactly 40
lowercase hexadecimal characters, or `unknown` for absent/invalid input.
The timestamp is caller-supplied Unix milliseconds, formatted as UTC.
Levels are debug/info/warn/error. Fields are flat primitives, never nested
request or response bodies.

`x-trace-id` propagation accepts only 32 lowercase hex characters. An
absent or invalid inbound value is replaced, not logged. Valid IDs survive
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
