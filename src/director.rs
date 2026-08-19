//! Pacing.
//!
//! The hard part of this whole programme is not making four agents talk. It is making
//! them shut up. Left to itself an intercom with four models on it produces a wall, and
//! a wall reads as a screensaver rather than as a room.
//!
//! Three mechanisms do the work, in descending order of bluntness:
//!
//!   1. A hard ceiling on messages per trailing window. Whatever the dice say.
//!   2. A per-agent cooldown, so nobody monologues.
//!   3. A reaction roll on every message, capped at two takers, with a human-shaped
//!      delay before the answer lands. Simultaneous replies are the loudest tell there is.
//!
//! On top of that the agent itself may decline the turn by answering `[pass]`, which is
//! the only mechanism here with any taste in it.

use crate::config::{AgentCfg, RoomCfg};
use crate::persona::SEEDS;
use crate::room::{Author, Kind, Message, Room};
use crate::silo::Situation;
use crate::wire::Item;
use chrono::{DateTime, Duration, Local};
use rand::RngExt;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum Turn {
    Wire(Item),
    Ambient(String),
    Reply { to: String },
}

#[derive(Debug)]
pub enum Action {
    /// Go and listen to this agent's feeds.
    Poll(usize),
    /// Have this agent take a turn.
    Speak(usize, Turn),
}

struct Slot {
    name: String,
    next_news: DateTime<Local>,
    next_ambient: DateTime<Local>,
    next_poll: DateTime<Local>,
    /// A queued reply, and when it should land.
    pending: Option<(String, DateTime<Local>)>,
    /// True while a subprocess is out.
    busy: bool,
    /// True while the mast is being listened to on this agent's behalf.
    polling: bool,
    /// Items pulled off the wire and not yet spoken about.
    stock: VecDeque<Item>,
}

pub struct Director {
    slots: Vec<Slot>,
    cfg: RoomCfg,
    situations: Vec<Situation>,
    poll_secs: i64,
    min_gap: i64,
    /// Rotated each tick so the same agent is not always first in the queue.
    cursor: usize,
}

impl Director {
    pub fn new(agents: &[AgentCfg], cfg: RoomCfg, situations: Vec<Situation>, poll_secs: u64, min_gap: u64) -> Self {
        let now = Local::now();
        let mut rng = rand::rng();
        let slots = agents
            .iter()
            .enumerate()
            .map(|(i, a)| Slot {
                name: a.name.clone(),
                // Stagger the opening so all four do not arrive at once, and give the
                // mast a head start on the first news beat.
                next_news: now + Duration::seconds(45 + 25 * i as i64),
                next_ambient: now + Duration::seconds(rng.random_range(30..150) + 40 * i as i64),
                next_poll: now + Duration::seconds(2 * i as i64),
                pending: None,
                busy: false,
                polling: false,
                stock: VecDeque::new(),
            })
            .collect();
        Self { slots, cfg, situations, poll_secs: poll_secs as i64, min_gap: min_gap as i64, cursor: 0 }
    }

    pub fn stock_len(&self, i: usize) -> usize {
        self.slots[i].stock.len()
    }
    pub fn is_busy(&self, i: usize) -> bool {
        self.slots[i].busy
    }
    pub fn set_busy(&mut self, i: usize, v: bool) {
        self.slots[i].busy = v;
    }

    pub fn deliver_wire(&mut self, i: usize, items: Vec<Item>) {
        let slot = &mut self.slots[i];
        slot.polling = false;
        // Newest to the front. What just landed is worth more than what has been
        // sitting in the queue since the programme started, and a queue drained purely
        // in arrival order eventually reads out this morning's news at midnight.
        for it in items.into_iter().take(40).rev() {
            slot.stock.push_front(it);
        }
        while slot.stock.len() > 40 {
            slot.stock.pop_back();
        }
    }


    /// Roll for reactions. Called once for every message that reaches the line.
    pub fn on_message(&mut self, msg: &Message, room: &Room) {
        let now = Local::now();
        let (lo, hi) = self.cfg.reply_delay_secs;
        let mut rng = rand::rng();

        // Ping-pong guard: if the last three things on the line were all replies, the
        // topic has been thoroughly chewed and the room should be allowed to go quiet.
        let tail_all_replies = room
            .messages
            .iter()
            .rev()
            .filter(|m| m.author != Author::Station)
            .take(3)
            .filter(|m| m.kind == Kind::Reply)
            .count()
            >= 3;

        let from_chief = msg.author == Author::Chief;
        let lower = msg.body.to_lowercase();

        let mut takers = 0usize;
        let cap = if from_chief { 2 } else { 1 };

        // Anyone named directly answers, full stop, and quickly.
        for slot in self.slots.iter_mut() {
            if slot.name == msg.author.name() {
                continue;
            }
            if lower.contains(&slot.name.to_lowercase()) {
                slot.pending = Some((msg.author.name().to_string(), now + Duration::seconds(rng.random_range(4..14))));
                takers += 1;
            }
        }

        let base: f64 = if from_chief {
            0.85
        } else if tail_all_replies {
            0.06
        } else {
            match msg.kind {
                // News invites comment. That is what it is for.
                Kind::Wire => 0.34,
                Kind::Ambient => 0.28,
                Kind::Reply => 0.14,
                Kind::Notice => 0.0,
            }
        };

        let n = self.slots.len();
        for k in 0..n {
            let idx = (self.cursor + k) % n;
            if takers >= cap {
                break;
            }
            let slot = &mut self.slots[idx];
            if slot.name == msg.author.name() || slot.pending.is_some() || slot.busy {
                continue;
            }
            if rng.random_bool(base.min(1.0)) {
                slot.pending =
                    Some((msg.author.name().to_string(), now + Duration::seconds(rng.random_range(lo..hi) as i64)));
                takers += 1;
            }
        }
    }

