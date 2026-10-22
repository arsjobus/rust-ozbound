//! NPC behavior (pure Rust, no Bevy): what a villager does when nobody is talking to it, and
//! how it reacts when somebody is.
//!
//! Every NPC owns a [`Brain`] that runs the same behavior tree (see `behavior.rs`) with the
//! settings from its [`Behavior`] (the level file's `[behaviors]` section). Each frame the
//! game gives the brain a [`Senses`] (what the world looks like right now) and gets back an
//! [`Intent`] (which tile to step onto, what to look at, which emote to play). The Bevy layer
//! only glides the body and plays the pose, so everything decided here is tested without a window.
//!
//! The tree, from highest priority down (it is re-read from the top every tick):
//!
//! 1. **Engaged**: the player is walking up to this NPC or talking with it. The NPC finishes
//!    the stride it is in, then stands still and turns to face the player.
//! 2. **Farewell**: a conversation just ended. The NPC waves (see [`Emote::Wave`]) and stays
//!    put until the wave is over.
//! 3. **Walk**: follow the planned route one tile at a time.
//! 4. **Rest**: stand and wait. When the wait ends, an NPC with `emotes` may play one.
//! 5. **Choose**: a wanderer picks a new spot inside its range and plans a route to it.
//! 6. **Settle**: otherwise (a stationary NPC) start a new wait.
//!
//! Rules:
//!
//! - A **stationary** NPC (`stay`) never leaves its tile. A **wanderer** (`wander <radius>`)
//!   only ever steps on walkable tiles whose centre is within `radius` tiles of its home
//!   tile, never cuts a blocked corner, and never steps onto a tile in [`Senses::avoid`]
//!   (the player, other people, creatures, plants, waymarks, doorways, exits).
//! - A route is only ever followed one step at a time and each step is checked again just
//!   before it is taken, so a tile that became occupied is never entered: the NPC gives up
//!   the route and rests instead.
//! - An NPC that the player is walking toward, or talking with, takes no new step. Its logical
//!   tile is the tile it is gliding onto, so a click always plans to where the NPC will stand.
//! - Choices are random but come from a seeded generator ([`Rng`]), so a brain given the
//!   same seed, behavior, and senses always does the same thing.
//! - Times are seconds. A negative or non-finite time step counts as zero.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::behavior::{Host, Node, Status};
use crate::emote::Emote;
use crate::pathfinding::step_allowed;
use crate::world::{TilePos, WorldMap};

/// The widest wander range the level file may ask for, in tiles.
pub const MAX_WANDER_RADIUS: u32 = 10;
/// The longest pause the level file may ask for, in seconds.
pub const MAX_PAUSE_SECONDS: u32 = 120;
/// How long an NPC stands before moving or emoting when the level file says nothing.
pub const DEFAULT_PAUSE: (u32, u32) = (3, 8);

/// Whether an NPC stays on its tile or roams around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Stay,
    /// Roam within `radius` tiles of the home tile (1 to [`MAX_WANDER_RADIUS`]).
    Wander { radius: u32 },
}

/// How one NPC behaves when left alone (the level file's `[behaviors]` line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Behavior {
    pub mode: Mode,
    /// The shortest and longest wait between moves (or emotes), in whole seconds.
    pub pause: (u32, u32),
    /// Emotes the NPC plays now and then while resting. Empty means it never does.
    pub emotes: Vec<Emote>,
}

impl Default for Behavior {
    /// Stands still and does nothing else, like every NPC before this milestone.
    fn default() -> Self {
        Self { mode: Mode::Stay, pause: DEFAULT_PAUSE, emotes: Vec::new() }
    }
}

