use prometheus::{IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(usize)]
pub enum Family {
    KernelMessageCount,
    KernelBug,
    KernelGpf,
    KernelEmergency,
    KernelNfConntrackTableFull,
    KernelOomCount,
    LxcStartFailed,
    LxcStartNetnsLimit,
    MessageCount,
    NodectldCrash,
    NodectldSegfault,
    NodectldZfsStreamReceiveError,
    OsctldCommandCount,
    OsctldInternalError,
    FlareTest,
    VpsadminWebuiFatalErrorCount,
    VpsadminWebuiFatalError,
    ZfsPanic,
}
pub const SPECS: &[(&str, &str, bool, &[&str])] = &[
    (
        "syslog_kernel_message_count",
        "Number of kernel log messages per syslog namespace",
        true,
        &["syslogns"],
    ),
    (
        "syslog_kernel_bug",
        "1 if kernel bug has occurred, 0 otherwise",
        false,
        &["type"],
    ),
    (
        "syslog_kernel_gpf",
        "1 if kernel general protection fault has occurred, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_kernel_emergency",
        "1 if kernel emergency message has been detected, 0 otherwise",
        false,
        &["type"],
    ),
    (
        "syslog_kernel_nf_conntrack_table_full",
        "1 if kernel nf_conntrack table full message has been detected, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_kernel_oom_count",
        "Number of out-of-memory situations reported by the kernel",
        true,
        &[],
    ),
    (
        "syslog_lxc_start_failed",
        "1 if a container has failed to start, 0 otherwise",
        false,
        &["id"],
    ),
    (
        "syslog_lxc_start_netns_limit",
        "1 if a container has hit netns limit and failed to start, 0 otherwise",
        false,
        &["id"],
    ),
    (
        "syslog_message_count",
        "Number of syslog messages sent by a host",
        true,
        &["program"],
    ),
    (
        "syslog_nodectld_crash",
        "1 if nodectld has crashed, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_nodectld_segfault",
        "1 if nodectld has segfaulted, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_nodectld_zfs_stream_receive_error",
        "1 if zfs recv in nodectld has reported an error, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_osctld_command_count",
        "Number of executed management commands",
        true,
        &["command"],
    ),
    (
        "syslog_osctld_internal_error",
        "1 if osctld internal error has occurred, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_flare_test",
        "1 if flare is on, 0 otherwise",
        false,
        &[],
    ),
    (
        "syslog_vpsadmin_webui_fatal_error_count",
        "Number of fatal vpsAdmin WebUI PHP errors reported by nginx",
        true,
        &["error_type"],
    ),
    (
        "syslog_vpsadmin_webui_fatal_error",
        "1 if a fatal vpsAdmin WebUI PHP error has occurred, 0 otherwise",
        false,
        &["error_type"],
    ),
    (
        "syslog_zfs_panic",
        "1 if ZFS panic has occurred, 0 otherwise",
        false,
        &[],
    ),
];

#[derive(Clone)]
pub enum Vector {
    Counter(IntCounterVec),
    Gauge(IntGaugeVec),
}
#[derive(Clone)]
pub enum Handle {
    Counter(IntCounter),
    Gauge(IntGauge),
}
impl Handle {
    pub fn increment(&self) {
        if let Self::Counter(c) = self {
            c.inc();
        } else {
            unreachable!()
        }
    }
    pub fn set(&self, value: i64) {
        if let Self::Gauge(g) = self {
            g.set(value);
        } else {
            unreachable!()
        }
    }
}
pub fn register(registry: &Registry) -> Result<Vec<Vector>, prometheus::Error> {
    SPECS
        .iter()
        .map(|(name, help, counter, extra)| {
            let mut labels = extra.to_vec();
            labels.extend(["alias", "fqdn"]);
            let opts = Opts::new(*name, *help);
            if *counter {
                let v = IntCounterVec::new(opts, &labels)?;
                registry.register(Box::new(v.clone()))?;
                Ok(Vector::Counter(v))
            } else {
                let v = IntGaugeVec::new(opts, &labels)?;
                registry.register(Box::new(v.clone()))?;
                Ok(Vector::Gauge(v))
            }
        })
        .collect()
}
impl Vector {
    pub fn handle(&self, f: Family, key: &str, alias: &str, fqdn: &str) -> Handle {
        let mut labels = Vec::with_capacity(3);
        if !SPECS[f as usize].3.is_empty() {
            labels.push(key);
        }
        labels.extend([alias, fqdn]);
        match self {
            Self::Counter(v) => Handle::Counter(v.with_label_values(&labels)),
            Self::Gauge(v) => Handle::Gauge(v.with_label_values(&labels)),
        }
    }
}

#[derive(Clone)]
pub struct Diagnostics {
    pub bytes: IntCounter,
    pub records: IntCounter,
    pub body_decoded: IntCounter,
    pub rejected: IntCounterVec,
    pub unknown_hosts: IntCounter,
    pub reconnects: IntCounter,
    pub capacity: IntGauge,
}
impl Diagnostics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let d = Self {
            bytes: IntCounter::new(
                "syslog_exporter_bytes_read_total",
                "Bytes read from the FIFO",
            )?,
            records: IntCounter::new(
                "syslog_exporter_records_read_total",
                "Complete FIFO records read, including rejected records",
            )?,
            body_decoded: IntCounter::new(
                "syslog_exporter_body_decoded_records_total",
                "Parsed records whose body required UTF-8 replacement decoding",
            )?,
            rejected: IntCounterVec::new(
                Opts::new(
                    "syslog_exporter_rejected_records_total",
                    "Rejected FIFO records by fixed reason",
                ),
                &["reason"],
            )?,
            unknown_hosts: IntCounter::new(
                "syslog_exporter_unknown_host_records_total",
                "Parsed records from unconfigured hosts",
            )?,
            reconnects: IntCounter::new(
                "syslog_exporter_fifo_reconnects_total",
                "FIFO opens after the initial open",
            )?,
            capacity: IntGauge::new(
                "syslog_exporter_fifo_capacity_bytes",
                "Effective kernel FIFO capacity in bytes",
            )?,
        };
        registry.register(Box::new(d.bytes.clone()))?;
        registry.register(Box::new(d.records.clone()))?;
        registry.register(Box::new(d.body_decoded.clone()))?;
        registry.register(Box::new(d.rejected.clone()))?;
        registry.register(Box::new(d.unknown_hosts.clone()))?;
        registry.register(Box::new(d.reconnects.clone()))?;
        registry.register(Box::new(d.capacity.clone()))?;
        for reason in ["header", "timestamp", "utf8", "oversized", "incomplete"] {
            d.rejected.with_label_values(&[reason]);
        }
        Ok(d)
    }
    pub fn reject(&self, reason: &str) {
        self.rejected.with_label_values(&[reason]).inc();
    }
}

/// Keep Ruby's metadata-only families; the library encoder handles every sample.
pub fn encode(registry: &Registry) -> Result<String, prometheus::Error> {
    let families = registry.gather();
    let mut text = prometheus::TextEncoder::new().encode_to_string(&families)?;
    for (name, help, counter, _) in SPECS {
        if !families.iter().any(|family| family.name() == *name) {
            text.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} {}\n",
                if *counter { "counter" } else { "gauge" }
            ));
        }
    }
    Ok(text)
}