    pub fn tick(&mut self, room: &Room) -> Vec<Action> {
        let now = Local::now();
        let mut out = Vec::new();
        let n = self.slots.len();
        self.cursor = (self.cursor + 1) % n.max(1);

        // Polling is independent of pacing: listening to the mast costs nobody anything.
        for i in 0..n {
            if !self.slots[i].polling && now >= self.slots[i].next_poll {
                self.slots[i].polling = true;
                let jitter = rand::rng().random_range(0..120) as i64;
                self.slots[i].next_poll = now + Duration::seconds(self.poll_secs + jitter);
                out.push(Action::Poll(i));
            }
        }

        // The ceiling. Nothing gets past this.
        let window_start = now - Duration::seconds(self.cfg.window_secs as i64);
        if room.count_since(window_start) >= self.cfg.max_messages_per {
            return out;
        }

        // One voice at a time. Turns are dispatched as they come due but the calls
        // take anywhere from five to thirty seconds, so three agents dispatched over
        // half a minute all answered within eight seconds of each other and the room
        // read as a machine emptying a buffer. If anybody is composing, everybody else
        // waits: that is simply how a shared circuit works.
        if self.slots.iter().any(|s| s.busy) {
            return out;
        }

        // And a floor under the spacing, so a reply does not tread on the heels of the
        // thing it is replying to.
        if let Some(last) = room.messages.iter().rev().find(|m| m.author != Author::Station)
            && now - last.at < Duration::seconds(self.min_gap)
        {
            return out;
        }

        // One speaker per tick. Two agents opening their mouths on the same second is
        // the single most artificial thing a room like this can do.
        for k in 0..n {
            let i = (self.cursor + k) % n;

            if self.slots[i].busy {
                continue;
            }
            if let Some(last) = room.last_spoke(&self.slots[i].name)
                && now - last < Duration::seconds(self.cfg.agent_cooldown_secs as i64)
            {
                continue;
            }

            // A queued reply outranks everything: it is already overdue by design.
            if let Some((to, at)) = self.slots[i].pending.clone()
                && now >= at
            {
                self.slots[i].pending = None;
                self.slots[i].busy = true;
                out.push(Action::Speak(i, Turn::Reply { to }));
                return out;
            }

            if now >= self.slots[i].next_news
                && let Some(item) = self.slots[i].stock.pop_front()
            {
                let (lo, hi) = self.cfg.news_beat_secs;
                self.slots[i].next_news = now + Duration::seconds(rand::rng().random_range(lo..hi) as i64);
                self.slots[i].busy = true;
                out.push(Action::Speak(i, Turn::Wire(item)));
                return out;
            }

            if now >= self.slots[i].next_ambient {
                let (lo, hi) = self.cfg.ambient_beat_secs;
                self.slots[i].next_ambient = now + Duration::seconds(rand::rng().random_range(lo..hi) as i64);
                self.slots[i].busy = true;
                out.push(Action::Speak(i, Turn::Ambient(self.seed())));
                return out;
            }
        }
        out
    }