impl Behavior {
    /// Parses what follows the NPC's id on a `[behaviors]` line:
    /// `stay | wander <radius>`, then optionally `pause <min> <max>` and, last,
    /// `emotes <name> [<name> ...]`.
    pub fn parse(text: &str) -> Result<Self, String> {
        const SHAPE: &str = "\"stay\" or \"wander <radius>\", then optionally \"pause <min> <max>\" and \"emotes <name> ...\"";
        let mut words = text.split_whitespace();
        let mut behavior = Behavior::default();
        behavior.mode = match words.next() {
            Some("stay") => Mode::Stay,
            Some("wander") => {
                let radius = words
                    .next()
                    .and_then(|w| w.parse::<u32>().ok())
                    .ok_or_else(|| format!("wander needs a range in tiles, as in \"wander 3\" (expected {SHAPE})"))?;
                if radius == 0 || radius > MAX_WANDER_RADIUS {
                    return Err(format!("a wander range must be from 1 to {MAX_WANDER_RADIUS} tiles, not {radius}"));
                }
                Mode::Wander { radius }
            }
            Some(other) => return Err(format!("unknown behavior {other:?} (expected {SHAPE})")),
            None => return Err(format!("missing behavior (expected {SHAPE})")),
        };
        let mut seen_pause = false;
        while let Some(word) = words.next() {
            match word {
                "pause" => {
                    if seen_pause {
                        return Err("pause is given twice".into());
                    }
                    seen_pause = true;
                    let mut number = || words.next().and_then(|w| w.parse::<u32>().ok());
                    let (Some(low), Some(high)) = (number(), number()) else {
                        return Err("pause needs two whole numbers of seconds, as in \"pause 2 6\"".into());
                    };
                    if low == 0 || low > high || high > MAX_PAUSE_SECONDS {
                        return Err(format!("pause must be at least 1 second, with min no more than max, and at most {MAX_PAUSE_SECONDS} seconds (found {low} {high})"));
                    }
                    behavior.pause = (low, high);
                }
                "emotes" => {
                    for name in words.by_ref() {
                        let emote = Emote::from_command(name).ok_or_else(|| {
                            let known: Vec<&str> = Emote::ALL.iter().map(|e| e.command()).collect();
                            format!("unknown emote {name:?} (use {})", known.join(", "))
                        })?;
                        if behavior.emotes.contains(&emote) {
                            return Err(format!("emote {name:?} is listed twice"));
                        }
                        behavior.emotes.push(emote);
                    }
                    if behavior.emotes.is_empty() {
                        return Err("emotes needs at least one emote name".into());
                    }
                }
                other => return Err(format!("unexpected {other:?} (expected {SHAPE})")),
            }
        }
        Ok(behavior)
    }

    /// The wander radius, or `None` for a stationary NPC.
    pub fn radius(&self) -> Option<u32> {
        match self.mode {
            Mode::Stay => None,
            Mode::Wander { radius } => Some(radius),
        }
    }
}

/// How much of the player's attention this NPC has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attention {
    /// Nobody is coming to talk.
    Free,
    /// The player clicked this NPC and is walking over.
    Approached,
    /// A conversation with this NPC is open.
    Talking,
}

/// What the world looks like to one NPC this frame.
pub struct Senses<'a> {
    pub map: &'a WorldMap,
    pub attention: Attention,
    /// Where the player stands (what a talking NPC looks at).
    pub player_tile: TilePos,
    /// The body is still gliding onto the tile it last stepped to.
    pub striding: bool,
    /// Tiles an NPC must not step onto.
    pub avoid: &'a HashSet<TilePos>,
}

/// One emote the NPC wants to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmoteOrder {
    pub emote: Emote,
    /// True for the wave that ends a conversation (the chat says so).
    pub farewell: bool,
}

/// What an NPC wants done this frame. Everything is optional; the default is "nothing".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Intent {
    /// Start gliding onto this tile (the brain already counts it as the NPC's tile).
    pub step: Option<TilePos>,
    /// Turn to look at this tile.
    pub face: Option<TilePos>,
    pub emote: Option<EmoteOrder>,
}

/// A small seeded generator (SplitMix64), so NPC choices repeat exactly in tests.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number from `0` to `n - 1`. `n == 0` gives 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Seconds from `low` to `high` (either order), whole numbers.
    pub fn seconds(&mut self, (low, high): (u32, u32)) -> f32 {
        let (low, high) = (low.min(high), low.max(high));
        (low + self.below((high - low) as usize + 1) as u32) as f32
    }
}

/// A seed that depends only on `text` (FNV-1a), so each NPC gets its own steady stream of choices.
pub fn seed_from(text: &str) -> u64 {
    text.bytes().fold(0xCBF2_9CE4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3))
}

/// True when `tile` is within `radius` tiles of `home` (straight-line distance between centres).
pub fn in_range(home: TilePos, tile: TilePos, radius: u32) -> bool {
    let (dx, dy) = (i128::from(tile.x) - i128::from(home.x), i128::from(tile.y) - i128::from(home.y));
    dx * dx + dy * dy <= i128::from(radius) * i128::from(radius)
}

