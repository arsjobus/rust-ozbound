//! End-to-end check of the M10 acceptance criteria using only the public API (no Bevy): the
//! shipped villagers think with the shared behavior tree, some stay put and some roam inside
//! their range, they play the same emotes as the player, and whoever the player talks to stops,
//! faces the player, and waves goodbye. The simulation below is the Bevy layer's loop without
//! the rendering: build the avoid set, tick every brain, glide each stride to its end.

use std::collections::{HashMap, HashSet};

use ozbound::emote::Emote;
use ozbound::level::{Level, GUIDE_ID};
use ozbound::npc::{in_range, may_step, seed_from, Attention, Brain, Intent, Mode, Senses};
use ozbound::village::builtin;
use ozbound::world::TilePos;

fn level() -> Level {
    builtin().expect("the built-in world is valid").level
}

/// Every person in the level with their brain, as the game spawns them.
fn cast(level: &Level) -> Vec<(String, String, Brain)> {
    level
        .npc_list()
        .into_iter()
        .map(|(id, tile, name)| (id.to_string(), name.to_string(), Brain::new(level.behavior_of(id), tile, seed_from(id))))
        .collect()
}

struct Log {
    /// `(who, from, to, seconds in)` for every step.
    steps: Vec<(String, TilePos, TilePos, f32)>,
    /// `(who, emote, is_farewell, seconds in)` for every emote started.
    emotes: Vec<(String, Emote, bool, f32)>,
    /// A tile held by two people at the same moment.
    collisions: usize,
}

/// Runs everyone for `seconds` at 10 ticks a second. `player` stands still on a tile.
fn simulate(level: &Level, people: &mut [(String, String, Brain)], seconds: f32, player: TilePos, attention: impl Fn(&str, f32) -> Attention) -> Log {
    let keep_out: HashSet<TilePos> = level.npc_keep_out().into_iter().collect();
    let mut log = Log { steps: Vec::new(), emotes: Vec::new(), collisions: 0 };
    let mut time = 0.0;
    while time < seconds {
        let mut avoid = keep_out.clone();
        avoid.extend(people.iter().map(|(_, _, b)| b.tile()));
        avoid.insert(player);
        for (id, _, brain) in people.iter_mut() {
            let before = brain.tile();
            let senses = Senses { map: &level.map, attention: attention(id, time), player_tile: player, striding: false, avoid: &avoid };
            let Intent { step, emote, .. } = brain.tick(0.1, &senses);
            if let Some(step) = step {
                avoid.insert(step);
                log.steps.push((id.clone(), before, step, time));
            }
            if let Some(order) = emote {
                log.emotes.push((id.clone(), order.emote, order.farewell, time));
            }
        }
        let tiles: Vec<TilePos> = people.iter().map(|(_, _, b)| b.tile()).collect();
        log.collisions += tiles.len() - tiles.iter().collect::<HashSet<_>>().len();
        time += 0.1;
    }
    log
}

#[test]
fn the_shipped_villagers_mix_stationary_and_roaming_behaviors() {
    let level = level();
    let modes: HashMap<String, Mode> = level.npc_list().iter().map(|(id, ..)| (id.to_string(), level.behavior_of(id).mode)).collect();
    assert_eq!(modes[GUIDE_ID], Mode::Stay);
    assert!(modes.values().any(|m| matches!(m, Mode::Stay)), "someone stays put");
    assert!(modes.values().any(|m| matches!(m, Mode::Wander { .. })), "someone roams");
    assert!(level.behaviors.iter().all(|b| b.behavior.radius().map_or(true, |r| r >= 1)));
}

