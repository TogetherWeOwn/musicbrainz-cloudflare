# Workspace supply-chain review

The initial workspace reuses the pinned worker Cargo.lock verbatim at the
root. No new registry package/version/checksum is resolved by extraction.
The later normalization repair adds one direct edge to the already-locked
`unicode-normalization =0.1.25` with default features disabled; it does not
update any registry package or checksum.
All registry package versions and integrity checksums are recorded there;
the only path dependency is the exported `crates/nntune-logging`.
No root npm/pnpm/Python install, Docker build or package publication is
required. The example Wrangler config has no install/build hook.

## Normalization dependency review

Before the direct-edge change, the exact registry artifacts were inspected
without installation or execution (2026-11-08):

| Package | SHA-256, matching Cargo.lock | Executable build hooks |
| --- | --- | --- |
| unicode-normalization 0.1.25 | `5fd4f6878c9cb28d874b009da9e8d183b5abc80117c40bbd187a1fde336be6e8` | `build = false`, no build.rs or proc-macro library |
| tinyvec 1.13.3 | `fd3ca314f692efd6c868f8408f53fe444634a845f96c028b97d35f6a1f79f0ee` | `build = false`, no build.rs or proc-macro library |

The normalization crate exposes NFC iterators and uses tinyvec's `alloc`
feature, already needed by the locked graph. No new script exemption or
host install is introduced. Cargo 1.99.1 on the preparation host was only
queried for version; clean locked fetch/native tests/Wasm qualification
of the corrected graph remain exact-head GitHub Actions obligations. This
is artifact/hooks review, not a claim of exhaustive dependency-code audit.
The direct edge must be reconsidered if normalization is removed or either
locked package changes; it is not a blanket future-version exception.

## Controls and residual gaps

Stable Cargo has **no documented native release-age filter** and **no
blanket dependency build.rs/proc-macro refusal**. `--locked` prevents
unreviewed graph changes, not build-script execution or fresh-release
exposure. No made-up equivalent policy is claimed. Builds run in ephemeral
GitHub-hosted runners without production secrets, with read-only repository
permissions and credential-free checkouts. This isolation does not make
arbitrary dependencies trustworthy. Dependency changes require review of
the exact artifact, build scripts/macros, integrity and necessity before
installation; any exception must be narrow, attributed and time-bounded.

The locked graph includes compiler-executed code (notably serde derive,
wasm-bindgen and worker macros and their proc-macro dependencies). Their
inclusion is inherited, not a new permission to execute arbitrary build
hooks on the shared host. Clean `cargo fetch --locked`, native tests and
Wasm build evidence belong to the first Actions run; until it finishes,
clean-install/build acceptance is pending, not proved by the lockfile.
The GitHub-hosted job installs the minimal Rust toolchain plus rustfmt,
clippy and the Wasm target explicitly. No host package/global configuration
is changed by this repository.

Workflow downloads use literal action SHAs or versioned checksummed
release artifacts. The secret scan uses the free binary, not an org-paid
wrapper. No broad allowlist, gitleaks suppression, source build fallback
for Python, security-update delay or required-check waiver is introduced.

Dependabot configuration covers Cargo at root and GitHub Actions at root,
with an explicit three-day version-update cooldown. This does not delay
security updates. A future ecosystem adoption must apply that manager's
supported native age/script controls and verify them in Actions rather
than treating Cargo's residual gap as permission to omit available controls.