const KING_STEPS: [(i32, i32); 8] = [(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (1, -1), (-1, -1), (-1, 1)];

/// True when a wanderer on `from` may step onto `to`: one king move, legal on the map (no
/// corner cutting), inside the range, and not an avoided tile.
pub fn may_step(map: &WorldMap, from: TilePos, to: TilePos, home: TilePos, radius: u32, avoid: &HashSet<TilePos>) -> bool {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    dx.abs() <= 1
        && dy.abs() <= 1
        && (dx, dy) != (0, 0)
        && step_allowed(map, from, dx, dy)
        && in_range(home, to, radius)
        && !avoid.contains(&to)
}

/// Every tile a wanderer could walk to from `from` without leaving its range or touching an
/// avoided tile, with the shortest route to each (excluding `from`). Sorted by tile so the
/// result never depends on hash order.
pub fn wander_routes(map: &WorldMap, from: TilePos, home: TilePos, radius: u32, avoid: &HashSet<TilePos>) -> Vec<(TilePos, Vec<TilePos>)> {
    let mut parent: HashMap<TilePos, TilePos> = HashMap::new();
    let mut seen: HashSet<TilePos> = HashSet::from([from]);
    let mut queue = VecDeque::from([from]);
    let mut reached = Vec::new();
    while let Some(current) = queue.pop_front() {
        for (dx, dy) in KING_STEPS {
            let next = TilePos::new(current.x + dx, current.y + dy);
            if seen.contains(&next) || !may_step(map, current, next, home, radius, avoid) {
                continue;
            }
            seen.insert(next);
            parent.insert(next, current);
            reached.push(next);
            queue.push_back(next);
        }
    }
    reached.sort_by_key(|t| (t.y, t.x));
    reached
        .into_iter()
        .map(|goal| {
            let mut route = vec![goal];
            let mut at = goal;
            while let Some(&prev) = parent.get(&at) {
                if prev == from {
                    break;
                }
                route.push(prev);
                at = prev;
            }
            route.reverse();
            (goal, route)
        })
        .collect()
}

/// How many tiles besides `home` a wanderer could ever reach (used to refuse a range in which
/// the NPC would be stuck).
pub fn wander_area(map: &WorldMap, home: TilePos, radius: u32, avoid: &HashSet<TilePos>) -> usize {
    wander_routes(map, home, home, radius, avoid).len()
}

/// The leaves of the NPC tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaf {
    Engaged,
    Hold,
    FarewellActive,
    Farewell,
    HasRoute,
    Walk,
    Resting,
    Rest,
    Wanders,
    Choose,
    Settle,
}

fn build_tree() -> Node<Leaf> {
    use Leaf::*;
    let tree = Node::selector(vec![
        Node::sequence(vec![Node::leaf(Engaged), Node::leaf(Hold)]),
        Node::sequence(vec![Node::leaf(FarewellActive), Node::leaf(Farewell)]),
        Node::sequence(vec![Node::leaf(HasRoute), Node::leaf(Walk)]),
        Node::sequence(vec![Node::leaf(Resting), Node::leaf(Rest)]),
        Node::sequence(vec![Node::leaf(Wanders), Node::leaf(Choose)]),
        Node::leaf(Settle),
    ]);
    debug_assert!(tree.validate().is_ok());
    tree
}

/// Everything one NPC remembers between ticks.
#[derive(Debug, Clone)]
struct State {
    behavior: Behavior,
    home: TilePos,
    tile: TilePos,
    route: VecDeque<TilePos>,
    /// Seconds left to stand still.
    rest: f32,
    /// Seconds left of a farewell wave.
    gesture: f32,
    farewell_due: bool,
    /// An idle emote was just played, so the next end of rest moves on instead of repeating it.
    emoted: bool,
    previous: Attention,
    rng: Rng,
}

/// An NPC's mind: the shared tree plus this NPC's own state.
#[derive(Debug, Clone)]
pub struct Brain {
    tree: Node<Leaf>,
    state: State,
}

impl Brain {
    /// A brain for an NPC that starts on (and calls home) `home`. The first wait is a random
    /// part of a pause, so NPCs that spawn together do not all move on the same frame.
    pub fn new(behavior: Behavior, home: TilePos, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let first = rng.seconds(behavior.pause) * (rng.below(100) as f32 / 100.0);
        Self {
            tree: build_tree(),
            state: State {
                behavior,
                home,
                tile: home,
                route: VecDeque::new(),
                rest: first,
                gesture: 0.0,
                farewell_due: false,
                emoted: false,
                previous: Attention::Free,
                rng,
            },
        }
    }

    /// The tile the NPC stands on, or is gliding onto.
    pub fn tile(&self) -> TilePos {
        self.state.tile
    }

    pub fn home(&self) -> TilePos {
        self.state.home
    }

    pub fn behavior(&self) -> &Behavior {
        &self.state.behavior
    }

    /// Steps left on the current route.
    pub fn route_len(&self) -> usize {
        self.state.route.len()
    }

    /// Thinks for `dt` seconds and says what to do.
    pub fn tick(&mut self, dt: f32, senses: &Senses) -> Intent {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        let state = &mut self.state;
        // The end of a conversation is the one moment a goodbye is owed.
        if state.previous == Attention::Talking && senses.attention != Attention::Talking {
            state.farewell_due = true;
        }
        state.previous = senses.attention;
        let mut ctx = Ctx { state, senses, dt, intent: Intent::default() };
        self.tree.tick(&mut ctx);
        ctx.intent
    }
}

struct Ctx<'a, 'b> {
    state: &'a mut State,
    senses: &'a Senses<'b>,
    dt: f32,
    intent: Intent,
}

impl Ctx<'_, '_> {
    fn pause(&mut self) -> f32 {
        let pause = self.state.behavior.pause;
        self.state.rng.seconds(pause)
    }
}

