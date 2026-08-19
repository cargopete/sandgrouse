//! The canon. Loaded once, rendered to prose, and pasted into every agent's system
//! prompt so that when Mark mentions Old Fenwick, Gemma knows exactly who he means.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Silo {
    pub station: Station,
    #[serde(rename = "system", default)]
    pub systems: Vec<SystemEntry>,
    #[serde(rename = "resident", default)]
    pub residents: Vec<Resident>,
    #[serde(rename = "situation", default)]
    pub situations: Vec<Situation>,
    #[serde(rename = "extra", default)]
    pub extras: Vec<Extra>,
}

/// The rest of the two hundred and thirteen, in one block of shorthand. Not everybody
/// needs a paragraph; most of a silo is background, and a name with a level and a trade
/// is enough for somebody to be mentioned in passing without four agents inventing four
/// different versions of them.
#[derive(Debug, Deserialize)]
pub struct Extra {
    pub roll: String,
}

#[derive(Debug, Deserialize)]
pub struct Station {
    pub name: String,
    pub designation: String,
    pub located: String,
    pub sealed: String,
    pub levels: u16,
    pub population: u32,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct SystemEntry {
    pub name: String,
    #[serde(rename = "where")]
    pub location: String,
    pub what: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Resident {
    pub name: String,
    pub level: u16,
    pub role: String,
    pub note: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Situation {
    pub tag: String,
    pub text: String,
}

impl Silo {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
    }

    /// The canon as one block of prose, for the system prompt. Deliberately written
    /// as briefing rather than as data: agents inhabit a paragraph better than a table.
    pub fn brief(&self) -> String {
        let s = &self.station;
        let mut out = String::new();
        out.push_str(&format!(
            "WHERE YOU ARE\n\n{} ({}), {}. Sealed {}. {} levels. {} people.\n\n{}\n",
            s.name,
            s.designation,
            s.located,
            s.sealed,
            s.levels,
            s.population,
            s.summary.trim()
        ));

        out.push_str("\nTHE MACHINERY\n\n");
        for sys in &self.systems {
            out.push_str(&format!("- {} ({}): {}\n", sys.name, sys.location, sys.what));
        }

        out.push_str("\nPEOPLE YOU ALL KNOW\n\nThese are real residents. Everyone on the line \
                      knows them. Refer to them naturally, as colleagues, never as \
                      introductions. Do not invent new named residents unless you must, and \
                      if you must, keep them minor and keep them consistent.\n\n");
        for r in &self.residents {
            out.push_str(&format!("- {} (L-{}, {}): {}\n", r.name, r.level, r.role, r.note));
        }

        if !self.extras.is_empty() {
            out.push_str(
                "\nTHE REST OF THE ROLL\n\nBackground. Mention them in passing, by name and \
                 level, the way you would mention somebody you have shared a stairwell with \
                 for thirty-one years. Do not give them scenes.\n\n",
            );
            for e in &self.extras {
                out.push_str(e.roll.trim());
                out.push('\n');
            }
        }

        out.push_str("\nWHAT IS GOING ON AT THE MOMENT\n\n");
        for sit in &self.situations {
            out.push_str(&format!("- {}\n", sit.text));
        }
        out
    }
}
