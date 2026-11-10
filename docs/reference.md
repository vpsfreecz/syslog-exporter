# Configuration and behavior

The JSON object accepts `syslog_pipe`, `pipe_size` (default 1048576),
`max_line_bytes` (default 1048576 including LF), and `hosts`.
Each host entry requires `alias`, `fqdn`, and `os`. The optional `collectors` list
defaults to empty. Unknown JSON keys are ignored. Unknown collector names produce
one startup warning and are ignored. No reload is supported.

The host object's key and its `fqdn` both route incoming messages. Its alias is
used only as a metric label. The first host in JSON order wins if
identities collide, with a startup warning. Host aliases and FQDNs appear on
all legacy metrics as `alias` and `fqdn`.

Every host gets kernel, message-count, flare-test and ZFS detection. LXC,
nodectld and osctld detection requires `os: "vpsadminos"`. Add
`"collectors": ["vpsadmin_webui"]` to enable WebUI detection on any OS; this
list does not disable the other eligible collectors.

## Input and delivery

The supported FIFO record is `ISO8601_TIMESTAMP HOST PROGRAM[PID] MESSAGE` plus
LF, using one ASCII space between header fields. The timestamp subset is RFC3339
as emitted by the pinned producer, including fractional seconds and offsets.
This is the central pipe format, not a general RFC 3164/5424 socket listener.
The special localhost svlogd timestamp/host/program form is supported. Kernel,
kernel[], kernel[digits] and forwarded kernel messages share namespace handling.
Ordinary PID suffixes contain ASCII digits and are stripped; forwarded programs
otherwise stay verbatim. The svlogd recognizer also uses ASCII digits. A program
colon is retained. Kernel namespace tags use the `[ TAG ] body` form, with ASCII
whitespace (space, tab, LF, VT, FF and CR). Unicode whitespace remains part of
the namespace. LXC IDs follow Ruby's first-colon rule: use the prefix before the
first colon, an empty ID if there is no colon, or the whole body if it starts
with a colon. Trim only NUL and those six ASCII whitespace characters from
either end of the selected ID; preserve Unicode whitespace.

The first complete line is processed. Timestamp, host and program fields must
contain valid UTF-8, including the inner host and program in a recognized svlogd
record. Invalid encoding, invalid timestamps and malformed required fields are
rejected without aborting ingestion. Nested header text in an ordinary message
stays in the body; the parser does not recursively unwrap it.

Valid UTF-8 bodies remain unchanged and borrowed. Bodies with invalid UTF-8 use
standard replacement decoding once per record, substituting U+FFFD for invalid
sequences. Intact ASCII alert signatures before or after those sequences remain
detectable; replacement preserves the separation between interrupted signature
fragments. Every collector and body-derived label uses the same decoded text.
Different invalid byte sequences and a literal U+FFFD can share a label value,
so labels cannot preserve every distinction in the original bytes.

An oversized record is counted once and discarded through the next LF, then
parsing resumes. The limit is independent of pipe capacity. A partial tail is
discarded on EOF or shutdown. There is no multiline reconstruction, replay, disk queue or
persistent metric state. A partial write joined to a later record may remain
parseable, so rejection counts cannot establish transport loss.
Limits and byte counters use the original bytes, including LF, before decoding.
Replacement text can use up to three output bytes per input body byte; the rare
allocation remains bounded by one input record.

The reader checks the opened descriptor is a FIFO. A refused pipe enlargement
warns once and uses the actual capacity; it does not prevent startup. EOF closes
the descriptor and reopens with an interruptible 100ms backoff. One input thread
owns framing, routing, collector state and expiry, without an application queue.
It services deadlines between bounded batches, even under continuous input.

The kernel FIFO provides limited buffering. Rsyslog can suspend or discard
output during saturation, restart or reader gaps. Exporter counters cover only
records reaching this reader; they do not measure upstream TCP/UDP, rsyslog
queues, short writes or lost events. Ordinary remote log files are not an
automatic replay source. Faster ingestion does not guarantee lossless delivery.
The line buffer is bounded; retained dynamic metric series can still grow with
program/command/container/namespace cardinality.

## Metrics and timers

All 18 Ruby metric families, their types, HELP text, label sets, initial zeros
and conditional series creation are preserved. Counter names retain `_count`,
without an added `_total`. The [wire fixtures](../tests/fixtures/README.md) and
`src/metrics.rs` contain the exact contract. Empty legacy families still expose
HELP/TYPE metadata, and absent kernel namespace is the empty label value.

Gauges pulse at 1 for 240s (kernel/LXC/ZFS), 180s (nodectld/osctld/WebUI) or 15s
(flare test), then return to zero on a five-second sweep strictly after their
deadline. Repeated matches renew the deadline for that host/family/label set.
Expiry uses monotonic processing time, independent of message timestamps.
There is no cross-line or PID correlation. Expired zero series remain exposed.
Counters and pulses reset on restart; rollback does not recover lost events.

The independent HTTP runtime serves `/metrics` and the plain `OK` fallback.
Gzip is supported for metrics. The two-byte fallback follows middleware
compression thresholds, so its incidental `Vary` and `Content-Encoding` headers can
differ from Ruby. Fallback status, content type and body remain compatible.
There is one encoding job at a time. Two metrics requests can be admitted: one
active encode and one asynchronous waiter for up to one second. Excess requests
or an expired wait receive HTTP 503. Admission also applies to HTTP/2 streams.
The admission and encoder permits remain held until encoding finishes, even if
the client disconnects. The server releases them before writing the response.
At most 32 connections are active, each limited to 30 seconds total lifetime.
Response bytes are detached from ingestion before client writes. Scrapes are not
transactional snapshots across metric families.

Diagnostics have a separate `syslog_exporter_` namespace: bytes and complete
records read, rejected records by fixed reason (`header`, `timestamp`, `utf8`,
`oversized`, `incomplete`), unknown-host records, FIFO reopen count and effective
capacity. `syslog_exporter_body_decoded_records_total` is a fixed unlabeled
counter for successfully parsed records whose bodies needed replacement decoding.
It increments once before host routing, including for unknown hosts. Invalid
headers do not increment it. Replacement-decoded bodies are accepted; the `utf8`
rejection reason applies to headers. The decoded-body counter overlaps intake
counts and must not be added to them. No payload-derived diagnostic labels or
per-record payload logs exist.
HTTP availability alone does not establish healthy ingestion.
