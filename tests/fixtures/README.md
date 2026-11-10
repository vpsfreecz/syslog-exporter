# Compatibility fixtures

`startup.prom`, `triggered.prom` and `expired.prom` were captured from the real
Ruby Rack middleware at syslog-exporter
`7c5a8e74e44a704cceb99cd05716af6800d78efd` (0.13.4), using
`config.json` and `events.log`. Startup preserves every registered family's
HELP/TYPE metadata, even without samples; missing kernel namespace is `""`.
The alias deliberately contains a quote, backslash, newline and Unicode.
Tests normalize sample/label order and equivalent numeric rendering only.

`user-records.log` preserves two user-supplied archive records byte for byte. Their
outer program is `1`; nested RFC5424-looking text stays in the body. Both real
Ruby and Rust applications counted them without a kernel flare in the isolated
comparison. A separate direct-kernel control raised the expected gauge. These
are parser regression inputs, not messages to send to production.

The capture constructed `SyslogExporter::Rackup.app`, intercepted `Processor.start`
to retain the processor without launching a FIFO reader, and used
`Rack::MockRequest` on the real app. Each fixture event passed through the Ruby
parser and host-selected collectors. For the expiry capture, deadlines were set
to the past and ordinary `settle_flares` ran; there was no 240-second sleep.
`http.json` records actual middleware headers and fallback behavior.
No mocked Prometheus registry was used.

Pinned closure: `/nix/store/w7g32wplgls6x79608h6nij02a4vvq9y-syslog-exporter`,
derivation `/nix/store/hzv5vgrxq9iz7kl4brm8w8k0bbikwrbf-syslog-exporter.drv`.
Dependencies: Ruby 3.4.9, prometheus-client 4.2.5, Rack 3.2.7, Puma 8.0.2,
Bundler 2.7.2. It was derived from the configuration package with stable nixpkgs
`718f670e845328e9c2cecdd832835712b2cbd065` and proven present in that log host's
baseline system derivation graph. This store path is capture provenance, not a
runtime dependency.

`producer-input.log` is a synthetic RFC 5424 TCP input corpus;
`producer-output.log` contains actual FIFO bytes from pinned rsyslog 8.2610.0,
`/nix/store/g0g8nvq928bgl2dwvvddh99l0543s9cx-rsyslog-8.2610.0`, default
RSYSLOG_FileFormat ompipe output. The upstream tarball SHA256 is
`sha256-e9+V6B6mwO5dWGNC1X0qbjlbCJ+zxnMYoNxVWBbeQEs=`. Capture used a
registered finite local fixture with imtcp → omfile → ompipe on a disposable FIFO;
it did not read production logs. The tab becomes `#011`, high bytes survive,
and app/PID becomes `osctld[123]` without a colon for this input.
These three finite small records establish framing, not message-size or loss
limits. Legacy/UDP/local and overload behavior need separate measured evidence.

Intentional differences from Ruby have explicit tests: first complete record is
accepted, malformed records do not abort ingestion, kernel PID/forwarded forms
have safe namespace access, expiry uses monotonic processing time and has one
state owner, oversized records are bounded and discarded through LF, and
capacity-enlargement refusal falls back to actual capacity. A truncated record
joined to a subsequent valid record can still parse; the protocol cannot prove
complete resynchronization or detect all producer loss.
