# Syslog Exporter

Syslog Exporter reads newline-delimited rsyslog messages from a FIFO and exposes
Prometheus metrics on `/metrics`. It includes kernel/OOM, ZFS, container startup,
osctld, nodectld and opt-in vpsAdmin WebUI detectors.

Build with Rust 1.95.0 on Linux:

```sh
cargo build --release --locked
./target/release/syslog-exporter --config config-sample.json --check-config
./target/release/syslog-exporter --config /etc/syslog-exporter.json --listen 0.0.0.0:9102
```

The configured FIFO must already exist with permissions allowing the exporter to
read it. Run exactly one reader per FIFO. The default listener is `0.0.0.0:9102`;
use network policy or a loopback listener to restrict access as needed.
`SYSLOG_EXPORTER_CONFIG` supplies the config path when `--config` is absent;
if neither is set, the path defaults to `config-sample.json`. Configuration
validation does not open a FIFO or listener. The process exits on fatal input
errors and handles SIGTERM/SIGINT with bounded shutdown.

Development uses the same toolchain as the configuration package:

```sh
nix-shell -I nixpkgs=https://github.com/NixOS/nixpkgs/archive/718f670e845328e9c2cecdd832835712b2cbd065.tar.gz
# Inside the shell:
# Standalone clones only (see linked worktrees below):
git config --local core.hooksPath .githooks
scripts/check
```

For linked worktrees, repository-local config is shared. Keep the shared hook
setting unchanged and select this worktree's active hook for each Git command:

```sh
git -c core.hooksPath="$PWD/.githooks" commit -F /path/to/message
git -c core.hooksPath="$PWD/.githooks" push origin HEAD
```

The hook runs formatting checks, Clippy and tests, and never stages or rewrites
files. Cargo.lock is committed. `shell.nix` installs nothing on shell entry.
CI uses the pinned toolchain and the same check entry point.

- [Configuration, line protocol and metric behavior](docs/reference.md)
- [Tests, fixtures and finite performance measurements](docs/verification.md)
- [Pinned Ruby/rsyslog fixture provenance](tests/fixtures/README.md)
