//! The transcript. What was said, by whom, from which level.

use chrono::{DateTime, Local};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Author {
    /// One of the residents on the line.
    Resident { name: String, level: u16 },
    /// You.
    Chief,
    /// The station itself: wire faults, joins, notices. Not a person.
    Station,
}

impl Author {
    pub fn name(&self) -> &str {
        match self {
            Author::Resident { name, .. } => name,
            Author::Chief => "Chief",
            Author::Station => "CROW",
        }
    }
    pub fn level(&self) -> Option<u16> {
        match self {
            Author::Resident { level, .. } => Some(*level),
            Author::Chief => Some(38),
            Author::Station => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Carries an item off the wire.
    Wire,
    /// Life in the silo: work, gossip, musing, complaint.
    Ambient,
    /// Answering somebody.
    Reply,
    /// Station notice.
    Notice,
}

#[derive(Debug, Clone)]
pub struct Citation {
    pub source: String,
    pub title: String,
    /// Not shown on the line - a URL is unreadable out loud and unclickable in a
    /// terminal - but kept so the log book can carry it later.
    #[allow(dead_code)]
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: u64,
    pub author: Author,
    pub at: DateTime<Local>,
    pub kind: Kind,
    pub body: String,
    pub cite: Option<Citation>,
}

#[derive(Default)]
pub struct Room {
    pub messages: Vec<Message>,
    next_id: u64,
    /// The log book. An evening on the party line is worth keeping, and a transcript
    /// on disk is also the only way to audit what the room actually did while you
    /// were asleep in front of it.
    book: Option<std::path::PathBuf>,
}

impl Room {
    pub fn keep_a_log(&mut self, path: std::path::PathBuf) {
        self.book = Some(path);
    }

    /// One line per message, appended as it lands. Deliberately append-only and
    /// deliberately best-effort: a full disk should cost you the log, not the evening.
    fn record(&self, m: &Message) {
        let Some(path) = &self.book else { return };
        use std::io::Write;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let lvl = m.author.level().map(|l| format!(" L-{l}")).unwrap_or_default();
        let cite = m.cite.as_ref().map(|c| format!("  [{} · {}]", c.source, c.title)).unwrap_or_default();
        let line = format!("{} {}{}: {}{}\n", m.at.format("%Y-%m-%d %H:%M:%S"), m.author.name(), lvl, m.body, cite);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn push(&mut self, author: Author, kind: Kind, body: String, cite: Option<Citation>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let m = Message { id, author, at: Local::now(), kind, body, cite };
        self.record(&m);
        self.messages.push(m);
        id
    }

    /// Messages in the trailing window, for the pacing floor.
    pub fn count_since(&self, cutoff: DateTime<Local>) -> usize {
        self.messages.iter().filter(|m| m.at >= cutoff && m.author != Author::Station).count()
    }

    pub fn last_spoke(&self, name: &str) -> Option<DateTime<Local>> {
        self.messages.iter().rev().find(|m| m.author.name() == name).map(|m| m.at)
    }

    /// The transcript window an agent is shown, rendered the way it appears on the line.
    pub fn context(&self, n: usize) -> String {
        let start = self.messages.len().saturating_sub(n);
        self.messages[start..]
            .iter()
            .filter(|m| m.author != Author::Station)
            .map(|m| {
                let lvl = m.author.level().map(|l| format!(" L-{l}")).unwrap_or_default();
                let cite = m
                    .cite
                    .as_ref()
                    .map(|c| format!("  [off the wire: \"{}\" — {}]", c.title, c.source))
                    .unwrap_or_default();
                format!("[{}] {}{}: {}{}", m.at.format("%H:%M"), m.author.name(), lvl, m.body, cite)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
