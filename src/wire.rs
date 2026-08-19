//! The mast. Pulls RSS and Atom off whatever is left of the satellites, strips the
//! markup, and hands back items nobody on the line has seen before.
//!
//! A feed that answers 200 with nothing in it is not a healthy feed, so the fault
//! model here distinguishes "quiet" from "answered with nothing", and the roster
//! shows the difference.

use crate::config::{AgentCfg, WireCfg};
use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Item {
    pub source: String,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub published: Option<DateTime<Utc>>,
    /// Stable identity for the seen-set: the entry id, or the link, or the title.
    pub key: String,
}

/// What one poll of one agent's feeds produced.
#[derive(Debug)]
pub struct WireReport {
    /// Whose poll this was. Carried for the log and for anyone debugging a report
    /// in isolation; the event loop already knows from the index.
    #[allow(dead_code)]
    pub agent: String,
    pub fresh: Vec<Item>,
    pub ok_feeds: usize,
    pub dead_feeds: Vec<(String, String)>,
    /// The seen-set, travelling home. It is moved into the poll rather than shared,
    /// so if it is not returned here it is lost, and a lost seen-set means every
    /// subsequent poll cheerfully re-announces the entire wire as breaking news.
    pub seen: HashSet<String>,
}

pub struct Mast {
    client: reqwest::Client,
    max_age: Duration,
}

impl Mast {
    pub fn new(cfg: &WireCfg) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(cfg.user_agent.clone())
            .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .build()?;
        Ok(Self { client, max_age: Duration::hours(cfg.max_item_age_hours) })
    }

    pub async fn poll(&self, agent: &AgentCfg, mut seen: HashSet<String>) -> WireReport {
        let mut fresh = Vec::new();
        let mut dead = Vec::new();
        let mut ok = 0usize;
        let cutoff = Utc::now() - self.max_age;

        for feed in &agent.feeds {
            match self.fetch(&feed.name, &feed.url).await {
                Ok(items) if items.is_empty() => {
                    // Answered, and had nothing to say. That is a fault, not a calm.
                    dead.push((feed.name.clone(), "answered with no items".into()));
                }
                Ok(items) => {
                    ok += 1;
                    for it in items {
                        if let Some(p) = it.published
                            && p < cutoff
                        {
                            continue;
                        }
                        // Two keys, both required. The entry id stops the same item
                        // arriving twice off the same feed; the normalised headline
                        // stops the BBC, the Guardian and Reuters each reporting the
                        // same election on the same evening as though it were three.
                        let story = story_key(&it.title);
                        if seen.contains(&it.key) || seen.contains(&story) {
                            continue;
                        }
                        seen.insert(it.key.clone());
                        seen.insert(story);
                        fresh.push(it);
                    }
                }
                Err(e) => dead.push((feed.name.clone(), short_err(&e))),
            }
        }

        // Newest first, so the freshest thing is the likeliest to get picked up.
        fresh.sort_by(|a, b| b.published.cmp(&a.published));
        WireReport { agent: agent.name.clone(), fresh, ok_feeds: ok, dead_feeds: dead, seen }
    }

    async fn fetch(&self, source: &str, url: &str) -> Result<Vec<Item>> {
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            bail!("HTTP {}", resp.status().as_u16());
        }
        let bytes = resp.bytes().await?;
        // Some feeds declare utf-8 and then post a stray latin-1 byte in somebody's
        // surname. Try them honestly first, then forgive them.
        let feed = match feed_rs::parser::parse(&bytes[..]) {
            Ok(f) => f,
            Err(_) => {
                let lossy = String::from_utf8_lossy(&bytes);
                feed_rs::parser::parse(lossy.as_bytes())?
            }
        };

        Ok(feed
            .entries
            .into_iter()
            .map(|e| {
                let title = e.title.map(|t| clean(&t.content)).unwrap_or_default();
                let url = e.links.first().map(|l| l.href.clone()).unwrap_or_default();
                let summary = e
                    .summary
                    .map(|s| clean(&s.content))
                    .or_else(|| e.content.and_then(|c| c.body).map(|b| clean(&b)))
                    .unwrap_or_default();
                let key = if !e.id.is_empty() {
                    e.id.clone()
                } else if !url.is_empty() {
                    url.clone()
                } else {
                    format!("{source}:{title}")
                };
                Item {
                    source: source.to_string(),
                    title,
                    summary: truncate(&summary, 600),
                    url,
                    published: e.published.or(e.updated),
                    key,
                }
            })
            .filter(|i| !i.title.is_empty())
            .collect())
    }
}

/// Strip tags and unescape the handful of entities feeds actually use. Feeds are
/// full of markup that has no business being read aloud on an intercom.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&hellip;", "...")
        .replace("&mdash;", " - ")
        .replace("&ndash;", "-");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A rough identity for "the same story". Lowercase, punctuation gone, the little
/// words gone, first six substantial words joined. Crude, and crude is correct here:
/// the cost of a false match is one skipped headline out of several hundred, and the
/// cost of a miss is the same event read out three times in five minutes.
fn story_key(title: &str) -> String {
    const NOISE: &[&str] = &[
        "the", "a", "an", "of", "in", "on", "at", "to", "for", "and", "or", "as", "is",
        "are", "was", "were", "be", "by", "with", "from", "that", "this", "it", "its",
        "after", "over", "into", "says", "say", "said", "new",
    ];
    let words: Vec<String> = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| w.len() > 2 && !NOISE.contains(w))
        .take(6)
        .map(|w| w.to_string())
        .collect();
    if words.len() < 3 {
        // Too little to match on. Fall back to the whole headline rather than
        // colliding every short title with every other short title.
        return format!("story:{}", title.to_lowercase());
    }
    format!("story:{}", words.join("-"))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    match cut.rfind(' ') {
        Some(i) => format!("{}...", &cut[..i]),
        None => cut,
    }
}

fn short_err(e: &anyhow::Error) -> String {
    let s = e.to_string();
    s.chars().take(48).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_markup_and_entities() {
        let got = clean("<p>Cost &amp; effect <a href=\"x\">here</a></p>\n  spaced");
        assert_eq!(got, "Cost & effect here spaced");
    }

    #[test]
    fn same_story_from_two_papers_shares_a_key() {
        let a = story_key("EU agrees on 2040 climate target after marathon session");
        let b = story_key("The EU agrees on a 2040 climate target after a marathon session");
        assert_eq!(a, b);
    }

    #[test]
    fn different_stories_do_not_collide() {
        assert_ne!(
            story_key("Rust 1.98 released with new borrow checker diagnostics"),
            story_key("Generator two bearing fails at Crow's Heart")
        );
    }

    #[test]
    fn short_titles_fall_back_to_the_whole_headline() {
        assert_ne!(story_key("Budget day"), story_key("Election day"));
    }

    #[test]
    fn truncate_breaks_on_a_word() {
        assert_eq!(truncate("one two three", 7), "one...");
        assert_eq!(truncate("short", 40), "short");
    }
}
