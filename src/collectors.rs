use crate::{
    config::Config,
    metrics::{self, Diagnostics, Family, Handle, Vector},
    parser::Message,
};
use Family::*;
use prometheus::Registry;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
    time::Duration,
};
static COMMAND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Received command '([^']+)'(?:\n)?$").unwrap());
static SPAWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Failed to spawn container "[^"]+""#).unwrap());
const WEB: [(&str, &str); 3] = [
    ("fatal", "PHP message: PHP Fatal error:"),
    ("parse", "PHP message: PHP Parse error:"),
    (
        "recoverable_fatal",
        "PHP message: PHP Recoverable fatal error:",
    ),
];

struct HostState {
    alias: String,
    fqdn: String,
    vpsadminos: bool,
    webui: bool,
    cache: Vec<HashMap<String, usize>>,
}
pub struct Engine {
    index: HashMap<String, usize>,
    hosts: Vec<HostState>,
    vectors: Vec<Vector>,
    handles: Vec<Handle>,
    flares: HashMap<usize, Duration>,
    pub diagnostics: Diagnostics,
}
impl Engine {
    pub fn new(config: &Config, registry: &Registry) -> Result<Self, prometheus::Error> {
        let mut engine = Self {
            index: HashMap::new(),
            hosts: Vec::new(),
            vectors: metrics::register(registry)?,
            handles: Vec::new(),
            flares: HashMap::new(),
            diagnostics: Diagnostics::new(registry)?,
        };
        let mut warned = HashSet::new();
        for (name, host) in config.host_entries() {
            let id = engine.hosts.len();
            for identity in [name, host.fqdn.as_str()] {
                if let Some(previous) = engine.index.get(identity) {
                    if *previous != id && warned.insert(identity.to_owned()) {
                        eprintln!(
                            "ambiguous host identity {identity:?}; keeping the first configured host"
                        );
                    }
                } else {
                    engine.index.insert(identity.to_owned(), id);
                }
            }
            for collector in &host.collectors {
                if ![
                    "kernel",
                    "lxc_start",
                    "message_count",
                    "nodectld",
                    "osctld",
                    "test",
                    "vpsadmin_webui",
                    "zfs",
                ]
                .contains(&collector.as_str())
                    && warned.insert(collector.clone())
                {
                    eprintln!("unknown collector {collector:?}; ignoring it");
                }
            }
            let vpsadminos = host.os == "vpsadminos";
            let webui = host.collectors.iter().any(|c| c == "vpsadmin_webui");
            engine.hosts.push(HostState {
                alias: host.alias,
                fqdn: host.fqdn,
                vpsadminos,
                webui,
                cache: (0..metrics::SPECS.len()).map(|_| HashMap::new()).collect(),
            });
            for (family, key) in [
                (KernelBug, "nullptr"),
                (KernelEmergency, "unregister_netdevice"),
                (FlareTest, ""),
                (ZfsPanic, ""),
            ] {
                engine.gauge(id, family, key, 0);
            }
            if vpsadminos {
                for family in [
                    NodectldCrash,
                    NodectldSegfault,
                    NodectldZfsStreamReceiveError,
                    OsctldInternalError,
                ] {
                    engine.gauge(id, family, "", 0);
                }
            }
            if webui {
                for (key, _) in WEB {
                    engine.gauge(id, VpsadminWebuiFatalError, key, 0);
                }
            }
        }
        Ok(engine)
    }
    fn series(&mut self, host: usize, family: Family, key: &str) -> usize {
        let h = &mut self.hosts[host];
        if let Some(id) = h.cache[family as usize].get(key) {
            return *id;
        }
        let id = self.handles.len();
        self.handles
            .push(self.vectors[family as usize].handle(family, key, &h.alias, &h.fqdn));
        h.cache[family as usize].insert(key.to_owned(), id);
        id
    }
    fn counter(&mut self, host: usize, family: Family, key: &str) {
        let id = self.series(host, family, key);
        self.handles[id].increment();
    }
    fn gauge(&mut self, host: usize, family: Family, key: &str, value: i64) {
        let id = self.series(host, family, key);
        self.handles[id].set(value);
    }
    fn flare(&mut self, host: usize, family: Family, key: &str, now: Duration, seconds: u64) {
        let id = self.series(host, family, key);
        self.handles[id].set(1);
        self.flares.insert(id, now + Duration::from_secs(seconds));
    }
    pub fn settle(&mut self, now: Duration) {
        self.flares.retain(|id, deadline| {
            if now > *deadline {
                self.handles[*id].set(0);
                false
            } else {
                true
            }
        });
    }
    pub fn process(&mut self, message: &Message<'_>, now: Duration) {
        let Some(&host) = self.index.get(message.host) else {
            self.diagnostics.unknown_hosts.inc();
            return;
        };
        let (program, body) = (message.program, message.body.as_ref());
        if program == "kernel" {
            self.counter(host, KernelMessageCount, message.namespace());
            if body.contains("BUG: kernel NULL pointer dereference") {
                self.flare(host, KernelBug, "nullptr", now, 240);
            } else if body.contains("unregister_netdevice: waiting for") {
                self.flare(host, KernelEmergency, "unregister_netdevice", now, 240);
            } else if body.contains("general protection fault") && !body.contains("traps: ") {
                self.flare(host, KernelGpf, "", now, 240);
            } else if body.contains("nf_conntrack: nf_conntrack: table full, dropping packet") {
                self.flare(host, KernelNfConntrackTableFull, "", now, 240);
            } else if body.contains(" invoked oom-killer: ") {
                self.counter(host, KernelOomCount, "");
            }
        }
        if self.hosts[host].vpsadminos && (program == "lxc" || program == "lxc-start") {
            let family =
                if body.contains("No space left on device - Failed to unshare CLONE_NEWNET") {
                    Some(LxcStartNetnsLimit)
                } else if SPAWN.is_match(body) {
                    Some(LxcStartFailed)
                } else {
                    None
                };
            if let Some(family) = family {
                let id = match body.find(':') {
                    Some(0) => body,
                    Some(colon) => &body[..colon],
                    None => "",
                }
                .trim_matches(['\0', ' ', '\t', '\n', '\u{b}', '\u{c}', '\r']);
                self.flare(host, family, id, now, 240);
            }
        }
        self.counter(host, MessageCount, program);
        if self.hosts[host].vpsadminos && program == "nodectld" {
            let family = if body.contains("Daemon crashed") {
                Some(NodectldCrash)
            } else if body.contains("[BUG] Segmentation fault") {
                Some(NodectldSegfault)
            } else if body.contains("cannot receive incremental stream") {
                Some(NodectldZfsStreamReceiveError)
            } else {
                None
            };
            if let Some(family) = family {
                self.flare(host, family, "", now, 180);
            }
        }
        if self.hosts[host].vpsadminos && program == "osctld" {
            if let Some(capture) = COMMAND.captures(body) {
                self.counter(host, OsctldCommandCount, &capture[1]);
            } else if body.contains("Error: internal error") {
                self.flare(host, OsctldInternalError, "", now, 180);
            }
        }
        if body.contains("flare test") {
            self.flare(host, FlareTest, "", now, 15);
        }
        if self.hosts[host].webui
            && program == "nginx"
            && body.contains("FastCGI sent in stderr")
            && let Some((key, _)) = WEB.iter().find(|(_, signature)| body.contains(signature))
        {
            self.counter(host, VpsadminWebuiFatalErrorCount, key);
            self.flare(host, VpsadminWebuiFatalError, key, now, 180);
        }
        if program == "kernel" && (body.contains("PANIC: zfs:") || body.contains("spl_panic+0x")) {
            self.flare(host, ZfsPanic, "", now, 240);
        }
    }
}
