# Workspace supply-chain review

The initial workspace reuses the pinned worker Cargo.lock verbatim at the
root. No new Rust dependency/version/checksum is resolved by extraction.
All registry package versions and integrity checksums are recorded there;
the only path dependency is the exported `crates/nntune-logging`.
No root npm/pnpm/Python install, Docker build or package publication is
required. The example Wrangler config has no install/build hook.

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
