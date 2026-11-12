# Cargo dependency updates

The single `Update dependencies` workflow runs weekly on Monday at 04:23 UTC
and can be started manually. It uses the pinned Cargo to update `Cargo.lock` within the
existing manifest constraints, then audits and checks the result before creating
one commit as `github-actions[bot]`. It does not create PRs or change manifests,
flake inputs, configuration pins, repository settings or deployed systems.

Only this repository's default `master` branch can publish. Manual feature runs
are validation-only. Select `dry_run` on master to exercise verification without
committing or pushing, including when resolution finds no change. Ordinary
unchanged runs finish successfully without a commit or expensive checks.

Verification requires a fresh RustSec advisory database and a successful audit
without automatic ignores. The owning Nix package uses `cargoLock.lockFile` and
runs `scripts/check --release` with `sandbox = true` and
`sandbox-fallback = false`. If isolation is unavailable, the build fails. Checks
include formatting, Clippy, the full Rust suite (including FIFO/HTTP/golden
regressions), Python driver checks and finite updater-shell tests with real Git
repositories.
Updated crate build scripts, procedural macros, exporter binaries and tests run
only there.
No candidate benchmark or application binary runs on the outer runner.

The pinned checkout and Nix installer are trusted setup and may consume the
built-in token; checkout does not persist Git credentials. Installer-owned host
Nix configuration remains outside the candidate sandbox. No token or runner
credential/configuration is passed or mounted into the derivation. Resolution,
fresh auditing and Git handling use trusted tooling outside it. The final static
push step is the only explicit shell token environment. CI's active pre-commit
hook invokes the same cached sandboxed Nix check. Developer hooks still run
`scripts/check` locally. This isolation does not guarantee that every upstream dependency is
free of defects.

The whole tracked diff must be only the existing regular `Cargo.lock`, with no
mode change. Its tested checksum must still match before staging. Author and
committer use the official bot identity. Immediately before the ordinary
nonforce push, remote master must equal the recorded starting commit. Stale
source or a push race fails visibly; the job never rebases, merges or force-pushes.
Push receipts print the exact expected commit and observed remote identity. An
uncertain result must be reconciled against that commit, not retried blindly.
A failed audit/check or a dry-run cannot publish. GitHub token pushes do not start
normal push CI, so all verification finishes before the push.

Failures require diagnosis. A later bad lock is recovered with a checked revert,
not a forced reset; deployed consumer pins remain unchanged. Initial workflow
integration is a separate maintainer action. No auto-merge setting or additional
credential is required. The read-only `Rust` workflow also builds the same checked
release package on pushes, PRs and manual runs. These checks establish functional
regression coverage, not production capacity or a dependency performance claim.
