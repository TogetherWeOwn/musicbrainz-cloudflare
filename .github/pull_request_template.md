<!--
Title: Conventional Commits header, e.g. `fix(auth): refuse expired sudo sessions`.
Types: feat fix perf refactor test docs build ci chore revert style security. ≤100 chars, no trailing period.
In every repo, public or private: no internal ticket ids, instance links, localhost or private-network
addresses in title, body, commits or branch. Link only public GitHub issues (`Fixes #123`); link the card
from the tracker side. The required pr-standards check enforces this.
Rules: https://github.com/TogetherWeOwn/.github/blob/main/CONTRIBUTING.md
Open as draft; mark ready when complete. Batch fixes per review round.
-->

## Thinking Path

<!-- 3–6 bullets: context → subsystem → problem → why this change. -->

## Linked Issues or Issue Description

<!-- Required for feat/fix/perf/refactor/security: `Fixes #123` / `Refs #123`, or describe the problem here. -->

-

## What Changed

<!-- One bullet per logical unit; don't repeat the diff. -->

-

## Verification

<!-- Only what you actually ran and observed: exact commands + results. Say what you did not run and why. -->

-

## Attestation

<!-- Mandatory, machine-checked. Syntax must stay exact; tier rules: CONTRIBUTING.md §5. -->

risk_tier: <T0|T1|T2>

### Claims

- C1: <behavior guaranteed by this change>

### Tests

- C1 | test=<path/to/test_file>::<test_name> | failed_before=not-run | proof=<command and observed result>

### High-risk checklist

- [ ] D1: <secret sinks, redaction and log/artifact exposure>
- [ ] D2: <authentication, authorization and cross-principal/oracle cases>
- [ ] D3: <migrations, grants, restore, erasure and state integrity>
- [ ] D4: <input bounds, query limits, retries and resource caps>
- [ ] D5: <concurrency, lost writes, replay and single-use effects>
- [ ] D6: <pinned dependencies/actions and test/guard wiring>
- [ ] D7: <failure propagation, non-empty evidence and honest verdicts>
- [ ] D8: <malformed shapes, Unicode, parsing and host validation>

## Risks

<!-- Blast radius, rollback, follow-ups. "Low" is fine if true. -->

## Model Used

<!-- Provider + exact model/version that produced or assisted, or "None — human-authored". -->

## Checklist

- [ ] I searched existing issues/PRs; duplicates and related PRs are linked above.
- [ ] Verification records only tests actually run, with results.
- [ ] Attestation describes actual evidence, including tests not run before the change.
- [ ] No secrets or private/internal identifiers in title, body, commits or branch name.
- [ ] Every automated-review finding is answered in its own thread, first line `Fixed in <sha>` / `Rejected: <proof>` / `Duplicate of <link>`.