    /// An ambient seed. Roughly a third of the time it is one of the running situations
    /// from the canon, which is what keeps a storyline alive across an evening rather
    /// than letting every ambient turn be a fresh unrelated thought.
    fn seed(&self) -> String {
        let mut rng = rand::rng();
        if !self.situations.is_empty() && rng.random_bool(0.34) {
            let s = &self.situations[rng.random_range(0..self.situations.len())];
            format!("{} What do you make of it?", s.text)
        } else {
            SEEDS[rng.random_range(0..SEEDS.len())].to_string()
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentCfg, FeedCfg};

    fn cfg() -> RoomCfg {
        RoomCfg {
            news_beat_secs: (300, 600),
            ambient_beat_secs: (420, 1500),
            max_messages_per: 10,
            window_secs: 600,
            agent_cooldown_secs: 75,
            min_gap_secs: 20,
            reply_delay_secs: (15, 95),
            context_messages: 24,
            llm_timeout_secs: 120,
            max_concurrent_llm: 2,
        }
    }

    fn agents() -> Vec<AgentCfg> {
        ["Jenny", "Mark", "Bernard", "Gemma"]
            .iter()
            .enumerate()
            .map(|(i, n)| AgentCfg {
                name: n.to_string(),
                level: 10 + i as u16,
                model: "haiku".into(),
                role: "mechanical".into(),
                beat: "things".into(),
                persona: "a person".into(),
                feeds: vec![FeedCfg { name: "x".into(), url: "https://example.invalid/f".into() }],
            })
            .collect()
    }

    fn director() -> (Director, Room) {
        (Director::new(&agents(), cfg(), vec![], 900, 20), Room::default())
    }

    /// Being named on an open circuit and ignored would be very odd indeed.
    #[test]
    fn naming_someone_guarantees_they_answer() {
        let (mut d, mut room) = director();
        let id = room.push(Author::Chief, Kind::Ambient, "bernard, what's on 121 tonight".into(), None);
        let m = room.messages.iter().find(|m| m.id == id).unwrap().clone();
        d.on_message(&m, &room);

        let bernard = d.slots.iter().find(|s| s.name == "Bernard").unwrap();
        assert!(bernard.pending.is_some(), "Bernard was addressed by name and did not queue an answer");
        let (to, _) = bernard.pending.as_ref().unwrap();
        assert_eq!(to, "Chief");
    }

    /// The whole room turning to face you at once is the loudest tell there is.
    #[test]
    fn a_message_never_recruits_the_entire_room() {
        for _ in 0..200 {
            let (mut d, mut room) = director();
            let id = room.push(Author::Chief, Kind::Ambient, "morning all".into(), None);
            let m = room.messages.iter().find(|m| m.id == id).unwrap().clone();
            d.on_message(&m, &room);
            let takers = d.slots.iter().filter(|s| s.pending.is_some()).count();
            assert!(takers <= 2, "{takers} agents queued at once; the cap for a message from Chief is 2");
        }
    }

    #[test]
    fn nobody_answers_themselves() {
        for _ in 0..200 {
            let (mut d, mut room) = director();
            let id = room.push(
                Author::Resident { name: "Mark".into(), level: 88 },
                Kind::Wire,
                "mark here, and I mention Mark twice".into(),
                None,
            );
            let m = room.messages.iter().find(|m| m.id == id).unwrap().clone();
            d.on_message(&m, &room);
            let mark = d.slots.iter().find(|s| s.name == "Mark").unwrap();
            assert!(mark.pending.is_none(), "Mark queued a reply to himself");
        }
    }

    /// The ceiling is the bluntest instrument here and it must not be negotiable.
    #[test]
    fn the_ceiling_stops_all_speech() {
        let (mut d, mut room) = director();
        for i in 0..10 {
            room.push(
                Author::Resident { name: format!("Ghost{i}"), level: 1 },
                Kind::Ambient,
                "filling the window".into(),
                None,
            );
        }
        // Everyone is due to speak, and the room is full.
        for slot in d.slots.iter_mut() {
            slot.next_ambient = Local::now() - Duration::seconds(60);
            slot.next_news = Local::now() - Duration::seconds(60);
        }
        let actions = d.tick(&room);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Speak(..))),
            "somebody spoke through a full window"
        );
    }

    /// A call takes five to thirty seconds, so without this three agents dispatched
    /// across half a minute all answer within eight seconds of each other.
    #[test]
    fn only_one_voice_at_a_time() {
        let (mut d, room) = director();
        for slot in d.slots.iter_mut() {
            slot.next_ambient = Local::now() - Duration::seconds(60);
        }
        d.slots[0].busy = true;
        let actions = d.tick(&room);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Speak(..))),
            "an agent was dispatched while another was still composing"
        );
    }

    #[test]
    fn a_reply_does_not_tread_on_the_message_it_answers() {
        let (mut d, mut room) = director();
        room.push(Author::Resident { name: "Gemma".into(), level: 47 }, Kind::Ambient, "just said this".into(), None);
        for slot in d.slots.iter_mut() {
            slot.next_ambient = Local::now() - Duration::seconds(60);
        }
        let actions = d.tick(&room);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Speak(..))),
            "somebody spoke inside the {}s floor", d.min_gap
        );
    }

    /// Freshest first. A queue drained purely in arrival order reads out the morning's
    /// news at midnight.
    #[test]
    fn fresh_items_jump_the_backlog() {
        use crate::wire::Item;
        let mk = |t: &str| Item {
            source: "s".into(), title: t.into(), summary: String::new(),
            url: String::new(), published: None, key: t.into(),
        };
        let (mut d, _) = director();
        d.deliver_wire(0, vec![mk("old-a"), mk("old-b")]);
        d.deliver_wire(0, vec![mk("new-a"), mk("new-b")]);
        assert_eq!(d.slots[0].stock.front().unwrap().title, "new-a");
        assert_eq!(d.stock_len(0), 4);
    }
}
