use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time;

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
pub struct IpInfo {
    pub ip: String,
    pub port: u16,
    pub protocol: Protocol,
    pub banner: Option<String>,
}

impl IpInfo {
    async fn try_get_response(stream: &mut TcpStream, message: Option<&str>) -> Option<String> {
        if let Some(message) = message {
            stream.write_all(message.as_bytes()).await.ok()?;
        }
        let mut buf = vec![0; 1024];
        match time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => Some(String::from_utf8_lossy(&buf[..n]).to_string()),
            _ => None,
        }
    }
    async fn enrich(&mut self) {
        let messages = [None, Some("GET / HTTP/1.0\r\n\r\n")];
        let addr = format!("{}:{}", self.ip, self.port);
        info!("[{}] attempting to enrich ", addr);
        match time::timeout(Duration::from_secs(5), TcpStream::connect(&addr)).await {
            Ok(Ok(mut stream)) => {
                for message in messages {
                    self.banner =
                        self.banner
                            .take()
                            .or(Self::try_get_response(&mut stream, message).await);
                }
            }
            _ => {
                warn!("[{}] failed to connect", addr)
            }
        };
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanResult(Vec<IpInfo>);

impl std::ops::Deref for ScanResult {
    type Target = Vec<IpInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for ScanResult {
    type Item = IpInfo;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Into<Vec<IpInfo>> for ScanResult {
    fn into(self) -> Vec<IpInfo> {
        self.0
    }
}

impl From<Vec<IpInfo>> for ScanResult {
    fn from(value: Vec<IpInfo>) -> Self {
        return Self(value);
    }
}

impl ScanResult {
    pub fn save_result(&self, path: &str) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_result(path: &str) -> anyhow::Result<Self> {
        let data = fs::read_to_string(path).context("failed to read json file")?;
        let parsed = serde_json::from_str(&data).context("failed to parse json file")?;
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
            .collect::<Vec<IpInfo>>()
            .into()
    }

    /// populate scan results using masscan
    pub async fn populate(
        self,
        ip_range: &str,
        port_range: &str,
        masscan_rate: usize,
    ) -> Result<Self> {
        info!(
            "running masscan over '{}', ports '{}' with rate: {}",
            ip_range, port_range, masscan_rate
        );
        let output = Command::new("sudo")
            .arg("masscan")
            .arg(ip_range)
            .arg("-p")
            .arg(port_range)
            .arg("--rate")
            .arg(masscan_rate.to_string())
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
                results.push(IpInfo {
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

    /// enrich scan results by grabbing banners
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