#[test]
fn stationary_villagers_never_move_and_wanderers_stay_in_range_on_legal_ground() {
    let level = level();
    let mut people = cast(&level);
    let homes: HashMap<String, TilePos> = people.iter().map(|(id, _, b)| (id.clone(), b.home())).collect();
    let log = simulate(&level, &mut people, 900.0, level.player_start, |_, _| Attention::Free);
    let keep_out: HashSet<TilePos> = level.npc_keep_out().into_iter().collect();
    let mut moved: HashSet<&str> = HashSet::new();
    for (id, from, to, _) in &log.steps {
        moved.insert(id);
        let behavior = level.behavior_of(id);
        let radius = behavior.radius().unwrap_or_else(|| panic!("{id} is stationary but stepped"));
        assert!(may_step(&level.map, *from, *to, homes[id], radius, &HashSet::new()), "{id}: {from:?} -> {to:?}");
        assert!(in_range(homes[id], *to, radius), "{id} left its range");
        assert!(!keep_out.contains(to), "{id} stepped onto {to:?}, which must stay clear");
        assert_ne!(*to, level.player_start, "nobody steps onto the player");
    }
    for (id, ..) in level.npc_list() {
        assert_eq!(moved.contains(id), level.behavior_of(id).radius().is_some(), "{id}: only wanderers walk, and every wanderer does");
    }
    assert_eq!(log.collisions, 0, "two people never hold the same tile");
}

#[test]
fn wanderers_cover_a_good_part_of_their_range() {
    let level = level();
    let mut people = cast(&level);
    let log = simulate(&level, &mut people, 1800.0, level.player_start, |_, _| Attention::Free);
    for (id, _, brain) in &people {
        let Some(radius) = brain.behavior().radius() else { continue };
        let visited: HashSet<TilePos> = log.steps.iter().filter(|(who, ..)| who == id).map(|(_, _, to, _)| *to).collect();
        let possible = ozbound::npc::wander_area(&level.map, brain.home(), radius, &level.npc_keep_out().into_iter().collect());
        assert!(visited.len() * 3 >= possible.min(30), "{id} saw {} of {possible} tiles", visited.len());
    }
}

#[test]
fn every_villager_does_the_emotes_it_was_given_and_nothing_else() {
    let level = level();
    let mut people = cast(&level);
    let log = simulate(&level, &mut people, 1800.0, level.player_start, |_, _| Attention::Free);
    for (id, ..) in level.npc_list() {
        let allowed = level.behavior_of(id).emotes;
        let played: HashSet<Emote> = log.emotes.iter().filter(|(who, ..)| who == id).map(|(_, e, _, _)| *e).collect();
        assert!(played.iter().all(|e| allowed.contains(e)), "{id} played {played:?} but may only play {allowed:?}");
        assert_eq!(played.is_empty(), allowed.is_empty(), "{id}: those with emotes use them, the rest never do");
    }
    assert!(log.emotes.iter().all(|(_, _, farewell, _)| !farewell), "no goodbyes without a conversation");
    let used: HashSet<Emote> = log.emotes.iter().map(|(_, e, _, _)| *e).collect();
    assert_eq!(used.len(), Emote::ALL.len(), "the village uses wave, dance, and cheer");
}

#[test]
fn the_same_world_always_plays_out_the_same_way() {
    let level = level();
    let run = || {
        let mut people = cast(&level);
        let log = simulate(&level, &mut people, 300.0, level.player_start, |_, _| Attention::Free);
        (log.steps, log.emotes.iter().map(|(id, e, f, _)| (id.clone(), e.command(), *f)).collect::<Vec<_>>())
    };
    assert_eq!(run(), run());
}

