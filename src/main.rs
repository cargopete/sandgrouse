//! Sandgrouse.
//!
//! A party line between the levels of a buried silo, staffed by four people who do not
//! know they are models. They read the surface off a mast, argue about it, gossip about
//! two hundred and nine colleagues, and occasionally say something true about being
//! buried. You are on Level 38 and you have a handset.
//!
//! The name is the bird: a sandgrouse flies fifty miles across the Empty Quarter for
//! water, soaks it into its belly feathers, and carries it back to chicks waiting on the
//! ground. Which is roughly the arrangement here.

mod config;
mod director;
mod llm;
mod persona;
mod room;
mod silo;
mod tui;
mod wire;

use anyhow::{Context, Result};
use config::Config;
use director::{Action, Director, Turn};
use llm::{Speaker, Utterance};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use room::{Author, Citation, Kind, Room};
use silo::Silo;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tui::draw::{Lamp, Metrics, Scroll, Station, View};

/// Everything the event loop needs to hear about.
enum Signal {
    Key(KeyEvent),
    /// An agent finished a turn.
    Spoke { idx: usize, out: Result<Utterance>, kind: Kind, cite: Option<Citation> },
    /// The mast came back.
    Wire { idx: usize, report: wire::WireReport },
    Redraw,
}

struct Agent {
    cfg: config::AgentCfg,
    system: String,
    wires_ok: usize,
    wires_total: usize,
    seen: HashSet<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--line-test") {
        return line_test().await;
    }
    if std::env::args().any(|a| a == "--panel-test") {
        return panel_test();
    }
    let root = std::env::var("SANDGROUSE_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let cfg = Config::load(&root.join("sandgrouse.toml"))?;
    let canon = Silo::load(&root.join("silo.toml"))?;

    which_claude().context("sandgrouse drives the `claude` CLI; it is not on PATH")?;

    let roster: Vec<(String, u16, String)> =
        cfg.agents.iter().map(|a| (a.name.clone(), a.level, a.beat.clone())).collect();

    let mut agents: Vec<Agent> = cfg
        .agents
        .iter()
        .map(|a| Agent {
            system: persona::system_prompt(a, &canon, &roster),
            wires_total: a.feeds.len(),
            wires_ok: 0,
            seen: HashSet::new(),
            cfg: a.clone(),
        })
        .collect();

    let mast = Arc::new(wire::Mast::new(&cfg.wire)?);
    let speaker = Arc::new(Speaker { timeout: std::time::Duration::from_secs(cfg.room.llm_timeout_secs) });
    let gate = Arc::new(Semaphore::new(cfg.room.max_concurrent_llm));
    let context_n = cfg.room.context_messages;
    let poll_secs = cfg.wire.poll_secs;

    let situations = canon.situations.clone();
    let min_gap = cfg.room.min_gap_secs;
    let mut director = Director::new(&cfg.agents, cfg.room, situations.clone(), poll_secs, min_gap);
    let mut room = Room::default();
    room.keep_a_log(log_book());
    room.push(
        Author::Station,
        Kind::Notice,
        "circuit open · the mast is warming up · give them a few minutes".into(),
        None,
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<Signal>();

    // Terminal input on its own thread. crossterm's read blocks, and blocking the
    // runtime would stall every agent mid-sentence.
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            loop {
                match event::read() {
                    Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                        if tx.send(Signal::Key(k)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Resize(_, _)) => {
                        if tx.send(Signal::Redraw).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
    }

    let mut terminal = ratatui::init();
    let mut input = String::new();
    let mut scroll = Scroll::Live;
    let mut metrics = Metrics::default();
    let mut last_fault: Option<String> = None;
    let mut situation_idx = 0usize;
    let mut polling = 0usize;
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));

    let result: Result<()> = loop {
        // ── draw ───────────────────────────────────────────────────────────────
        let stations: Vec<Station> = agents
            .iter()
            .enumerate()
            .map(|(i, a)| Station {
                name: a.cfg.name.clone(),
                level: a.cfg.level,
                model: a.cfg.model.clone(),
                lamp: if director.is_busy(i) {
                    Lamp::Working
                } else if a.wires_total > 0 && a.wires_ok == 0 {
                    Lamp::Cut
                } else if room
                    .last_spoke(&a.cfg.name)
                    .is_some_and(|t| chrono::Local::now() - t < chrono::Duration::seconds(75))
                {
                    Lamp::Cooling
                } else {
                    Lamp::Ready
                },
                stock: director.stock_len(i),
                wires_ok: a.wires_ok,
                wires_total: a.wires_total,
            })
            .collect();

        let current = situations.get(situation_idx % situations.len().max(1));
        let situation = current.map(|s| s.text.clone()).unwrap_or_default();
        let situation_tag = current.map(|s| s.tag.clone()).unwrap_or_default();

        let mut measured = metrics;
        if let Err(e) = terminal.draw(|f| {
            measured = tui::draw::draw(
                f,
                &View {
                    room: &room,
                    stations: &stations,
                    input: &input,
                    scroll,
                    situation: &situation,
                    situation_tag: &situation_tag,
                    polling: polling > 0,
                    last_fault: last_fault.as_deref(),
                },
            )
        }) {
            break Err(e.into());
        }
        metrics = measured;

        // ── wait ───────────────────────────────────────────────────────────────
        tokio::select! {
            _ = ticker.tick() => {
                for action in director.tick(&room) {
                    match action {
                        Action::Poll(i) => {
                            polling += 1;
                            let mast = mast.clone();
                            let tx = tx.clone();
                            let acfg = agents[i].cfg.clone();
                            // The seen-set travels with the task and comes home on the
                            // report, so it is never read and written at the same time.
                            let seen = std::mem::take(&mut agents[i].seen);
                            tokio::spawn(async move {
                                let report = mast.poll(&acfg, seen).await;
                                let _ = tx.send(Signal::Wire { idx: i, report });
                            });
                        }
                        Action::Speak(i, turn) => {
                            let transcript = room.context(context_n);
                            let (prompt, kind, cite) = match &turn {
                                Turn::Wire(item) => (
                                    persona::wire_turn(item, &transcript),
                                    Kind::Wire,
                                    Some(Citation {
                                        source: item.source.clone(),
                                        title: item.title.clone(),
                                        url: item.url.clone(),
                                    }),
                                ),
                                Turn::Ambient(seed) => (persona::ambient_turn(seed, &transcript), Kind::Ambient, None),
                                Turn::Reply { to } => (persona::reply_turn(&transcript, to), Kind::Reply, None),
                            };
                            let speaker = speaker.clone();
                            let gate = gate.clone();
                            let tx = tx.clone();
                            let model = agents[i].cfg.model.clone();
                            let system = agents[i].system.clone();
                            tokio::spawn(async move {
                                let _permit = gate.acquire().await;
                                let out = speaker.speak(&model, &system, &prompt).await;
                                let _ = tx.send(Signal::Spoke { idx: i, out, kind, cite });
                            });
                        }
                    }
                }
                // Rotate the situation on the sidebar every few minutes so the running
                // storylines are visible rather than only implied.
                if chrono::Local::now().timestamp() % 300 == 0 { situation_idx += 1; }
            }

            Some(sig) = rx.recv() => match sig {
                Signal::Redraw => {}

                Signal::Key(k) => {
                    match (k.code, k.modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => break Ok(()),
                        (KeyCode::Enter, _) if !input.trim().is_empty() => {
                            let body = input.trim().to_string();
                            input.clear();
                            // Saying something is a commitment to seeing the answer.
                            scroll = Scroll::Live;
                            let id = room.push(Author::Chief, Kind::Ambient, body, None);
                            let msg = room.messages.iter().find(|m| m.id == id).cloned();
                            if let Some(m) = msg { director.on_message(&m, &room); }
                        }
                        (KeyCode::Backspace, _) => { input.pop(); }
                        (KeyCode::Up, _)       => scroll = nudge(metrics, -1),
                        (KeyCode::Down, _)     => scroll = nudge(metrics, 1),
                        (KeyCode::PageUp, _)   => scroll = nudge(metrics, -(metrics.viewport as isize - 2).max(1)),
                        (KeyCode::PageDown, _) => scroll = nudge(metrics, (metrics.viewport as isize - 2).max(1)),
                        (KeyCode::Home, _)     => scroll = Scroll::Held(0),
                        (KeyCode::End, _)      => scroll = Scroll::Live,
                        (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => input.push(c),
                        _ => {}
                    }
                }

                Signal::Wire { idx, mut report } => {
                    polling = polling.saturating_sub(1);
                    agents[idx].wires_ok = report.ok_feeds;
                    if let Some((name, why)) = report.dead_feeds.first() {
                        last_fault = Some(format!("{name}: {why}"));
                    }
                    agents[idx].seen = std::mem::take(&mut report.seen);
                    let n = report.fresh.len();
                    director.deliver_wire(idx, report.fresh);
                    if n > 0 && room.messages.len() == 1 {
                        room.push(Author::Station, Kind::Notice,
                            format!("mast has a signal · {n} items down the wire"), None);
                    }
                }

                Signal::Spoke { idx, out, kind, cite } => {
                    director.set_busy(idx, false);
                    match out {
                        Ok(Utterance::Said(body)) => {
                            let author = Author::Resident {
                                name: agents[idx].cfg.name.clone(),
                                level: agents[idx].cfg.level,
                            };
                            let cite = if kind == Kind::Wire { cite } else { None };
                            let id = room.push(author, kind, body, cite);
                            let msg = room.messages.iter().find(|m| m.id == id).cloned();
                            if let Some(m) = msg { director.on_message(&m, &room); }
                        }
                        Ok(Utterance::Passed) => {}
                        Err(e) => {
                            last_fault = Some(format!("{}: {}", agents[idx].cfg.name,
                                e.to_string().chars().take(40).collect::<String>()));
                        }
                    }
                }
            }
        }
    };

    ratatui::restore();
    result
}

/// Where the evening goes. `SANDGROUSE_LOGBOOK` overrides it; otherwise one file per
/// day under the state directory, which is where a log book belongs.
fn log_book() -> PathBuf {
    if let Ok(p) = std::env::var("SANDGROUSE_LOGBOOK") {
        return PathBuf::from(p);
    }
    let base = directories::ProjectDirs::from("", "", "sandgrouse")
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(format!("logbook-{}.txt", chrono::Local::now().format("%Y-%m-%d")))
}

/// Move the view by `delta` lines, clamped, and re-engage following the moment it
/// arrives back at the foot. Scrolling to the bottom means "show me what happens next",
/// and having to press End as well to say so is the sort of thing that annoys people
/// without their ever quite working out why.
fn nudge(m: Metrics, delta: isize) -> Scroll {
    let max = m.max_offset();
    if max == 0 {
        return Scroll::Live;
    }
    let from = m.offset as isize;
    let next = (from + delta).clamp(0, max as isize) as usize;
    if next >= max { Scroll::Live } else { Scroll::Held(next) }
}

fn which_claude() -> Result<()> {
    let ok = std::process::Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    anyhow::ensure!(ok, "`claude --version` did not answer");
    Ok(())
}


/// A line test: pick the handset up, listen to every wire, and have each resident say
/// one thing off the news and one thing off their own head. No terminal, no pacing, no
/// dice. This is how you find out whether the room actually works before committing an
/// evening to watching it.
async fn line_test() -> Result<()> {
    let root = std::env::var("SANDGROUSE_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let cfg = Config::load(&root.join("sandgrouse.toml"))?;
    let canon = Silo::load(&root.join("silo.toml"))?;
    which_claude()?;

    let roster: Vec<(String, u16, String)> =
        cfg.agents.iter().map(|a| (a.name.clone(), a.level, a.beat.clone())).collect();
    let mast = wire::Mast::new(&cfg.wire)?;
    let speaker = Speaker { timeout: std::time::Duration::from_secs(cfg.room.llm_timeout_secs) };

    println!("\n  CROW'S HEART — LINE TEST\n  {}\n", "=".repeat(72));

    let mut transcript = String::new();
    for a in &cfg.agents {
        let system = persona::system_prompt(a, &canon, &roster);
        let report = mast.poll(a, HashSet::new()).await;

        println!("  ── {} (L-{}, {}) ──", a.name.to_uppercase(), a.level, a.model);
        println!(
            "     wires: {}/{} answering, {} items fresh",
            report.ok_feeds,
            a.feeds.len(),
            report.fresh.len()
        );
        for (name, why) in &report.dead_feeds {
            println!("     WIRE DOWN  {name}: {why}");
        }
        // The system prompt is the largest single thing here; worth knowing its size.
        println!("     system prompt: {} chars", system.len());

        let Some(item) = report.fresh.first() else {
            println!("     no items, skipping the turn\n");
            continue;
        };

        let wire_prompt = persona::wire_turn(item, &transcript);
        match speaker.speak(&a.model, &system, &wire_prompt).await {
            Ok(Utterance::Said(body)) => {
                println!("\n     [wire · {}] {}", item.source, body);
                transcript.push_str(&format!("[00:00] {} L-{}: {}\n", a.name, a.level, body));
            }
            Ok(Utterance::Passed) => println!("\n     [wire] passed"),
            Err(e) => println!("\n     [wire] FAULT: {e}"),
        }

        let seed = persona::SEEDS[a.level as usize % persona::SEEDS.len()];
        match speaker.speak(&a.model, &system, &persona::ambient_turn(seed, &transcript)).await {
            Ok(Utterance::Said(body)) => {
                println!("     [ambient] {body}");
                transcript.push_str(&format!("[00:01] {} L-{}: {}\n", a.name, a.level, body));
            }
            Ok(Utterance::Passed) => println!("     [ambient] passed"),
            Err(e) => println!("     [ambient] FAULT: {e}"),
        }
        println!();
    }

    // And a round of reactions to whatever the line has accumulated, which is the part
    // that decides whether this reads as four monologues or as a conversation.
    println!("  ── REACTIONS ──\n");
    for a in &cfg.agents {
        let system = persona::system_prompt(a, &canon, &roster);
        let last = cfg.agents.last().map(|x| x.name.as_str()).unwrap_or("the line");
        if a.name == last {
            continue;
        }
        match speaker.speak(&a.model, &system, &persona::reply_turn(&transcript, last)).await {
            Ok(Utterance::Said(b)) => println!("     {} L-{}: {}", a.name, a.level, b),
            Ok(Utterance::Passed) => println!("     {} passed", a.name),
            Err(e) => println!("     {} FAULT: {e}", a.name),
        }
    }
    println!();
    Ok(())
}


/// Render one frame to a detached buffer and print it as plain text. Colour is lost,
/// which is the point: if the layout only works because it is pretty, it does not work.
fn panel_test() -> Result<()> {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut room = Room::default();
    room.keep_a_log(log_book());
    room.push(Author::Station, Kind::Notice, "circuit open · the mast is warming up · give them a few minutes · 694 items down the wire".into(), None);
    room.push(
        Author::Resident { name: "Jenny".into(), level: 12 },
        Kind::Wire,
        "Someone's arguing Fargate just borrows the Firecracker name without the actual \
         isolation guarantees underneath, which is exactly the kind of \"awaiting parts\" \
         labelling that gets you in trouble eventually.".into(),
        Some(Citation {
            source: "Lobsters".into(),
            title: "Fargate is not Firecracker, whatever the marketing says".into(),
            url: "https://lobste.rs/s/xxxx".into(),
        }),
    );
    room.push(
        Author::Resident { name: "Gemma".into(), level: 47 },
        Kind::Reply,
        "Sandy spotted water on 48 this morning where the map says there shouldn't be any.".into(),
        None,
    );
    room.push(Author::Chief, Kind::Ambient, "which map is he reading, the '19 revision?".into(), None);
    room.push(
        Author::Resident { name: "Bernard".into(), level: 121 },
        Kind::Reply,
        "Map's only good until it isn't. Need to know if that's a new leak or the record just being fifteen years stale.".into(),
        None,
    );

    let stations = vec![
        Station { name: "Jenny".into(),   level: 12,  model: "sonnet".into(), lamp: Lamp::Cooling, stock: 39, wires_ok: 25, wires_total: 25 },
        Station { name: "Mark".into(),    level: 88,  model: "sonnet".into(), lamp: Lamp::Working, stock: 40, wires_ok: 16, wires_total: 16 },
        Station { name: "Bernard".into(), level: 121, model: "haiku".into(),  lamp: Lamp::Ready,   stock: 12, wires_ok: 21, wires_total: 21 },
        Station { name: "Gemma".into(),   level: 47,  model: "haiku".into(),  lamp: Lamp::Cut,     stock: 0,  wires_ok: 0,  wires_total: 17 },
    ];

    let mut term = Terminal::new(TestBackend::new(100, 22))?;
    term.draw(|f| {
        let _ = tui::draw::draw(f, &View {
            room: &room,
            stations: &stations,
            input: "bernard, what's on 121 tonight",
            scroll: Scroll::Held(4),
            situation: "The number two water recycler on 62 has been making a harmonic for four days.",
            situation_tag: "recycler",
            polling: true,
            last_fault: Some("Metro Weird: HTTP 403"),
        });
    })?;

    let buf = term.backend().buffer().clone();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("{}", line.trim_end());
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 40 lines of traffic in a 15-line pane: 25 lines of room to move.
    fn at(offset: usize) -> Metrics {
        Metrics { total: 40, viewport: 15, offset }
    }

    #[test]
    fn reading_back_from_live_holds_the_view() {
        assert_eq!(nudge(at(25), -1), Scroll::Held(24));
    }

    /// Scrolling back to the foot means "show me what happens next". Making somebody
    /// press End as well to say so is the sort of thing that annoys people without
    /// their ever quite working out why.
    #[test]
    fn arriving_back_at_the_foot_re_engages_live() {
        assert_eq!(nudge(at(24), 1), Scroll::Live);
        assert_eq!(nudge(at(20), 99), Scroll::Live);
    }

    #[test]
    fn the_top_is_a_wall() {
        assert_eq!(nudge(at(0), -1), Scroll::Held(0));
        assert_eq!(nudge(at(3), -99), Scroll::Held(0));
    }

    /// A pane with nothing to scroll has no held position to offer.
    #[test]
    fn nothing_to_scroll_means_live() {
        let short = Metrics { total: 8, viewport: 15, offset: 0 };
        assert_eq!(short.max_offset(), 0);
        assert_eq!(nudge(short, -1), Scroll::Live);
        assert_eq!(nudge(short, 1), Scroll::Live);
    }

    #[test]
    fn a_page_is_the_viewport_less_two_for_overlap() {
        // Two lines of overlap so you do not lose your place across a page turn.
        assert_eq!(nudge(at(25), -13), Scroll::Held(12));
    }
}
