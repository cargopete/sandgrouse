//! The panel.
//!
//! Every noun here belongs to one machine: an intercom circuit strung down a buried
//! shaft. The transcript is TRAFFIC, the roster is ON THE LINE, the feed poller is
//! THE MAST, and a feed that has stopped answering is a wire that is down. Nothing is
//! called a "message list".
//!
//! Two instruments, both honest: the wire gauge is bound to the count of feeds that
//! actually returned items on the last poll, and each lamp is bound to whether that
//! agent has a subprocess out right now. Nothing on this screen moves on a timer.

use super::theme::{T, voice};
use crate::room::{Author, Kind, Room};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lamp {
    /// Composing. Bound to a live subprocess.
    Working,
    /// On the line and able to speak.
    Ready,
    /// Spoke recently; holding off.
    Cooling,
    /// The mast is answering for nobody: every wire down.
    Cut,
}

pub struct Station {
    pub name: String,
    pub level: u16,
    pub model: String,
    pub lamp: Lamp,
    pub stock: usize,
    pub wires_ok: usize,
    pub wires_total: usize,
}

/// Where the traffic panel is looking.
///
/// Stored as an absolute line from the top rather than as a distance from the bottom.
/// The difference matters: with an offset from the bottom, every message that lands
/// while you are reading back shifts the text under your eyes, and the only way to
/// compensate is to guess how many lines the new message occupied. Pinned to an
/// absolute line, a new arrival moves nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    /// Following the foot of the traffic. New messages appear as they land.
    Live,
    /// Held at a line. Nothing that arrives will move it.
    Held(usize),
}

/// What the traffic panel measured while drawing. The line count depends on the wrap,
/// which depends on the width, which only the draw knows - so the key handler is told
/// afterwards rather than made to work it out in advance.
#[derive(Debug, Clone, Copy, Default)]
pub struct Metrics {
    pub total: usize,
    pub viewport: usize,
    pub offset: usize,
}

impl Metrics {
    pub fn max_offset(&self) -> usize {
        self.total.saturating_sub(self.viewport)
    }
}

pub struct View<'a> {
    pub room: &'a Room,
    pub stations: &'a [Station],
    pub input: &'a str,
    pub scroll: Scroll,
    pub situation: &'a str,
    pub situation_tag: &'a str,
    pub polling: bool,
    pub last_fault: Option<&'a str>,
}

pub fn draw(f: &mut Frame, v: &View) -> Metrics {
    let area = f.area();
    let rows = Layout::vertical([
        Constraint::Length(1), // nameplate
        Constraint::Min(6),    // body
        Constraint::Length(3), // handset
        Constraint::Length(1), // keys
    ])
    .split(area);

    nameplate(f, rows[0], v);

    let cols = Layout::horizontal([Constraint::Length(27), Constraint::Min(34)]).split(rows[1]);
    sidebar(f, cols[0], v);
    let metrics = traffic(f, cols[1], v);

    handset(f, rows[2], v);
    keys(f, rows[3], v);
    metrics
}

/// One line of tracked uppercase. The terminal equivalent of an engraved plate, and
/// considerably better than a figlet banner.
fn nameplate(f: &mut Frame, area: Rect, v: &View) {
    let now = chrono::Local::now().format("%H%M").to_string();
    let left = Line::from(vec![
        Span::styled(" C R O W ' S   H E A R T ", Style::new().fg(T.brass_hi).add_modifier(Modifier::BOLD)),
        Span::styled("·", T.frame()),
        Span::styled("  P A R T Y   L I N E ", Style::new().fg(T.brass)),
    ]);
    let on_air = v.stations.iter().filter(|s| s.lamp != Lamp::Cut).count();
    let right = Line::from(vec![
        Span::styled(format!("{on_air}/{} on line  ", v.stations.len()), T.dim()),
        Span::styled(now, Style::new().fg(T.copper)),
        Span::styled("  L-38 ", T.dim()),
    ])
    .right_aligned();
    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(right), area);
}

fn panel(title: &str, reading: Option<String>) -> Block<'static> {
    let mut b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(T.frame())
        .title(Span::styled(format!(" {} ", spaced(title)), T.label()));
    if let Some(r) = reading {
        b = b.title(Line::from(Span::styled(format!(" {r} "), Style::new().fg(T.ink_dim))).right_aligned());
    }
    b
}

