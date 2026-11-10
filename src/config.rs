use serde::Deserialize;
use serde_json::Map;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub syslog_pipe: PathBuf,
    #[serde(default = "pipe_size")]
    pub pipe_size: i32,
    #[serde(default = "line_limit")]
    pub max_line_bytes: usize,
    pub hosts: Map<String, serde_json::Value>,
}
fn pipe_size() -> i32 {
    1024 * 1024
}
fn line_limit() -> usize {
    1024 * 1024
}
#[derive(Debug, Deserialize)]
pub struct Host {
    pub alias: String,
    pub fqdn: String,
    pub os: String,
    #[serde(default)]
    pub collectors: Vec<String>,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let config: Self =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.syslog_pipe.as_os_str().is_empty()
            || self.pipe_size <= 0
            || self.max_line_bytes == 0
        {
            return Err(
                "syslog_pipe must be nonempty; pipe_size and max_line_bytes must be positive"
                    .into(),
            );
        }
        for (name, value) in &self.hosts {
            let host: Host =
                serde_json::from_value(value.clone()).map_err(|e| format!("host {name}: {e}"))?;
            if name.is_empty()
                || host.alias.is_empty()
                || host.fqdn.is_empty()
                || host.os.is_empty()
            {
                return Err(format!(
                    "host {name}: name, alias, fqdn and os must be nonempty"
                ));
            }
        }
        Ok(())
    }
    pub fn host_entries(&self) -> impl Iterator<Item = (&str, Host)> {
        self.hosts.iter().map(|(name, host)| {
            (
                name.as_str(),
                serde_json::from_value(host.clone()).expect("validated host"),
            )
        })
    }
}
