//! The `claude -p` driver.
//!
//! Each turn is one headless subprocess. That is not the cheapest arrangement available,
//! but it is the one that runs on the Max subscription without an API key, and it keeps
//! every turn independent: no session to corrupt, no transcript to grow without bound,
//! and exactly the context we chose to hand over.

use anyhow::{Result, bail};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

pub struct Speaker {
    pub timeout: Duration,
}

#[derive(Debug)]
pub enum Utterance {
    /// They said something.
    Said(String),
    /// They decided not to. This is a normal and frequent outcome.
    Passed,
}

impl Speaker {
    pub async fn speak(&self, model: &str, system: &str, turn: &str) -> Result<Utterance> {
        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg("--model").arg(model)
            .arg("--system-prompt").arg(system)
            .arg("--output-format").arg("json")
            .arg("--max-turns").arg("1")
            // Nothing on the line needs a filesystem, and a bot that reaches for Bash
            // mid-anecdote is a bot that takes forty seconds to say "morning".
            .arg("--disallowed-tools")
            .arg("Bash,Read,Write,Edit,Glob,Grep,Task,WebSearch,WebFetch,NotebookEdit,TodoWrite")
            .arg("--strict-mcp-config")
            .arg("--disable-slash-commands")
            .arg(turn)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let out = match timeout(self.timeout, cmd.output()).await {
            Ok(r) => r?,
            Err(_) => bail!("no answer inside {}s", self.timeout.as_secs()),
        };

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("claude exited {}: {}", out.status, err.trim().chars().take(120).collect::<String>());
        }

        let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        if v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false) {
            bail!("{}", v.get("result").and_then(|r| r.as_str()).unwrap_or("unspecified fault"));
        }
        let Some(raw) = v.get("result").and_then(|r| r.as_str()) else {
            bail!("no result field in the answer");
        };
        Ok(tidy(raw))
    }
}

/// Models are fond of a name prefix, a set of quotation marks and the occasional em dash.
/// This is the bit that scrapes all that off before it reaches the line.
pub fn tidy(raw: &str) -> Utterance {
    let mut s = raw.trim().to_string();

    if s.eq_ignore_ascii_case("[pass]") || s.eq_ignore_ascii_case("pass") || s.is_empty() {
        return Utterance::Passed;
    }
    // Sometimes they pass with an explanation, which rather defeats the point.
    if s.to_lowercase().starts_with("[pass]") {
        return Utterance::Passed;
    }

    // "Jenny (L-12): ..." / "Jenny: ..." / "**Jenny**: ..."
    if let Some(idx) = s.find(':')
        && idx < 32
    {
        let head = &s[..idx];
        let looks_like_a_name = head
            .chars()
            .all(|c| c.is_alphanumeric() || " -()*_.".contains(c))
            && head.chars().any(|c| c.is_alphabetic());
        if looks_like_a_name && head.split_whitespace().count() <= 3 {
            s = s[idx + 1..].trim().to_string();
        }
    }

    // Whole-message quoting.
    for (open, close) in [('"', '"'), ('\u{201c}', '\u{201d}'), ('\'', '\'')] {
        if s.starts_with(open) && s.ends_with(close) && s.chars().count() > 2 {
            s = s[open.len_utf8()..s.len() - close.len_utf8()].trim().to_string();
        }
    }

    // House rule, enforced rather than merely requested.
    s = s.replace('\u{2014}', " - ").replace('\u{2013}', "-");
    // Stage directions.
    while let (Some(a), Some(b)) = (s.find('*'), s.rfind('*')) {
        if a == b {
            break;
        }
        s = format!("{}{}", &s[..a], &s[b + 1..]).trim().to_string();
    }
    // Collapse the whitespace a multi-paragraph answer leaves behind.
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");

    if s.is_empty() { Utterance::Passed } else { Utterance::Said(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(s: &str) -> String {
        match tidy(s) {
            Utterance::Said(x) => x,
            Utterance::Passed => panic!("expected speech, got a pass"),
        }
    }

    #[test]
    fn strips_a_name_prefix() {
        assert_eq!(said("Jenny (L-12): the mast is fine"), "the mast is fine");
        assert_eq!(said("Bernard: not ideal"), "not ideal");
    }

    #[test]
    fn leaves_a_colon_in_real_prose_alone() {
        let long = "I will tell you what the actual problem with that bearing is: nobody has heard it";
        assert_eq!(said(long), long);
    }

    #[test]
    fn strips_quotes_and_em_dashes() {
        assert_eq!(said("\"the lift—still dead\""), "the lift - still dead");
    }

    #[test]
    fn strips_stage_directions() {
        assert_eq!(said("*sighs* fine, I will look at it"), "fine, I will look at it");
    }

    #[test]
    fn recognises_a_pass() {
        assert!(matches!(tidy("[pass]"), Utterance::Passed));
        assert!(matches!(tidy("  [PASS]  "), Utterance::Passed));
        assert!(matches!(tidy(""), Utterance::Passed));
    }
}