/// Wide tracking does a stencil's work. `.18em` has no meaning in a terminal, so a
/// single space between letters is the nearest honest equivalent.
fn spaced(s: &str) -> String {
    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

fn sidebar(f: &mut Frame, area: Rect, v: &View) {
    let rows = Layout::vertical([
        Constraint::Length(2 + v.stations.len() as u16 * 2),
        Constraint::Length(5),
        Constraint::Min(3),
    ])
    .split(area);

    // ── ON THE LINE ─────────────────────────────────────────────────────────────
    let mut lines: Vec<Line> = Vec::new();
    for s in v.stations {
        let (glyph, colour) = match s.lamp {
            Lamp::Working => ("◐", T.ember),
            Lamp::Ready => ("●", T.verdigris),
            Lamp::Cooling => ("○", T.ink_faint),
            Lamp::Cut => ("×", T.rust),
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {glyph} "), Style::new().fg(colour)),
            Span::styled(s.name.to_uppercase(), Style::new().fg(voice(&s.name)).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  L-{}", s.level), T.dim()),
        ]));
        let held = if s.wires_total > 0 && s.wires_ok == 0 {
            // Every wire down. "0 held" would read as a quiet night rather than a fault.
            "wires down".to_string()
        } else {
            format!("{} held", s.stock)
        };
        lines.push(Line::from(vec![
            Span::styled("   ", T.dim()),
            Span::styled(format!("{:<7}", s.model), Style::new().fg(T.ink_faint).add_modifier(Modifier::DIM)),
            Span::styled(held, Style::new().fg(if s.wires_ok == 0 && s.wires_total > 0 { T.rust } else { T.ink_faint })
                .add_modifier(Modifier::DIM)),
        ]));
    }
    f.render_widget(Paragraph::new(lines).block(panel("ON THE LINE", None)), rows[0]);

    // ── THE MAST ────────────────────────────────────────────────────────────────
    // A real gauge: one glyph per wire, filled for the ones that answered with items
    // on the last poll. A wire that returns 200 and nothing is drawn as down, because
    // it is down.
    let ok: usize = v.stations.iter().map(|s| s.wires_ok).sum();
    let total: usize = v.stations.iter().map(|s| s.wires_total).sum();
    // The gauge is sized from the panel it lives in, so a reading is never half drawn.
    let inner = area.width.saturating_sub(3) as usize;
    let mast = if total == 0 {
        // A missing reading is not a reading of zero, and it is certainly not green.
        vec![Line::from(Span::styled(" NO SIGNAL", Style::new().fg(T.ink_faint)))]
    } else {
        let width = inner.saturating_sub(1).min(22);
        let lit = (ok * width).div_ceil(total.max(1));
        vec![
            Line::from(vec![
                Span::styled(" ", T.dim()),
                Span::styled("▓".repeat(lit), Style::new().fg(T.brass)),
                Span::styled("░".repeat(width.saturating_sub(lit)), Style::new().fg(T.seam)),
            ]),
            Line::from(vec![
                Span::styled(" wires ", T.label()),
                Span::styled(format!("{ok}/{total}"), Style::new().fg(T.copper)),
                Span::styled(if v.polling { " listening" } else { "" }, Style::new().fg(T.ember)),
            ]),
            Line::from(match v.last_fault {
                Some(fault) => vec![
                    Span::styled(" down  ", T.label()),
                    Span::styled(clip(fault, inner.saturating_sub(8)), Style::new().fg(T.rust)),
                ],
                None => vec![Span::styled(" down  ", T.label()), Span::styled("none", Style::new().fg(T.verdigris))],
            }),
        ]
    };
    f.render_widget(Paragraph::new(mast).block(panel("THE MAST", None)), rows[1]);

    // ── SITUATION ───────────────────────────────────────────────────────────────
    // The tag goes inside as a stencilled label rather than into the title bar: a
    // spaced title plus a right-hand reading does not fit a 27-column panel, and a
    // half-drawn label is worse than none.
    let mut sit = vec![Line::from(Span::styled(v.situation_tag.to_uppercase(), T.label()))];
    sit.extend(
        textwrap::wrap(v.situation, (area.width.saturating_sub(3)).max(12) as usize)
            .into_iter()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::new().fg(T.ink_dim)))),
    );
    f.render_widget(Paragraph::new(sit).block(panel("SITUATION", None)), rows[2]);
}

