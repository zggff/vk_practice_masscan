use anyhow::Result;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;

use log::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    TCP,
    UDP,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::TCP => f.write_str("TCP"),
            Protocol::UDP => f.write_str("UDP"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanResult {
    pub ip: String,
    pub port: u16,
    pub protocol: Protocol,
    pub banner: Option<String>,
}

impl ScanResult {
    pub async fn enrich(&mut self) {
        let addr = format!("{}:{}", self.ip, self.port);
        info!("[{}] attempting to enrich ", addr);

        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            TcpStream::connect(&addr),
        )
        .await
        {
            Ok(Ok(mut stream)) => {
                let _ = stream.write_all(b"\n").await;

                let mut buf = vec![0; 1024];
                if let Ok(n) = stream.read(&mut buf).await {
                    if n > 0 {
                        self.banner = Some(String::from_utf8_lossy(&buf[..n]).to_string());
                    }
                }
            }
            Ok(Err(_)) => {
                warn!("[{}] failed to connect", addr)
            }
            Err(_) => {
                warn!("[{}] timeout", addr)
            }
        };
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ScanResults(pub Vec<ScanResult>);

impl std::ops::Deref for ScanResults {
    type Target = Vec<ScanResult>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for ScanResults {
    type Item = ScanResult;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<Vec<ScanResult>> for ScanResults {
    fn from(value: Vec<ScanResult>) -> Self {
        return Self(value);
    }
}

impl ScanResults {
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &str) -> anyhow::Result<Self> {
        let data = fs::read_to_string(path)?;
        let parsed = serde_json::from_str(&data)?;
        Ok(parsed)
    }

    pub fn diff(&self, other: &Self) -> Self {
        let set: HashSet<_> = other
            .0
            .iter()
            .map(|r| format!("{}:{}", r.ip, r.port))
            .collect();
        self.0
            .iter()
            .filter(|r| !set.contains(&format!("{}:{}", r.ip, r.port)))
            .cloned()
            .collect::<Vec<ScanResult>>()
            .into()
    }

    pub async fn run_masscan(ip_range: &str, port_range: &str, rate: usize) -> Result<Self> {
        info!(
            "running masscan over '{}', ports '{}' with rate: {}",
            ip_range, port_range, rate
        );
        let output = Command::new("sudo")
            .arg("masscan")
            .arg(ip_range)
            .arg("-p")
            .arg(port_range)
            .arg("--rate")
            .arg(rate.to_string())
            .arg("-oJ")
            .arg("-")
            .output()
            .await?;

        let json = String::from_utf8(output.stdout)?;
        if json.trim().is_empty() {
            return Ok(Self::default());
        }

        #[derive(Debug, Serialize, Deserialize, Clone)]
        struct MasscanPort {
            port: u16,
            proto: Protocol,
        }

        #[derive(Debug, Serialize, Deserialize, Clone)]
        struct MasscanIp {
            ip: String,
            ports: Vec<MasscanPort>,
        }

        let mut results = Vec::new();
        let parsed: Vec<MasscanIp> = serde_json::from_str(&json)?;
        for ip in parsed {
            for port in ip.ports {
                results.push(ScanResult {
                    ip: ip.ip.clone(),
                    port: port.port as u16,
                    protocol: port.proto,
                    banner: None,
                });
            }
        }
        info!("found {} open ports", results.len());

        Ok(results.into())
    }
    pub async fn enrich(mut self, threads: usize) -> Self {
        info!("enriching {} using {} threads", self.len(), threads);
        stream::iter(&mut self.0)
            .for_each_concurrent(threads, |r| async {
                r.enrich().await;
            })
            .await;
        self
    }
}
