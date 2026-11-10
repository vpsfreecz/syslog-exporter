use prometheus::Registry;
use std::{collections::BTreeSet, time::Duration};
use syslog_exporter::{collectors::Engine, config::Config, input::Framer, metrics, parser};
fn setup() -> (Engine, Registry) {
    let config: Config = serde_json::from_str(include_str!("fixtures/config.json")).unwrap();
    config.validate().unwrap();
    let registry = Registry::new();
    (Engine::new(&config, &registry).unwrap(), registry)
}
fn canonical(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.contains("syslog_exporter_"))
        .map(|line| {
            if line.starts_with('#') {
                return line.to_owned();
            }
            let (name_labels, value) = line.rsplit_once(' ').unwrap();
            let name_labels = if let Some((name, labels)) = name_labels.split_once('{') {
                let mut pieces = Vec::new();
                let mut start = 0;
                let mut quote = false;
                let mut escape = false;
                let labels = labels.strip_suffix('}').unwrap();
                for (i, ch) in labels.char_indices() {
                    if escape {
                        escape = false;
                        continue;
                    }
                    if ch == '\\' {
                        escape = true;
                    } else if ch == '"' {
                        quote = !quote;
                    } else if ch == ',' && !quote {
                        pieces.push(&labels[start..i]);
                        start = i + 1;
                    }
                }
                pieces.push(&labels[start..]);
                pieces.sort();
                format!("{name}{{{}}}", pieces.join(","))
            } else {
                name_labels.to_owned()
            };
            format!("{name_labels} {}", value.parse::<f64>().unwrap())
        })
        .collect()
}
#[test]
fn pinned_ruby_wire_parity() {
    let (mut engine, registry) = setup();
    assert_eq!(
        canonical(&metrics::encode(&registry).unwrap()),
        canonical(include_str!("fixtures/startup.prom"))
    );
    for line in include_bytes!("fixtures/events.log").split_inclusive(|b| *b == b'\n') {
        engine.process(&parser::parse(line).unwrap(), Duration::ZERO);
    }
    assert_eq!(
        canonical(&metrics::encode(&registry).unwrap()),
        canonical(include_str!("fixtures/triggered.prom"))
    );
    engine.settle(Duration::from_secs(241));
    assert_eq!(
        canonical(&metrics::encode(&registry).unwrap()),
        canonical(include_str!("fixtures/expired.prom"))
    );
}
fn sample(registry: &Registry, name: &str, contains: &[&str]) -> f64 {
    let text = metrics::encode(registry).unwrap();
    text.lines()
        .find(|line| {
            line.starts_with(&format!("{name}{{")) && contains.iter().all(|s| line.contains(s))
        })
        .unwrap_or_else(|| panic!("missing {name} {contains:?} in {text}"))
        .rsplit_once(' ')
        .unwrap()
        .1
        .parse()
        .unwrap()
}
fn event(engine: &mut Engine, host: &str, program: &str, body: &str, time: u64) {
    engine.process(
        &parser::Message {
            host,
            program,
            body: body.into(),
        },
        Duration::from_secs(time),
    );
}
#[test]
fn renewal_strict_expiry_host_isolation_and_retained_zero() {
    let (mut e, r) = setup();
    event(&mut e, "host-vps", "x", "flare test", 0);
    event(&mut e, "host-web", "x", "flare test", 5);
    event(&mut e, "host-vps", "x", "flare test", 10);
    e.settle(Duration::from_secs(20));
    assert_eq!(sample(&r, "syslog_flare_test", &["alias=\"web\""]), 1.0);
    e.settle(Duration::from_secs(21));
    assert_eq!(sample(&r, "syslog_flare_test", &["alias=\"web\""]), 0.0);
    assert_eq!(
        sample(&r, "syslog_flare_test", &["fqdn=\"node.example\""]),
        1.0
    );
    e.settle(Duration::from_secs(25));
    assert_eq!(
        sample(&r, "syslog_flare_test", &["fqdn=\"node.example\""]),
        1.0
    );
    e.settle(Duration::from_secs(26));
    assert_eq!(
        sample(&r, "syslog_flare_test", &["fqdn=\"node.example\""]),
        0.0
    );
}
#[test]
fn negative_matches_precedence_gates_and_newline_anchor() {
    let (mut e, r) = setup();
    event(
        &mut e,
        "host-vps",
        "kernel",
        "traps: x general protection fault",
        0,
    );
    event(
        &mut e,
        "host-vps",
        "kernel",
        "BUG: kernel NULL pointer dereference invoked oom-killer: x",
        0,
    );
    event(&mut e, "host-vps", "osctld", "Received command 'one'\n", 0);
    event(
        &mut e,
        "host-vps",
        "osctld",
        "Received command 'two' trailing",
        0,
    );
    event(&mut e, "host-vps", "osctld:", "Error: internal error", 0);
    event(&mut e, "host-other", "osctld", "Error: internal error", 0);
    event(
        &mut e,
        "host-vps",
        "nginx",
        "FastCGI sent in stderr PHP message: PHP Fatal error:",
        0,
    );
    let text = metrics::encode(&r).unwrap();
    assert!(!text.lines().any(|l| l.starts_with("syslog_kernel_gpf{")));
    assert!(
        !text
            .lines()
            .any(|l| l.starts_with("syslog_kernel_oom_count{"))
    );
    assert!(!text.contains("command=\"two\""));
    assert_eq!(
        sample(&r, "syslog_osctld_command_count", &["command=\"one\""]),
        1.0
    );
    assert_eq!(sample(&r, "syslog_osctld_internal_error", &[]), 0.0);
    assert!(
        !text
            .lines()
            .any(|l| l.starts_with("syslog_vpsadmin_webui_fatal_error_count{"))
    );
}
#[test]
fn framing_boundaries_recovery_and_partial_tail() {
    let (mut e, r) = setup();
    let record = b"2026-11-10T12:00:00Z host-vps kernel[] apache invoked oom-killer: flags\n";
    let mut f = Framer::new(record.len());
    f.span(&record[..15], &mut e, Duration::ZERO);
    f.span(&record[15..], &mut e, Duration::ZERO);
    // Exactly limit including LF is accepted; limit+1 is rejected once.
    f.span(&vec![b'x'; record.len() + 1], &mut e, Duration::ZERO);
    f.span(b"suffix\n", &mut e, Duration::ZERO);
    f.span(b"bad host x text\n", &mut e, Duration::ZERO);
    f.span(record, &mut e, Duration::ZERO);
    f.span(b"partial", &mut e, Duration::ZERO);
    f.end(&e.diagnostics);
    assert_eq!(sample(&r, "syslog_kernel_oom_count", &[]), 2.0);
    assert_eq!(
        e.diagnostics
            .rejected
            .with_label_values(&["oversized"])
            .get(),
        1
    );
    assert_eq!(
        e.diagnostics
            .rejected
            .with_label_values(&["timestamp"])
            .get(),
        1
    );
    assert_eq!(
        e.diagnostics
            .rejected
            .with_label_values(&["incomplete"])
            .get(),
        1
    );
    assert_eq!(e.diagnostics.records.get(), 4);
}
#[test]
fn first_configured_identity_wins_in_json_order() {
    let config: Config=serde_json::from_str(r#"{"syslog_pipe":"/tmp/not-opened","hosts":{"z":{"alias":"first","fqdn":"same","os":"nixos"},"a":{"alias":"second","fqdn":"same","os":"nixos"}}}"#).unwrap();
    config.validate().unwrap();
    let r = Registry::new();
    let mut e = Engine::new(&config, &r).unwrap();
    event(&mut e, "same", "x", "ok", 0);
    assert_eq!(
        sample(&r, "syslog_message_count", &["alias=\"first\""]),
        1.0
    );
    assert!(
        !metrics::encode(&r)
            .unwrap()
            .lines()
            .any(|l| l.starts_with("syslog_message_count{") && l.contains("second"))
    );
}
#[test]
fn pinned_producer_shape_and_kernel_crash_fixes() {
    for line in include_bytes!("fixtures/producer-output.log").split_inclusive(|b| *b == b'\n') {
        parser::parse(line).unwrap();
    }
    let (mut e, r) = setup();
    for line in [
        "2026-11-10T12:00:00Z host-vps kernel[12] apache invoked oom-killer: flags\n",
        "2026-11-10T12:00:00Z localhost 2026-11-10_12 00:00.000 host-vps kernel apache invoked oom-killer: flags\n",
    ] {
        e.process(&parser::parse(line.as_bytes()).unwrap(), Duration::ZERO);
    }
    assert_eq!(sample(&r, "syslog_kernel_oom_count", &[]), 2.0);
    // A parseable concatenation cannot be identified as transport loss by this protocol.
    assert!(
        parser::parse(
            b"2026-11-10T12:00:00Z host-vps x partial2026-11-10T12:00:01Z host-vps x next\n"
        )
        .is_ok()
    );
}

#[test]
fn longer_timers_preserve_strict_boundaries() {
    for (program, body, family, seconds) in [
        ("nodectld", "Daemon crashed", "syslog_nodectld_crash", 180),
        (
            "kernel",
            "BUG: kernel NULL pointer dereference",
            "syslog_kernel_bug",
            240,
        ),
    ] {
        let (mut engine, registry) = setup();
        event(&mut engine, "host-vps", program, body, 7);
        engine.settle(Duration::from_secs(seconds + 6));
        assert_eq!(sample(&registry, family, &["fqdn=\"node.example\""]), 1.0);
        engine.settle(Duration::from_secs(seconds + 7));
        assert_eq!(sample(&registry, family, &["fqdn=\"node.example\""]), 1.0);
        engine.settle(Duration::from_secs(seconds + 8));
        assert_eq!(sample(&registry, family, &["fqdn=\"node.example\""]), 0.0);
    }
}

fn compatibility_setup() -> (Engine, Registry) {
    let mut config: Config = serde_json::from_str(include_str!("fixtures/config.json")).unwrap();
    config.hosts.insert(
        "cz.vpsfree/nodes/stg/node2".into(),
        serde_json::json!({"alias":"node2.stg","fqdn":"node2.stg.vpsfree.cz","os":"vpsadminos"}),
    );
    config.validate().unwrap();
    let registry = Registry::new();
    (Engine::new(&config, &registry).unwrap(), registry)
}

#[test]
fn compatibility_user_records_and_positive_controls() {
    let (mut engine, registry) = compatibility_setup();
    let mut framer = Framer::new(1048576);
    for line in include_bytes!("fixtures/user-records.log").split_inclusive(|b| *b == b'\n') {
        let message = parser::parse(line).unwrap();
        assert_eq!(message.program, "1");
        framer.span(line, &mut engine, Duration::ZERO);
    }
    assert_eq!(
        sample(
            &registry,
            "syslog_message_count",
            &["alias=\"node2.stg\"", "program=\"1\""]
        ),
        2.0
    );
    assert_eq!(
        sample(
            &registry,
            "syslog_kernel_bug",
            &["alias=\"node2.stg\"", "type=\"nullptr\""]
        ),
        0.0
    );
    for line in [
        "2026-11-11T12:00:00Z node2.stg.vpsfree.cz kernel BUG: kernel NULL pointer dereference\n",
        "2026-11-11T12:00:00Z cz.vpsfree/nodes/stg/node2 kernel[] [ ns ] ordinary\n",
        "2026-11-11T12:00:00Z cz.vpsfree/nodes/stg/node2 kernel[123] ordinary\n",
        "2026-11-11T12:00:00Z cz.vpsfree/nodes/stg/node2 kernel  [ ns ] apache invoked oom-killer: flags\n",
        "2026-11-11T12:00:00Z localhost 2026-11-11_12 00:00.000000 node2.stg.vpsfree.cz nodectld Daemon crashed\n",
    ] {
        framer.span(line.as_bytes(), &mut engine, Duration::ZERO);
    }
    assert_eq!(
        sample(&registry, "syslog_kernel_bug", &["alias=\"node2.stg\""]),
        1.0
    );
    assert_eq!(
        sample(
            &registry,
            "syslog_kernel_message_count",
            &["alias=\"node2.stg\"", "syslogns=\"ns\""]
        ),
        1.0
    );
    assert_eq!(
        sample(
            &registry,
            "syslog_kernel_oom_count",
            &["alias=\"node2.stg\""]
        ),
        1.0
    );
    assert_eq!(
        sample(&registry, "syslog_nodectld_crash", &["alias=\"node2.stg\""]),
        1.0
    );
    assert_eq!(engine.diagnostics.body_decoded.get(), 0);
}

#[test]
fn compatibility_ascii_classes_and_ruby_lxc_prefix() {
    let (mut engine, registry) = compatibility_setup();
    let mut framer = Framer::new(1048576);
    framer.span("2026-11-11T12:00:00Z cz.vpsfree/nodes/stg/node2 kernel[١] BUG: kernel NULL pointer dereference\n".as_bytes(), &mut engine, Duration::ZERO);
    assert_eq!(
        sample(
            &registry,
            "syslog_message_count",
            &["alias=\"node2.stg\"", "program=\"kernel[١]\""]
        ),
        1.0
    );
    assert_eq!(
        sample(&registry, "syslog_kernel_bug", &["alias=\"node2.stg\""]),
        0.0
    );
    for body in ["[ \u{a0}ns ] ordinary", "[ \tns \t] ordinary"] {
        let line = format!("2026-11-11T12:00:00Z cz.vpsfree/nodes/stg/node2 kernel {body}\n");
        framer.span(line.as_bytes(), &mut engine, Duration::ZERO);
    }
    for ns in ["\u{a0}ns", "ns"] {
        assert_eq!(
            sample(
                &registry,
                "syslog_kernel_message_count",
                &["alias=\"node2.stg\"", &format!("syslogns=\"{ns}\"")]
            ),
            1.0
        );
    }
    for (body, expected) in [
        (
            "\u{a0}42\u{a0}: Failed to spawn container \"42\"",
            "\u{a0}42\u{a0}",
        ),
        (
            "\0 \t\u{b}\u{c}\r42\0 \t\u{b}\u{c}\r: Failed to spawn container \"42\"",
            "42",
        ),
        (
            ": Failed to spawn container \"42\"",
            ": Failed to spawn container \"42\"",
        ),
        ("Failed to spawn container \"42\"", ""),
        ("42: Failed to spawn container \"42\"", "42"),
    ] {
        let (mut e, r) = compatibility_setup();
        let line = format!("2026-11-11T12:00:00Z cz.vpsfree/nodes/stg/node2 lxc {body}\n");
        framer.span(line.as_bytes(), &mut e, Duration::ZERO);
        assert_eq!(
            sample(
                &r,
                "syslog_lxc_start_failed",
                &[
                    "alias=\"node2.stg\"",
                    &format!("id=\"{}\"", expected.replace('"', "\\\""))
                ]
            ),
            1.0
        );
    }
}

#[test]
fn compatibility_body_decoding_alerts_and_diagnostics() {
    for (body, flare, decoded) in [
        (b"\xffDaemon crashed\n".as_slice(), 1.0, 1),
        (b"Daemon crashed \xff\n".as_slice(), 1.0, 1),
        (b"Daemon cra\xffshed\n".as_slice(), 0.0, 1),
        (b"Daemon crashed \xe2\x82\n".as_slice(), 1.0, 1),
        ("Daemon crashed \u{fffd}\n".as_bytes(), 1.0, 0),
    ] {
        let (mut e, r) = setup();
        let mut line = b"2026-11-11T12:00:00Z host-vps nodectld ".to_vec();
        line.extend_from_slice(body);
        let mut f = Framer::new(line.len());
        f.span(&line, &mut e, Duration::ZERO);
        assert_eq!(
            sample(&r, "syslog_message_count", &["program=\"nodectld\""]),
            1.0
        );
        assert_eq!(
            sample(&r, "syslog_nodectld_crash", &["fqdn=\"node.example\""]),
            flare
        );
        assert_eq!(e.diagnostics.body_decoded.get(), decoded);
        assert_eq!(e.diagnostics.rejected.with_label_values(&["utf8"]).get(), 0);
    }
    let (mut e, r) = setup();
    let mut f = Framer::new(1048576);
    f.span(
        b"2026-11-11T12:00:00Z unknown x \xff\xff\n",
        &mut e,
        Duration::ZERO,
    );
    assert_eq!(e.diagnostics.body_decoded.get(), 1);
    assert_eq!(e.diagnostics.unknown_hosts.get(), 1);
    f.span(
        b"2026-11-11T12:00:00Z ho\xffst x \xff\n",
        &mut e,
        Duration::ZERO,
    );
    assert_eq!(e.diagnostics.body_decoded.get(), 1);
    assert_eq!(e.diagnostics.rejected.with_label_values(&["utf8"]).get(), 1);
    f.span(b"2026-11-11T12:00:00Z localhost 2026-11-11_12 00:00.000 host-vps nodectld \xffDaemon crashed\n", &mut e, Duration::ZERO);
    assert_eq!(e.diagnostics.body_decoded.get(), 2);
    assert_eq!(
        sample(&r, "syslog_nodectld_crash", &["fqdn=\"node.example\""]),
        1.0
    );
    let encoded = metrics::encode(&r).unwrap();
    assert!(
        encoded
            .lines()
            .any(|l| l == "syslog_exporter_body_decoded_records_total 2")
    );
    assert!(!encoded.contains("syslog_exporter_body_decoded_records_total{"));
}

#[test]
fn compatibility_replacement_labels_and_raw_byte_framing() {
    let (mut e, r) = setup();
    let mut f = Framer::new(1048576);
    for line in [
        b"2026-11-11T12:00:00Z host-vps kernel [ n\xff ] ordinary\n".as_slice(),
        b"2026-11-11T12:00:00Z host-vps kernel [ n\xfe ] ordinary\n",
        "2026-11-11T12:00:00Z host-vps kernel [ n\u{fffd} ] ordinary\n".as_bytes(),
        b"2026-11-11T12:00:00Z host-vps lxc \xff42: Failed to spawn container \"42\"\n",
        b"2026-11-11T12:00:00Z host-vps osctld Received command 'cmd\xff'\n",
    ] {
        f.span(line, &mut e, Duration::ZERO);
    }
    assert_eq!(
        sample(
            &r,
            "syslog_kernel_message_count",
            &["syslogns=\"n\u{fffd}\""]
        ),
        3.0
    );
    assert_eq!(
        sample(&r, "syslog_lxc_start_failed", &["id=\"\u{fffd}42\""]),
        1.0
    );
    assert_eq!(
        sample(
            &r,
            "syslog_osctld_command_count",
            &["command=\"cmd\u{fffd}\""]
        ),
        1.0
    );
    assert_eq!(e.diagnostics.body_decoded.get(), 4);
    let line = b"2026-11-11T12:00:00Z host-vps nodectld Daemon crashed \xe2\x82\n";
    let (mut e, r) = setup();
    let mut f = Framer::new(line.len());
    f.span(&line[..line.len() - 2], &mut e, Duration::ZERO);
    f.span(&line[line.len() - 2..], &mut e, Duration::ZERO);
    assert_eq!(
        sample(&r, "syslog_nodectld_crash", &["fqdn=\"node.example\""]),
        1.0
    );
    assert_eq!(e.diagnostics.body_decoded.get(), 1);
    f.span(&vec![b'\xff'; line.len() + 1], &mut e, Duration::ZERO);
    f.span(b"\n", &mut e, Duration::ZERO);
    f.span(b"tail\xff", &mut e, Duration::ZERO);
    f.end(&e.diagnostics);
    assert_eq!(e.diagnostics.body_decoded.get(), 1);
    assert_eq!(
        e.diagnostics
            .rejected
            .with_label_values(&["oversized"])
            .get(),
        1
    );
    assert_eq!(
        e.diagnostics
            .rejected
            .with_label_values(&["incomplete"])
            .get(),
        1
    );
}
