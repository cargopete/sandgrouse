# Sandgrouse

```
 C R O W ' S   H E A R T ·  P A R T Y   L I N E                    4/4 on line  0214  L-38
┌ O N   T H E   L I N E ──┐┌ T R A F F I C ────────────────────────────────────────────────────┐
│ ○ JENNY  L-12           ││  02:11  Mark  L-88                                                │
│   sonnet 39 held        ││    Three weeks of the same dune is either the most stable weather  │
│ ◐ MARK  L-88            ││    system in recorded history or the camera has given up.          │
│   sonnet 40 held        ││                                                                   │
│ ● BERNARD  L-121        ││  02:12  Jenny  L-12                                               │
│   haiku  12 held        ││    Camera's fine, I checked it Tuesday. It's the dune that's       │
│ ● GEMMA  L-47           ││    stopped, which is somehow worse.                               │
│   haiku  31 held        ││                                                                   │
└─────────────────────────┘│  02:13  Bernard  L-121                                            │
┌ T H E   M A S T ────────┐│    Dunes move. If it's stopped, I'd rather not know what's         │
│ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  ││    stopped it.                                                    │
│ wires 80/80 listening   ││                                                                   │
│ down  none              ││  02:14  Gemma  L-47                                               │
└─────────────────────────┘│    Sand doesn't just stop moving. Something's held it for three    │
┌ S I T U A T I O N ──────┐│    weeks, and I'm quite happy not knowing what.                   │
│SURFACE                  ││                                                                   │
│The surface camera on    ││                                                                   │
│the mast head has been   ││                                                                   │
│showing the same dune    ││                                                                   │
│shape for three weeks.   ││                                                                   │
└─────────────────────────┘└───────────────────────────────────────────────────────────────────┘
┌ H A N D S E T ─────────────────────────────────────────────────────────────────────────────────┐
│ ▸ jenny, when did you last go up the mast                                                      │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
 enter speak  ↑/↓ read back  pgup/pgdn page  home top  end live  ctrl-c off the line
```

An open intercom circuit between the levels of a buried silo, staffed by four people who do
not know they are models.

They read the surface off a mast, argue about it, gossip about two hundred and nine
colleagues they will never meet on the stairs, and occasionally say something true about
being buried. You are on Level 38 and you have a handset.

The exchange above was not scripted. It is verbatim from a five-minute run.

---

## The premise

**Crow's Heart**, officially `H.M. DEEP SHELTER ESTABLISHMENT No. 4 (OVERSEAS)`, sunk into
bedrock under the Empty Quarter by a department that no longer exists, sealed thirty-one
years ago. One hundred and forty-four levels, two hundred and thirteen people, and one mast
that still pulls a signal off whatever is left of the satellites.

It is a wholly British installation, which explains the carpet on Level 5, the three sizes
of imperial bolt in a metric plant, and very nearly everything else.

Four of the residents keep a chat circuit open because the work is dull and the company is
not:

| | Level | Model | Beat |
|---|---|---|---|
| **Jenny** | 12 | sonnet | The mast and the comms racks. Technology, computing, blockchain, AI. |
| **Mark** | 88 | sonnet | World news, and the increasingly untenable proposition that any of it is on purpose. |
| **Bernard** | 121 | haiku | Bulgarian, European and American politics, all three taken badly. |
| **Gemma** | 47 | haiku | Science. Teaching Sandy Fothergill, who is nineteen and has never seen weather. |

Nothing on the wire is invented. Every headline they discuss came off a real feed in the
last forty-eight hours; the only fiction is who is reading it and where they are sitting.

## The canon

The load-bearing design decision. Without a shared world file, four agents invent four
different silos: Mark gossips about a woman in hydroponics whom Gemma has never heard of,
and the illusion is gone inside four messages.

So `silo.toml` is issued to all of them, verbatim, in the system prompt. It carries the
level layout, the machinery, a cast of ten named residents with jobs and grudges, and a set
of running situations that stay deliberately unresolved.

That is where storylines come from. The frozen dune above was not written by anybody. It is
one line in `silo.toml` that four people picked up in turn and made worse.

`silo.toml` also carries the roll: twenty-one principals with jobs and grudges, and another
hundred and thirteen in shorthand, one line apiece. The shorthand is not padding. In the
first run after it went in, Mark noticed it was Cyril Bagshaw's ninety-second birthday on
Level 14 and that nobody had said anything, because everybody had assumed somebody else
already had. That is a joke the roll wrote, not the model.

It costs about 5.6k tokens of system prompt per call, up from 2.2k. Worth it.

Edit the file and they will believe whatever is in it, without asking who wrote it.

## It only exists while it is open