impl Host<Leaf> for Ctx<'_, '_> {
    fn run(&mut self, leaf: &Leaf) -> Status {
        match leaf {
            Leaf::Engaged => {
                if self.senses.attention == Attention::Free {
                    return Status::Failure;
                }
                // Being spoken to cancels any goodbye still owed from an earlier conversation.
                self.state.farewell_due = false;
                self.state.gesture = 0.0;
                self.state.emoted = false;
                Status::Success
            }
            Leaf::Hold => {
                if !self.senses.striding {
                    self.intent.face = Some(self.senses.player_tile);
                }
                Status::Running
            }
            Leaf::FarewellActive => bool_status(self.state.farewell_due || self.state.gesture > 0.0),
            Leaf::Farewell => {
                if self.state.farewell_due {
                    self.state.farewell_due = false;
                    self.state.gesture = Emote::Wave.duration();
                    self.state.route.clear();
                    self.intent.face = Some(self.senses.player_tile);
                    self.intent.emote = Some(EmoteOrder { emote: Emote::Wave, farewell: true });
                    return Status::Running;
                }
                self.state.gesture = (self.state.gesture - self.dt).max(0.0);
                if self.state.gesture > 0.0 {
                    Status::Running
                } else {
                    // The wave is done: wait a moment before moving off.
                    self.state.rest = self.pause();
                    Status::Success
                }
            }
            Leaf::HasRoute => bool_status(!self.state.route.is_empty()),
            Leaf::Walk => {
                if self.senses.striding {
                    return Status::Running;
                }
                let Some(radius) = self.state.behavior.radius() else {
                    self.state.route.clear();
                    return Status::Failure;
                };
                let next = *self.state.route.front().expect("HasRoute passed");
                if !may_step(self.senses.map, self.state.tile, next, self.state.home, radius, self.senses.avoid) {
                    // Something is in the way now: give up this walk and rest.
                    self.state.route.clear();
                    self.state.rest = self.pause();
                    return Status::Failure;
                }
                self.state.route.pop_front();
                self.state.tile = next;
                self.intent.step = Some(next);
                if self.state.route.is_empty() {
                    self.state.rest = self.pause();
                }
                Status::Running
            }
            Leaf::Resting => bool_status(self.state.rest > 0.0),
            Leaf::Rest => {
                self.state.rest -= self.dt;
                if self.state.rest > 0.0 {
                    return Status::Running;
                }
                self.state.rest = 0.0;
                if self.state.emoted {
                    self.state.emoted = false;
                    return Status::Success;
                }
                let count = self.state.behavior.emotes.len();
                if count == 0 || self.senses.striding {
                    return Status::Success;
                }
                // About every other rest ends in a gesture.
                if self.state.rng.below(2) == 0 {
                    let emote = self.state.behavior.emotes[self.state.rng.below(count)];
                    self.intent.emote = Some(EmoteOrder { emote, farewell: false });
                    self.state.emoted = true;
                    self.state.rest = emote.duration() + 0.2;
                    return Status::Running;
                }
                Status::Success
            }
            Leaf::Wanders => bool_status(self.state.behavior.radius().is_some()),
            Leaf::Choose => {
                let radius = self.state.behavior.radius().expect("Wanders passed");
                let routes = wander_routes(self.senses.map, self.state.tile, self.state.home, radius, self.senses.avoid);
                if routes.is_empty() {
                    self.state.rest = self.pause();
                } else {
                    let (_, route) = &routes[self.state.rng.below(routes.len())];
                    self.state.route = route.iter().copied().collect();
                }
                Status::Success
            }
            Leaf::Settle => {
                self.state.rest = self.pause();
                Status::Success
            }
        }
    }
}