fn traffic(f: &mut Frame, area: Rect, v: &View) -> Metrics {
    // One column is given back to the traveller on the right-hand rail.
    let inner_w = area.width.saturating_sub(5) as usize;
    let mut lines: Vec<Line> = Vec::new();

    if v.room.messages.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  The circuit is open and nobody has said anything yet.",
            Style::new().fg(T.ink_faint),
        )));
        lines.push(Line::from(Span::styled(
            "  Give them a minute. They are on shift.",
            Style::new().fg(T.ink_faint),
        )));
    }

    for m in &v.room.messages {
        if m.author == Author::Station {
            // Notices wrap like everything else. The stamp occupies the first line and
            // continuations are indented under it, so the column of times stays a column.
            let stamp_w = 9usize;
            for (n, w) in textwrap::wrap(&m.body, inner_w.saturating_sub(stamp_w).max(20))
                .into_iter()
                .enumerate()
            {
                lines.push(Line::from(vec![
                    Span::styled(
                        if n == 0 { format!("  {}  ", m.at.format("%H:%M")) } else { " ".repeat(stamp_w) },
                        Style::new().fg(T.seam),
                    ),
                    Span::styled(w.to_string(), Style::new().fg(T.ink_faint).add_modifier(Modifier::ITALIC)),
                ]));
            }
            continue;
        }

        let name = m.author.name().to_string();
        let colour = voice(&name);
        let lvl = m.author.level().map(|l| format!("L-{l}")).unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(format!("  {}  ", m.at.format("%H:%M")), Style::new().fg(T.seam)),
            Span::styled(name, Style::new().fg(colour).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {lvl}"), Style::new().fg(T.seam)),
        ]));

        for wrapped in textwrap::wrap(&m.body, inner_w.saturating_sub(4).max(20)) {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(wrapped.to_string(), Style::new().fg(T.ink)),
            ]));
        }

        // The citation is a footnote on the message, not a headline above it: what was
        // said matters more than where it came off the wire.
        if let Some(c) = &m.cite {
            // Budget the room properly: four of indent, two for the tick, the source
            // name and its separator. Whatever is left is what the headline may have.
            let room_left = inner_w.saturating_sub(6 + c.source.chars().count() + 3);
            let cite = format!("{} · {}", c.source, clip(&c.title, room_left));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("┄ ", Style::new().fg(T.seam)),
                Span::styled(cite, Style::new().fg(T.ink_faint).add_modifier(Modifier::DIM)),
            ]));
        }
        if m.kind != Kind::Notice {
            lines.push(Line::from(""));
        }
    }

    let viewport = area.height.saturating_sub(2) as usize;
    let total = lines.len();
    let max_off = total.saturating_sub(viewport);
    let offset = match v.scroll {
        Scroll::Live => max_off,
        Scroll::Held(top) => top.min(max_off),
    };

    let reading = match v.scroll {
        // Nothing above or below: there is no position to report, so report none.
        _ if max_off == 0 => None,
        Scroll::Live => None,
        // An honest position: which line of how many, not a percentage of nothing.
        Scroll::Held(_) => Some(format!("held {}/{}", offset + viewport.min(total), total)),
    };
    f.render_widget(
        Paragraph::new(lines).scroll((offset as u16, 0)).block(panel("TRAFFIC", reading)),
        area,
    );

    // The traveller on the rail. Drawn only when there is genuinely something above or
    // below the viewport: a scrollbar on content that does not scroll is an instrument
    // reporting a value that does not exist.
    if total > viewport {
        let mut state = ScrollbarState::new(total).position(offset).viewport_content_length(viewport);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                // Same glyphs as the mast gauge, so the two instruments read as
                // belonging to one machine rather than to two component libraries.
                .track_symbol(Some("░"))
                .thumb_symbol("▓")
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(Style::new().fg(T.seam))
                .thumb_style(Style::new().fg(T.brass)),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut state,
        );
    }

    Metrics { total, viewport, offset }
}

fn handset(f: &mut Frame, area: Rect, v: &View) {
    let line = Line::from(vec![
        Span::styled(" ▸ ", Style::new().fg(T.brass)),
        Span::styled(v.input.to_string(), Style::new().fg(T.ink)),
        Span::styled("▏", Style::new().fg(T.brass_hi)),
    ]);
    f.render_widget(Paragraph::new(line).block(panel("HANDSET", None)), area);
}

fn keys(f: &mut Frame, area: Rect, v: &View) {
    let k = |key: &'static str, what: &'static str| {
        vec![
            Span::styled(format!(" {key}"), Style::new().fg(T.brass)),
            Span::styled(format!(" {what} "), Style::new().fg(T.ink_faint)),
        ]
    };
    let mut spans = Vec::new();
    spans.extend(k("enter", "speak"));
    spans.extend(k("↑/↓", "read back"));
    spans.extend(k("pgup/pgdn", "page"));
    spans.extend(k("home", "top"));
    // Only offer the way back to the foot when you are not already standing on it.
    if v.scroll != Scroll::Live {
        spans.extend(k("end", "live"));
    }
    spans.extend(k("ctrl-c", "off the line"));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}