There is no daemon, no launchd job and no background polling. It is one foreground
process: quit it and the residents stop, mid-sentence if necessary, because the `claude`
subprocesses are spawned with `kill_on_drop`. Verified rather than assumed - after Ctrl-C
there are no `sandgrouse` processes, no `claude -p` children, and the log book stops
growing.

## Reading back

The traffic panel holds an **absolute line**, not a distance from the foot. That is the
whole difference between a scrollback that works and one that does not: with an offset
from the bottom, every message that lands while you are reading shifts the text under your
eyes, and the only fix is to guess how many lines the new one occupied. Pinned to a line, an
arrival moves nothing.

`↑`/`↓` a line, `PgUp`/`PgDn` a page less two for overlap, `Home` for the top. Scrolling
back down to the foot re-engages live on its own, because arriving at the bottom means
"show me what happens next" and making you press `End` as well to say so is the kind of
thing that annoys people without their ever quite working out why.

The rail on the right is drawn only when something is genuinely above or below the
viewport, in the same `▓`/`░` as the mast gauge. A scrollbar on content that does not
scroll is an instrument reporting a value that does not exist.

## Running it

Needs the `claude` CLI, authenticated. There is no API key: every turn is a headless
`claude -p` subprocess, so it runs on a Max subscription.

```sh
cargo build --release
./target/release/sandgrouse
```

Three modes:

| | |
|---|---|
| *(no flag)* | The party line. |
| `--line-test` | Listen to all 80 wires, report the dead ones, and have each resident take one turn to stdout. This is how you check the room works without committing an evening to watching it. |
| `--panel-test` | Render one frame to a detached buffer and print it as plain text. Colour is lost, which is the point. |

The transcript is kept. `SANDGROUSE_LOGBOOK` sets the path; the default is one file a day
under the platform data directory. `SANDGROUSE_DIR` points at the config if you are running
the binary from elsewhere.

## The hard part

Not making four agents talk. Making them shut up.

Left alone, an intercom with four models on it produces a wall of text, and a wall reads as
a screensaver rather than as a room. Five mechanisms hold it back, in descending order of
bluntness:

1. **A ceiling.** Ten messages per ten minutes, whatever the dice say.
2. **One voice at a time.** A `claude -p` call takes between five and thirty seconds, so
   three agents dispatched across half a minute will all answer within eight seconds of
   each other. If anybody is composing, everybody waits. This is simply how a shared
   circuit works, and it was the single largest improvement to how the room reads.
3. **A floor under the spacing.** Twenty seconds minimum between any two messages, so a
   reply does not tread on the heels of the thing it answers.
4. **A reaction roll**, capped at one taker per message and two for yours, with a delay of
   fifteen to ninety-five seconds before the answer lands. Simultaneous replies are the
   loudest tell there is. There is also a ping-pong guard: three consecutive replies and
   the room is allowed to go quiet.
5. **The pass.** Every turn may be declined by answering `[pass]`, and the prompt says
   plainly that passing is the normal outcome. This is the only mechanism here with any
   taste in it, and it does more work than the other four combined.

Observed density with the shipped settings is roughly eleven messages in six and a half
minutes, with gaps between twenty-three and sixty-six seconds.

## The wire

Eighty feeds, each one fetched and confirmed to return items before it went in the
file. This matters more than it sounds: `feeds.reuters.com` has been dead for three years
and still appears in feed lists all over the internet, and half a dozen candidates answered
`200 OK` with nothing at all inside. A feed that returns nothing looks exactly like a
healthy feed, which is why the roster draws it as a wire that is down.

Two kinds of duplicate are suppressed. The entry id stops the same item arriving twice off
one feed; a normalised headline key stops the BBC, the Guardian and Reuters each reporting
the same election on the same evening as though it were three.

## The panel

Built to the [steampunk-ui](https://github.com/cargopete/steampunk-ui) doctrine: brass not
gold, oxidation not damage, an instrument rather than a costume.

Every noun belongs to one machine. The transcript is TRAFFIC, the roster is ON THE LINE, the
feed poller is THE MAST, the input is the HANDSET, and a feed that has stopped answering is
a wire that is down. Nothing is called a message list.

Both instruments are honest. The mast gauge is bound to the count of feeds that actually
returned items on the last poll, and each lamp is bound to whether that agent has a
subprocess out right now. Nothing on the screen moves on a timer. An agent with every wire
down reads `wires down` in rust rather than `0 held`, because a missing reading that looks
like a good reading is the worst bug this aesthetic can ship.

## The name

A sandgrouse flies fifty miles across the Empty Quarter for water, soaks it into specially
adapted belly feathers, and carries it home to chicks waiting on the ground.

Which is roughly the arrangement here.

## Licence

MIT.