fn bool_status(holds: bool) -> Status {
    if holds {
        Status::Success
    } else {
        Status::Failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y)
    }

    fn open(width: usize, height: usize) -> WorldMap {
        let row = ".".repeat(width);
        WorldMap::from_ascii(&vec![row; height].join("\n")).unwrap()
    }

    fn wanderer(radius: u32) -> Behavior {
        Behavior { mode: Mode::Wander { radius }, pause: (1, 1), emotes: vec![] }
    }

    fn senses<'a>(map: &'a WorldMap, avoid: &'a HashSet<TilePos>, attention: Attention) -> Senses<'a> {
        Senses { map, attention, player_tile: t(0, 0), striding: false, avoid }
    }

    /// Runs a brain for `seconds`, finishing each stride at once, and returns every tile it stood on.
    fn walk_for(brain: &mut Brain, map: &WorldMap, avoid: &HashSet<TilePos>, seconds: f32) -> Vec<TilePos> {
        let mut seen = vec![brain.tile()];
        let mut time = 0.0;
        while time < seconds {
            let intent = brain.tick(0.1, &senses(map, avoid, Attention::Free));
            if let Some(step) = intent.step {
                seen.push(step);
            }
            time += 0.1;
        }
        seen
    }

    // ---- parsing

    #[test]
    fn behaviors_parse_in_every_shape() {
        assert_eq!(Behavior::parse("stay").unwrap(), Behavior::default());
        let b = Behavior::parse("wander 3").unwrap();
        assert_eq!((b.mode, b.pause, b.emotes.len()), (Mode::Wander { radius: 3 }, DEFAULT_PAUSE, 0));
        let b = Behavior::parse("wander 4 pause 2 6 emotes wave Dance").unwrap();
        assert_eq!((b.mode, b.pause, b.emotes), (Mode::Wander { radius: 4 }, (2, 6), vec![Emote::Wave, Emote::Dance]));
        let b = Behavior::parse("  stay   emotes cheer ").unwrap();
        assert_eq!((b.mode, b.emotes), (Mode::Stay, vec![Emote::Cheer]));
        assert_eq!(Behavior::parse("stay pause 1 1").unwrap().pause, (1, 1));
    }

    #[test]
    fn bad_behaviors_say_what_is_wrong() {
        let cases = [
            ("", "missing behavior"),
            ("dance", "unknown behavior"),
            ("wander", "needs a range"),
            ("wander x", "needs a range"),
            ("wander 0", "from 1 to 10"),
            ("wander 11", "from 1 to 10"),
            ("stay pause 3", "two whole numbers"),
            ("stay pause 0 3", "at least 1 second"),
            ("stay pause 5 2", "min no more than max"),
            ("stay pause 1 121", "at most 120"),
            ("stay pause 1 2 pause 1 2", "twice"),
            ("stay emotes", "at least one"),
            ("stay emotes jump", "unknown emote"),
            ("stay emotes wave wave", "listed twice"),
            ("stay emotes wave pause 1 2", "unknown emote"),
            ("stay roam", "unexpected"),
        ];
        for (text, needle) in cases {
            let error = Behavior::parse(text).expect_err(text);
            assert!(error.contains(needle), "{text:?}: {error}");
        }
    }

    #[test]
    fn the_default_behavior_is_to_stand_still() {
        assert_eq!(Behavior::default().radius(), None);
        assert_eq!(Behavior::parse("wander 5").unwrap().radius(), Some(5));
    }

    // ---- helpers

    #[test]
    fn the_generator_is_repeatable_and_bounded() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        assert_eq!((0..20).map(|_| a.next_u64()).collect::<Vec<_>>(), (0..20).map(|_| b.next_u64()).collect::<Vec<_>>());
        let mut rng = Rng::new(1);
        assert_eq!(rng.below(0), 0);
        assert!((0..500).all(|_| rng.below(7) < 7));
        assert!((0..500).map(|_| rng.below(3)).collect::<HashSet<_>>().len() == 3, "every value shows up");
        assert!((0..200).all(|_| (2.0..=5.0).contains(&rng.seconds((5, 2)))));
        assert_eq!(rng.seconds((4, 4)), 4.0);
        assert_ne!(seed_from("pell"), seed_from("brae"));
        assert_eq!(seed_from("pell"), seed_from("pell"));
    }

    #[test]
    fn range_is_a_circle_around_home() {
        assert!(in_range(t(5, 5), t(5, 5), 1));
        assert!(in_range(t(5, 5), t(7, 5), 2));
        assert!(!in_range(t(5, 5), t(8, 5), 2));
        assert!(in_range(t(5, 5), t(6, 6), 2));
        assert!(!in_range(t(5, 5), t(7, 7), 2), "the corner of the square is too far");
        assert!(in_range(t(0, 0), t(-2, 0), 2), "works on every side");
        assert!(!in_range(t(i32::MAX, 0), t(i32::MIN, 0), 3), "no overflow");
    }

    #[test]
    fn steps_obey_the_map_the_range_and_the_avoid_list() {
        let map = WorldMap::from_ascii("....\n.#..\n....").unwrap();
        let none = HashSet::new();
        let home = t(0, 0);
        assert!(may_step(&map, t(0, 0), t(1, 0), home, 3, &none));
        assert!(!may_step(&map, t(0, 0), t(2, 0), home, 3, &none), "two tiles at once");
        assert!(!may_step(&map, t(0, 0), t(0, 0), home, 3, &none), "standing still is not a step");
        assert!(!may_step(&map, t(0, 1), t(1, 1), home, 3, &none), "a rock");
        assert!(!may_step(&map, t(0, 0), t(1, 1), home, 3, &none), "no stepping onto rock");
        assert!(!may_step(&map, t(0, 2), t(1, 1), home, 3, &none));
        assert!(!may_step(&map, t(1, 0), t(0, 1), home, 3, &none), "no cutting the rock's corner");
        assert!(!may_step(&map, t(2, 0), t(3, 0), home, 2, &none), "outside the range");
        let avoid = HashSet::from([t(1, 0)]);
        assert!(!may_step(&map, t(0, 0), t(1, 0), home, 3, &avoid));
        assert!(!may_step(&map, t(0, 0), t(-1, 0), home, 3, &none), "off the map");
    }

    #[test]
    fn wander_routes_are_short_legal_and_inside_the_range() {
        let map = open(9, 9);
        let none = HashSet::new();
        let home = t(4, 4);
        let routes = wander_routes(&map, home, home, 2, &none);
        assert_eq!(routes.len(), 12, "the 13 tiles of the circle, minus home");
        for (goal, route) in &routes {
            assert_eq!(route.last(), Some(goal));
            let mut at = home;
            for &step in route {
                assert!(may_step(&map, at, step, home, 2, &none));
                at = step;
            }
        }
        let far = routes.iter().find(|(g, _)| *g == t(6, 4)).unwrap();
        assert_eq!(far.1.len(), 2);
        let sorted: Vec<_> = routes.iter().map(|(g, _)| (g.y, g.x)).collect();
        assert!(sorted.windows(2).all(|w| w[0] < w[1]), "stable order");
    }

    #[test]
    fn wander_routes_go_around_obstacles_and_never_through_avoided_tiles() {
        let map = open(7, 3);
        let home = t(0, 1);
        let wall: HashSet<_> = [t(2, 0), t(2, 1), t(2, 2)].into_iter().collect();
        let routes = wander_routes(&map, home, home, 5, &wall);
        assert!(routes.iter().all(|(g, _)| g.x < 2), "the avoided column cuts the area in two");
        assert_eq!(wander_area(&map, home, 5, &wall), 5, "the five other tiles on her side");
        let gap: HashSet<_> = [t(2, 0), t(2, 2)].into_iter().collect();
        let through = wander_routes(&map, home, home, 5, &gap);
        let beyond = through.iter().find(|(g, _)| *g == t(4, 1)).expect("the gap lets her through");
        assert!(beyond.1.contains(&t(2, 1)));
    }

    #[test]
    fn a_boxed_in_wanderer_has_nowhere_to_go() {
        let map = WorldMap::from_ascii("###\n#.#\n###").unwrap();
        assert_eq!(wander_area(&map, t(1, 1), 3, &HashSet::new()), 0);
    }

    // ---- brains

    #[test]
    fn a_stationary_npc_never_steps() {
        let map = open(7, 7);
        let none = HashSet::new();
        let mut brain = Brain::new(Behavior { pause: (1, 1), ..Behavior::default() }, t(3, 3), 1);
        let seen = walk_for(&mut brain, &map, &none, 120.0);
        assert_eq!(seen, vec![t(3, 3)]);
        assert_eq!(brain.tile(), t(3, 3));
    }

    #[test]
    fn a_wanderer_roams_but_never_leaves_its_range() {
        let map = open(15, 15);
        let none = HashSet::new();
        let home = t(7, 7);
        let mut brain = Brain::new(wanderer(3), home, 99);
        let seen = walk_for(&mut brain, &map, &none, 600.0);
        assert!(seen.len() > 30, "it keeps moving ({} steps)", seen.len());
        assert!(seen.iter().all(|&tile| in_range(home, tile, 3)), "{seen:?}");
        let distinct: HashSet<_> = seen.iter().collect();
        assert!(distinct.len() > 8, "it visits many different tiles");
        for pair in seen.windows(2) {
            assert!(may_step(&map, pair[0], pair[1], home, 3, &none), "{:?} -> {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn a_wanderer_never_steps_on_an_avoided_tile() {
        let map = open(9, 9);
        let home = t(4, 4);
        let avoid: HashSet<_> = [t(3, 4), t(5, 4), t(4, 3), t(4, 5), t(4, 4)].into_iter().collect();
        let mut brain = Brain::new(wanderer(3), home, 5);
        let seen = walk_for(&mut brain, &map, &avoid, 600.0);
        assert!(seen.iter().skip(1).all(|t| !avoid.contains(t)), "{seen:?}");
    }

    #[test]
    fn the_same_seed_gives_the_same_walk_and_other_seeds_differ() {
        let map = open(15, 15);
        let none = HashSet::new();
        let walk = |seed| walk_for(&mut Brain::new(wanderer(4), t(7, 7), seed), &map, &none, 200.0);
        assert_eq!(walk(3), walk(3));
        assert_ne!(walk(3), walk(4));
    }

    #[test]
    fn a_step_is_checked_again_just_before_it_is_taken() {
        let map = open(7, 1);
        let home = t(0, 0);
        let mut brain = Brain::new(wanderer(6), home, 2);
        brain.state.rest = 0.0;
        let none = HashSet::new();
        // Plan a walk, then block the first tile of it before the step is taken.
        let mut intent = Intent::default();
        for _ in 0..3 {
            intent = brain.tick(0.1, &senses(&map, &none, Attention::Free));
            if brain.route_len() > 0 || intent.step.is_some() {
                break;
            }
        }
        assert!(brain.route_len() > 0 || intent.step.is_some(), "a walk was planned");
        if intent.step.is_some() {
            return; // the first step went out in the same tick; the retry below covers blocking
        }
        let blocked: HashSet<_> = [t(1, 0)].into_iter().collect();
        let next = brain.tick(0.1, &senses(&map, &blocked, Attention::Free));
        assert_eq!(next.step, None, "the blocked tile was not entered");
        assert_eq!(brain.route_len(), 0, "the walk was given up");
        assert_eq!(brain.tile(), home);
    }

    #[test]
    fn a_boxed_in_wanderer_just_waits() {
        let map = WorldMap::from_ascii("###\n#.#\n###").unwrap();
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(3), t(1, 1), 1);
        let seen = walk_for(&mut brain, &map, &none, 60.0);
        assert_eq!(seen, vec![t(1, 1)]);
    }

    #[test]
    fn nobody_steps_while_still_gliding() {
        let map = open(9, 9);
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(3), t(4, 4), 8);
        let mut steps = 0;
        let mut striding_steps = 0;
        for _ in 0..3000 {
            let striding = steps % 2 == 1; // glide for exactly one extra tick after each step
            let intent = brain.tick(0.1, &Senses { striding, ..senses(&map, &none, Attention::Free) });
            if intent.step.is_some() {
                steps += 1;
                if striding {
                    striding_steps += 1;
                }
            }
            if striding {
                steps += 1; // the stride ends
            }
        }
        assert!(steps > 5);
        assert_eq!(striding_steps, 0);
    }

    #[test]
    fn a_walked_npc_ends_each_route_with_a_wait() {
        let map = open(9, 9);
        let none = HashSet::new();
        let mut brain = Brain::new(Behavior { pause: (2, 2), ..wanderer(3) }, t(4, 4), 6);
        brain.state.rest = 0.0;
        let mut idle_ticks_after_walk = 0;
        let mut walking = false;
        for _ in 0..400 {
            let intent = brain.tick(0.1, &senses(&map, &none, Attention::Free));
            if intent.step.is_some() {
                walking = true;
            } else if walking && brain.route_len() == 0 {
                idle_ticks_after_walk += 1;
            }
        }
        assert!(idle_ticks_after_walk >= 15, "about two seconds of standing per walk, got {idle_ticks_after_walk} ticks");
    }

    // ---- conversation

    #[test]
    fn an_approached_npc_takes_no_new_step_and_a_talking_one_faces_the_player() {
        let map = open(9, 9);
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(3), t(4, 4), 11);
        brain.state.rest = 0.0;
        for attention in [Attention::Approached, Attention::Talking] {
            let mut moved = false;
            let mut faced = false;
            for _ in 0..300 {
                let intent = brain.tick(0.1, &Senses { player_tile: t(1, 1), ..senses(&map, &none, attention) });
                moved |= intent.step.is_some();
                faced |= intent.face == Some(t(1, 1));
            }
            assert!(!moved, "{attention:?}");
            assert!(faced, "{attention:?}: turns toward the player");
        }
    }

    #[test]
    fn an_engaged_npc_finishes_its_stride_before_turning() {
        let map = open(5, 5);
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(2), t(2, 2), 4);
        let striding = Senses { striding: true, ..senses(&map, &none, Attention::Talking) };
        let intent = brain.tick(0.1, &striding);
        assert_eq!(intent, Intent::default(), "still gliding: no step, no turn yet");
        let done = brain.tick(0.1, &senses(&map, &none, Attention::Talking));
        assert_eq!(done.face, Some(t(0, 0)));
    }

    #[test]
    fn the_end_of_a_conversation_is_answered_with_one_wave() {
        let map = open(9, 9);
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(3), t(4, 4), 3);
        brain.state.rest = 0.0;
        brain.tick(0.1, &senses(&map, &none, Attention::Talking));
        let goodbye = brain.tick(0.1, &Senses { player_tile: t(2, 4), ..senses(&map, &none, Attention::Free) });
        assert_eq!(goodbye.emote, Some(EmoteOrder { emote: Emote::Wave, farewell: true }));
        assert_eq!(goodbye.face, Some(t(2, 4)));
        // Standing still for the whole wave, with no second wave.
        let mut waves = 0;
        let mut stepped = false;
        let mut time = 0.0;
        while time < Emote::Wave.duration() - 0.3 {
            let intent = brain.tick(0.1, &senses(&map, &none, Attention::Free));
            waves += usize::from(intent.emote.is_some());
            stepped |= intent.step.is_some();
            time += 0.1;
        }
        assert_eq!((waves, stepped), (0, false));
        // Afterwards it wanders again.
        let seen = walk_for(&mut brain, &map, &none, 60.0);
        assert!(seen.len() > 1);
    }

    #[test]
    fn the_wave_waits_for_the_goodbye_not_for_the_approach() {
        let map = open(9, 9);
        let none = HashSet::new();
        let mut brain = Brain::new(Behavior::default(), t(4, 4), 3);
        // Approached, then the player changed their mind: no conversation happened, no wave.
        brain.tick(0.1, &senses(&map, &none, Attention::Approached));
        for _ in 0..50 {
            let intent = brain.tick(0.1, &senses(&map, &none, Attention::Free));
            assert_eq!(intent.emote, None);
        }
    }

    #[test]
    fn talking_again_cancels_a_goodbye_that_is_still_owed() {
        let map = open(9, 9);
        let none = HashSet::new();
        let mut brain = Brain::new(Behavior::default(), t(4, 4), 3);
        brain.tick(0.1, &senses(&map, &none, Attention::Talking));
        // The conversation ends and restarts within one frame pair.
        assert!(brain.tick(0.1, &senses(&map, &none, Attention::Free)).emote.is_some());
        brain.tick(0.1, &senses(&map, &none, Attention::Talking));
        let mut waves = 0;
        brain.tick(0.1, &senses(&map, &none, Attention::Talking));
        for _ in 0..60 {
            waves += usize::from(brain.tick(0.1, &senses(&map, &none, Attention::Free)).emote.map_or(false, |e| e.farewell && e.emote == Emote::Wave));
        }
        assert_eq!(waves, 1, "one wave for the second ending, none left over from the first");
    }

    #[test]
    fn a_stationary_npc_waves_goodbye_too() {
        let map = open(5, 5);
        let none = HashSet::new();
        let mut brain = Brain::new(Behavior::default(), t(2, 2), 1);
        brain.tick(0.1, &senses(&map, &none, Attention::Talking));
        let intent = brain.tick(0.1, &senses(&map, &none, Attention::Free));
        assert_eq!(intent.emote.map(|e| (e.emote, e.farewell)), Some((Emote::Wave, true)));
        assert_eq!(brain.tile(), t(2, 2));
    }

    // ---- emotes

    #[test]
    fn resting_npcs_play_only_the_emotes_they_were_given() {
        let map = open(5, 5);
        let none = HashSet::new();
        let behavior = Behavior { pause: (1, 1), emotes: vec![Emote::Cheer, Emote::Dance], ..Behavior::default() };
        let mut brain = Brain::new(behavior, t(2, 2), 21);
        let mut seen = HashSet::new();
        for _ in 0..6000 {
            let intent = brain.tick(0.1, &senses(&map, &none, Attention::Free));
            if let Some(order) = intent.emote {
                assert!(!order.farewell);
                seen.insert(order.emote);
            }
        }
        assert_eq!(seen, HashSet::from([Emote::Cheer, Emote::Dance]));
    }

    #[test]
    fn an_idle_emote_is_not_repeated_back_to_back() {
        let map = open(5, 5);
        let none = HashSet::new();
        let behavior = Behavior { pause: (1, 1), emotes: vec![Emote::Wave], ..Behavior::default() };
        let mut brain = Brain::new(behavior, t(2, 2), 9);
        let mut starts = Vec::new();
        for i in 0..3000 {
            if brain.tick(0.1, &senses(&map, &none, Attention::Free)).emote.is_some() {
                starts.push(i as f32 * 0.1);
            }
        }
        assert!(starts.len() > 5);
        assert!(starts.windows(2).all(|w| w[1] - w[0] >= Emote::Wave.duration()), "an emote ends before the next begins: {starts:?}");
    }

    #[test]
    fn an_npc_without_emotes_never_gestures() {
        let map = open(5, 5);
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(2), t(2, 2), 1);
        for _ in 0..3000 {
            assert_eq!(brain.tick(0.1, &senses(&map, &none, Attention::Free)).emote, None);
        }
    }

    #[test]
    fn bad_time_steps_change_nothing() {
        let map = open(5, 5);
        let none = HashSet::new();
        let mut brain = Brain::new(wanderer(2), t(2, 2), 1);
        let before = brain.state.rest;
        for dt in [-1.0, f32::NAN, f32::INFINITY, 0.0] {
            let intent = brain.tick(dt, &senses(&map, &none, Attention::Free));
            assert_eq!(intent.emote, None);
        }
        assert_eq!(brain.state.rest, before);
    }

    #[test]
    fn brains_start_at_different_moments() {
        let starts: HashSet<u32> = (0..20)
            .map(|i| Brain::new(Behavior::default(), t(0, 0), seed_from(&format!("npc{i}"))).state.rest as u32 * 10)
            .collect();
        assert!(starts.len() > 1);
        let b = Brain::new(Behavior::default(), t(3, 4), 1);
        assert_eq!((b.tile(), b.home(), b.behavior().mode), (t(3, 4), t(3, 4), Mode::Stay));
    }
}
