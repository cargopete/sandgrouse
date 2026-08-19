//! Config loading. `sandgrouse.toml` describes who is on the line and what they read;
//! `silo.toml` is the canon they all believe.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub room: RoomCfg,
    pub wire: WireCfg,
    #[serde(rename = "agent")]
    pub agents: Vec<AgentCfg>,
}

#[derive(Debug, Deserialize)]
pub struct RoomCfg {
    pub news_beat_secs: (u64, u64),
    pub ambient_beat_secs: (u64, u64),
    pub max_messages_per: usize,
    pub window_secs: u64,
    pub agent_cooldown_secs: u64,
    pub min_gap_secs: u64,
    pub reply_delay_secs: (u64, u64),
    pub context_messages: usize,
    pub llm_timeout_secs: u64,
    pub max_concurrent_llm: usize,
}

#[derive(Debug, Deserialize)]
pub struct WireCfg {
    pub poll_secs: u64,
    pub user_agent: String,
    pub timeout_secs: u64,
    pub max_item_age_hours: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentCfg {
    pub name: String,
    pub level: u16,
    pub model: String,
    pub role: String,
    pub beat: String,
    pub persona: String,
    pub feeds: Vec<FeedCfg>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FeedCfg {
    pub name: String,
    pub url: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing {}", path.display()))?;
        anyhow::ensure!(!cfg.agents.is_empty(), "no agents configured; the line would be quiet");
        for a in &cfg.agents {
            anyhow::ensure!(!a.feeds.is_empty(), "{} has no feeds", a.name);
        }
        Ok(cfg)
    }
}
