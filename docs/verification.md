# Verification and measurements

`scripts/check` runs rustfmt in check mode, Clippy with warnings denied and the
fast Rust test suite. Linux tests cover real FIFO startup without a writer,
split/coalesced records, first record, EOF/reopen, malformed/oversized recovery,
HTTP under slow connections, fatal non-FIFO input and bounded shutdown.
Private handler tests hold an actual encoder to check two overlapping scrapes,
excess admission, wait expiry, cancellation and permit recovery; no public test
hook or additional encoder is used.
Fake-clock tests cover pulse renewal, strict deadlines and host isolation;
no test waits 240 seconds. Language-neutral golden tests compare the real pinned
Ruby middleware contract, preserving names, types, HELP, labels, escaping,
startup presence and zero after expiry. Only order and equivalent numeric
rendering are normalized. See [fixture provenance](../tests/fixtures/README.md).

Compatibility regressions cover the two supplied nested-header records,
direct and forwarded controls, ASCII PID/svlogd/namespace matching, Ruby LXC
prefix/strip behavior and body replacement labels. Raw-byte cases check an
intact ASCII alert before or after invalid UTF-8, interrupted signatures that
must not match, strict header validation, unknown-host diagnostics and borrowed
valid bodies. Real FIFO coverage includes an invalid UTF-8 body; framing tests
enforce the original byte limit before decoding, including split sequences and
EOF tails.

## Finite replay driver

Build a release binary once and use `scripts/benchmark.py` with an unused local
port and sanitized configuration. In a managed workspace, register the whole
finite driver with `dev-session service run` so all candidate processes are owned.
For example, the driver command itself is:

```sh
python3 scripts/benchmark.py --config tests/fixtures/config.json --port 19102 \
  --profile oom --records 100000 --rate 10000 --output /tmp/oom-result.json \
  -- ./target/release/syslog-exporter --config '{config}' --listen '{listen}'
```

The driver creates its own FIFO and config, runs the supplied command, performs
warm-up, writes seeded finite records and a final marker, collects scrapes and
process RSS/CPU, and terminates/reaps the candidate. It writes JSON, final metrics
and candidate stderr beside the selected output path. It never targets a live
FIFO. `{config}` and `{listen}` are substituted in individual arguments, without
a shell. An explicit timeout bounds the run; overload/reconnect experiments still
need their own rsyslog fixture and stage counters.

Use the same isolated CPU allocation, host map, pipe capacity, byte corpus, seed,
rate, run duration, scrape interval and release profile for old and new.
`--profile ordinary` varies ordinary record lengths; `--profile oom` includes OOM
starts and long stack-shaped lines. `--cardinality` varies ordinary program
labels independently of fixed-cardinality tests. Synthetic results are not
production traffic estimates. `--rate 0` measures finite direct-FIFO capacity;
blocking/backpressure can lower achieved write rate and must be reported.
`--scrape-interval 0` disables periodic scrapes for the control measurement.
Run warm-up separately and repeat each matched case at least three times.

Direct-FIFO counts establish reader-stage behavior only. Test actual pinned
rsyslog TCP, UDP and local paths separately, counting submitted, archived,
FIFO-observed and exporter-accepted records wherever observable. Full-pipe,
paused reader, stop/restart and partial-write recovery require explicit finite
experiments. Never place two readers on one FIFO for comparison. Report stages
that cannot be observed as unknown.

Compare sustainable rate, finite burst drain time, reader-stage count deltas,
p99 scrape latency, throughput with/without normal scrapes, CPU, maximum RSS and
post-warm-up memory trend. High-cardinality retained series are a separate memory
profile. Performance thresholds belong to a controlled acceptance run, not noisy
hosted CI. Source tests alone do not establish performance or production safety.

The same driver supports `--transport tcp`, `udp` or `local` with
`--rsyslog /exact/store/path/bin/rsyslogd`. It starts only an owned disposable
daemon, preserving the central default template and file-before-pipe action
order, with private paths and input endpoints. Local mode injects into a private
Unix socket; it does not test journald end to end. Archive evidence counts unique
`seq` identities and duplicates separately. Producer acceptance and unique FIFO/
exporter sequence identities remain unavailable; exporter metrics provide counts,
not sequence IDs. A zero count gap alone does not prove there were no duplicates.

`--burst-size N --burst-gap SECONDS` repeats finite bursts. `--pause-seconds N`
uses SIGSTOP halfway through replay, then resumes the owned exporter. This stops
HTTP and expiry as well as ingestion and measures a transport outage; the Rust
HTTP regression test covers independent slow-client isolation. All writes and
marker waits share the explicit deadline. Cleanup resumes and terminates only
owned child processes, preserving final metrics, diagnostics and compact JSON
before disposable fixture files are removed. Long rate sweeps, restart scenarios
and production-envelope acceptance are separate verification work.