#[test]
fn a_conversation_holds_the_villager_still_and_ends_in_exactly_one_wave() {
    let level = level();
    let wanderers: Vec<String> = level.npc_list().iter().filter(|(id, ..)| level.behavior_of(id).radius().is_some()).map(|(id, ..)| id.to_string()).collect();
    assert!(!wanderers.is_empty());
    for target in wanderers {
        let mut people = cast(&level);
        // Let everyone settle into their day first, then the player walks up (30 s), talks
        // (30 s), and the conversation ends at 60 s.
        simulate(&level, &mut people, 120.0, level.player_start, |_, _| Attention::Free);
        let spot = people.iter().find(|(id, ..)| *id == target).map(|(_, _, b)| b.tile()).unwrap();
        let phase = |id: &str, t: f32| match (id == target, t) {
            (false, _) => Attention::Free,
            (true, t) if t < 30.0 => Attention::Approached,
            (true, t) if t < 60.0 => Attention::Talking,
            _ => Attention::Free,
        };
        let log = simulate(&level, &mut people, 120.0, spot, phase);

        let steps: Vec<f32> = log.steps.iter().filter(|(who, ..)| *who == target).map(|(.., t)| *t).collect();
        // A stride already under way when the player clicks may finish, but no new one starts.
        assert!(steps.iter().all(|t| *t < 0.2 || *t >= 60.0), "{target} walked off during the conversation: {steps:?}");
        let waves: Vec<f32> = log.emotes.iter().filter(|(who, e, farewell, _)| *who == target && *farewell && *e == Emote::Wave).map(|(.., t)| *t).collect();
        assert_eq!(waves.len(), 1, "{target} waves goodbye exactly once");
        assert!((60.0..60.5).contains(&waves[0]), "the wave comes as the conversation ends, not {}", waves[0]);
        let idle_emotes = log.emotes.iter().filter(|(who, _, farewell, t)| *who == target && !*farewell && (0.2..60.0).contains(t)).count();
        assert_eq!(idle_emotes, 0, "no idle gestures while the player is there");
        assert!(steps.iter().any(|t| *t >= 60.0), "{target} goes back to their day afterwards");
        assert!(people.iter().all(|(_, _, b)| in_range(b.home(), b.tile(), b.behavior().radius().unwrap_or(0))), "everyone is still inside their range");
    }
}

#[test]
fn a_talking_villager_turns_toward_the_player_and_the_stationary_guide_waves_too() {
    let level = level();
    let mut people = cast(&level);
    let guide = people.iter().position(|(id, ..)| id == GUIDE_ID).unwrap();
    let player = TilePos::new(level.guide.unwrap().x + 1, level.guide.unwrap().y);
    let none = HashSet::new();
    let talking = Senses { map: &level.map, attention: Attention::Talking, player_tile: player, striding: false, avoid: &none };
    let intent = people[guide].2.tick(0.1, &talking);
    assert_eq!(intent.face, Some(player), "the guide looks at the player");
    assert_eq!(intent.step, None);
    let free = Senses { attention: Attention::Free, ..talking };
    let goodbye = people[guide].2.tick(0.1, &free);
    assert_eq!(goodbye.emote.map(|e| (e.emote, e.farewell)), Some((Emote::Wave, true)));
    assert_eq!(people[guide].2.tile(), level.guide.unwrap(), "a stationary guide never leaves its tile");
}

#[test]
fn the_overworld_composite_keeps_every_behavior() {
    let world = builtin().expect("valid world");
    let overworld = world.overworld.as_ref().expect("Bluebrook ships as an open world");
    let level_of = |id: &str| if id == world.start_id() { Some(&world.level) } else { world.level_of(id) };
    let composite = overworld.compose(&level_of, &[world.start_id(), "YELLOW_BRICK_ROAD_EAST"]);
    assert!(!world.level.behaviors.is_empty());
    for spawn in &world.level.behaviors {
        assert_eq!(composite.level.behavior_of(&spawn.id), spawn.behavior, "{}", spawn.id);
    }
}

#[test]
fn walking_up_to_a_villager_and_changing_your_mind_earns_no_wave_either_way() {
    let level = level();
    let mut people = cast(&level);
    let target = people.iter().find(|(_, _, b)| b.behavior().radius().is_some()).map(|(id, ..)| id.clone()).unwrap();
    let log = simulate(&level, &mut people, 120.0, level.player_start, |id, t| if id == target && t < 20.0 { Attention::Approached } else { Attention::Free });
    assert!(log.emotes.iter().all(|(_, _, farewell, _)| !farewell));
}
