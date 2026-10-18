//! Skills and progression.
//!
//! Pure Rust (no Bevy). Four skills grow with use: Foraging, Woodcraft, Herbalism, and
//! Wayfinding. Skill levels unlock recipes (`require = ...` in a recipe file) and dialogue
//! choices (`skill <id> at_least <n>` in a dialogue file); nothing else depends on them yet.
//!
//! # The XP curve
//!
//! The total XP needed to *reach* level `L` (levels run from 1 to [`MAX_LEVEL`]) is
//! `XP_STEP * (L - 1)^2`, with `XP_STEP = 30`:
//!
//! | Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
//! | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
//! | Total XP | 0 | 30 | 120 | 270 | 480 | 750 | 1080 | 1470 | 1920 | 2430 |
//!
//! There is no randomness anywhere: the same actions always give the same XP. XP is capped
//! at the level-10 total, so a maxed skill simply stops growing.
//!
//! # Keeping progression useful without grind
//!
//! - Every XP source is an [`Award`] with an `until` level. Once the skill has reached that
//!   level the source teaches nothing more, so repeating one easy action cannot carry a
//!   skill to the top; later levels need the harder, gated recipes. (An award that starts
//!   below the ceiling may carry the total slightly past it, and that XP is kept.)
//! - Gates stay reachable: [`check_gates`] refuses content that asks for a level nothing in
//!   the game can teach, so no recipe or dialogue choice is locked forever.
//! - There are no skill points, decay, daily bonuses, or timers.
//!
//! Skills are part of the save file (`save.rs`); `Skills::from_saved` rebuilds them and
//! refuses values that play could not have produced.

use std::fmt;

use crate::crafting::RecipeBook;
use crate::dialogue::DialogueBook;
use crate::gathering::ResourceKind;
use crate::textdata::ContentError;

/// The highest skill level.
pub const MAX_LEVEL: u32 = 10;
/// Scale of the XP curve (see the module docs).
pub const XP_STEP: u32 = 30;
/// Total XP of a skill at [`MAX_LEVEL`]; XP never goes above this.
pub const MAX_XP: u32 = XP_STEP * (MAX_LEVEL - 1) * (MAX_LEVEL - 1);
/// Tiles walked for one point of Wayfinding XP.
pub const WALK_TILES_PER_XP: u32 = 10;
/// Walking teaches Wayfinding only up to this level.
pub const WALK_UNTIL: u32 = 3;

/// Total XP needed to reach `level`. Levels below 1 count as 1 and above [`MAX_LEVEL`] as the max.
pub fn xp_for_level(level: u32) -> u32 {
    let steps = level.clamp(1, MAX_LEVEL) - 1;
    XP_STEP * steps * steps
}

/// The level that `xp` total XP gives, from 1 to [`MAX_LEVEL`].
pub fn level_for_xp(xp: u32) -> u32 {
    (1..=MAX_LEVEL).rev().find(|level| xp >= xp_for_level(*level)).unwrap_or(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillId {
    Foraging,
    Woodcraft,
    Herbalism,
    Wayfinding,
}

impl SkillId {
    /// Every skill, in a fixed order (listings, lookups, tests).
    pub const ALL: [SkillId; 4] = [Self::Foraging, Self::Woodcraft, Self::Herbalism, Self::Wayfinding];

    /// Stable lower-case key used in data files. Never change an existing key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Foraging => "foraging",
            Self::Woodcraft => "woodcraft",
            Self::Herbalism => "herbalism",
            Self::Wayfinding => "wayfinding",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|skill| skill.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Foraging => "Foraging",
            Self::Woodcraft => "Woodcraft",
            Self::Herbalism => "Herbalism",
            Self::Wayfinding => "Wayfinding",
        }
    }

    /// One line on what the skill is and how it grows.
    pub fn description(self) -> &'static str {
        match self {
            Self::Foraging => "Finding wild things: moonpetals and honeyberries.",
            Self::Woodcraft => "Outdoor handiwork: felling timber, working ore, and making gear.",
            Self::Herbalism => "Knowing herbs: gathering meadowmint and brewing with it.",
            Self::Wayfinding => "Learning the land by travelling across it.",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Foraging => 0,
            Self::Woodcraft => 1,
            Self::Herbalism => 2,
            Self::Wayfinding => 3,
        }
    }

    /// Valid keys for error messages: "foraging, woodcraft, herbalism, or wayfinding".
    pub fn key_list() -> String {
        let keys: Vec<&str> = Self::ALL.iter().map(|s| s.key()).collect();
        format!("{}, or {}", keys[..keys.len() - 1].join(", "), keys[keys.len() - 1])
    }
}

pub(crate) fn parse_skill(line: usize, key: &str) -> Result<SkillId, ContentError> {
    SkillId::from_key(key)
        .ok_or_else(|| ContentError::at(line, format!("unknown skill `{key}` (use {})", SkillId::key_list())))
}

/// Parses a level between `min` and [`MAX_LEVEL`].
pub(crate) fn parse_level(line: usize, what: &str, text: &str, min: u32) -> Result<u32, ContentError> {
    match text.parse::<u32>() {
        Ok(level) if (min..=MAX_LEVEL).contains(&level) => Ok(level),
        _ => Err(ContentError::at(
            line,
            format!("{what} must be a whole number from {min} to {MAX_LEVEL}, found `{text}`"),
        )),
    }
}

/// A minimum skill level, for example `woodcraft 3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillRequirement {
    pub skill: SkillId,
    pub level: u32,
}

impl SkillRequirement {
    /// Parses `<skill> <level>` with a level from 2 to [`MAX_LEVEL`] (level 1 is always met,
    /// so asking for it is almost certainly a mistake).
    pub fn parse(line: usize, text: &str) -> Result<Self, ContentError> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let [skill, level] = words.as_slice() else {
            return Err(ContentError::at(line, "a skill requirement needs a skill and a level, like `woodcraft 3`"));
        };
        Ok(Self { skill: parse_skill(line, skill)?, level: parse_level(line, "the skill level", level, 2)? })
    }

    /// "Woodcraft level 3".
    pub fn describe(&self) -> String {
        format!("{} level {}", self.skill.label(), self.level)
    }
}

/// XP for doing one thing, which stops once the skill has reached `until`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Award {
    pub skill: SkillId,
    pub xp: u32,
    /// The skill level at which this source stops teaching (2 to [`MAX_LEVEL`]).
    pub until: u32,
}

impl Award {
    /// Parses `<skill> <xp> <until-level>`, for example `woodcraft 25 5`.
    pub fn parse(line: usize, text: &str) -> Result<Self, ContentError> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let [skill, xp, until] = words.as_slice() else {
            return Err(ContentError::at(
                line,
                "`xp` needs a skill, an amount, and the level where it stops, like `woodcraft 25 5`",
            ));
        };
        Ok(Self {
            skill: parse_skill(line, skill)?,
            xp: crate::textdata::parse_positive(line, "the XP amount", xp)?,
            until: parse_level(line, "the level where the XP stops", until, 2)?,
        })
    }
}

/// What one award did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XpGain {
    pub skill: SkillId,
    /// XP actually added (less than offered when it hit the cap).
    pub gained: u32,
    pub level_before: u32,
    pub level_after: u32,
}

impl XpGain {
    pub fn leveled_up(&self) -> bool {
        self.level_after > self.level_before
    }

    /// A journal line for a level-up, or `None` when the level did not change.
    pub fn message(&self) -> Option<String> {
        self.leveled_up().then(|| format!("Your {} level is now {}.", self.skill.label(), self.level_after))
    }
}

/// What gathering from `kind` teaches. Every kind teaches something; the table is in one place
/// so the numbers can be tuned together.
pub fn gather_award(kind: ResourceKind) -> Award {
    let (skill, xp, until) = match kind {
        ResourceKind::Moonpetal => (SkillId::Foraging, 10, 3),
        ResourceKind::BerryBush => (SkillId::Foraging, 6, 5),
        ResourceKind::HerbPatch => (SkillId::Herbalism, 6, 4),
        ResourceKind::Tree => (SkillId::Woodcraft, 8, 4),
        ResourceKind::CopperVein => (SkillId::Woodcraft, 10, 4),
    };
    Award { skill, xp, until }
}

/// What walking teaches: one XP per [`WALK_TILES_PER_XP`] tiles, up to level [`WALK_UNTIL`].
pub fn walk_award() -> Award {
    Award { skill: SkillId::Wayfinding, xp: 1, until: WALK_UNTIL }
}

/// The player's skills: XP per skill (levels follow from it) and the tiles walked toward the
/// next point of Wayfinding XP.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Skills {
    xp: [u32; 4],
    walked: u32,
}

impl Skills {
    /// A new character: every skill at level 1 with no XP.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every skill exactly at `level` (clamped to 1..=[`MAX_LEVEL`]), with no XP beyond it.
    /// For tests, tools, and starting states.
    pub fn at_level(level: u32) -> Self {
        Self { xp: [xp_for_level(level); 4], walked: 0 }
    }

    /// Rebuilds skills from saved XP and the tiles walked toward the next Wayfinding point.
    /// XP may not exceed [`MAX_XP`]; the walked count must be below [`WALK_TILES_PER_XP`] and
    /// is only kept while Wayfinding is below [`WALK_UNTIL`], exactly as `walked` keeps it.
    pub fn from_saved(xp: [u32; 4], walked: u32) -> Result<Self, String> {
        for skill in SkillId::ALL {
            if xp[skill.index()] > MAX_XP {
                return Err(format!("{} XP {} is above the maximum of {MAX_XP}", skill.label(), xp[skill.index()]));
            }
        }
        if walked >= WALK_TILES_PER_XP {
            return Err(format!("walked count {walked} must be below {WALK_TILES_PER_XP}"));
        }
        let skills = Self { xp, walked };
        if walked > 0 && skills.level(walk_award().skill) >= WALK_UNTIL {
            return Err(format!("a walked count of {walked} is not kept once Wayfinding has reached level {WALK_UNTIL}"));
        }
        Ok(skills)
    }

    /// Tiles walked toward the next point of Wayfinding XP (always below [`WALK_TILES_PER_XP`]).
    pub fn walked_progress(&self) -> u32 {
        self.walked
    }

    pub fn xp(&self, skill: SkillId) -> u32 {
        self.xp[skill.index()]
    }

    pub fn level(&self, skill: SkillId) -> u32 {
        level_for_xp(self.xp(skill))
    }

    /// `(XP earned toward the next level, XP the next level needs)`, or `None` at the top.
    pub fn progress(&self, skill: SkillId) -> Option<(u32, u32)> {
        let level = self.level(skill);
        if level >= MAX_LEVEL {
            return None;
        }
        let base = xp_for_level(level);
        Some((self.xp(skill) - base, xp_for_level(level + 1) - base))
    }

    pub fn meets(&self, requirement: &SkillRequirement) -> bool {
        self.level(requirement.skill) >= requirement.level
    }

    /// The requirements not yet met, in the order given.
    pub fn unmet(&self, requirements: &[SkillRequirement]) -> Vec<SkillRequirement> {
        requirements.iter().filter(|r| !self.meets(r)).copied().collect()
    }

    /// Adds XP, never going above [`MAX_XP`]. Zero XP changes nothing.
    pub fn add_xp(&mut self, skill: SkillId, amount: u32) -> XpGain {
        let level_before = self.level(skill);
        let slot = &mut self.xp[skill.index()];
        let before = *slot;
        *slot = before.saturating_add(amount).min(MAX_XP);
        XpGain { skill, gained: *slot - before, level_before, level_after: level_for_xp(*slot) }
    }

    /// Adds an award's XP unless the skill has already reached the award's `until` level.
    /// Returns `None` when nothing was earned.
    pub fn award(&mut self, award: Award) -> Option<XpGain> {
        if award.xp == 0 || self.level(award.skill) >= award.until {
            return None;
        }
        let gain = self.add_xp(award.skill, award.xp);
        (gain.gained > 0).then_some(gain)
    }

    /// The XP for one successful gather from `kind`.
    pub fn gathered(&mut self, kind: ResourceKind) -> Option<XpGain> {
        self.award(gather_award(kind))
    }

    /// Counts tiles walked. Every [`WALK_TILES_PER_XP`] tiles earn a point of Wayfinding XP while
    /// that skill is below [`WALK_UNTIL`]; at or above it the count is not kept.
    pub fn walked(&mut self, tiles: u32) -> Option<XpGain> {
        let walk = walk_award();
        if self.level(walk.skill) >= walk.until {
            self.walked = 0;
            return None;
        }
        self.walked = self.walked.saturating_add(tiles);
        let points = self.walked / WALK_TILES_PER_XP;
        self.walked %= WALK_TILES_PER_XP;
        if points == 0 {
            return None;
        }
        self.award(Award { xp: points, ..walk })
    }

    /// "Skills: Foraging 2 (12/90), Woodcraft 1 (8/30), ..." with "(max)" at the top level.
    pub fn summary_line(&self) -> String {
        let parts: Vec<String> = SkillId::ALL
            .iter()
            .map(|skill| match self.progress(*skill) {
                Some((have, need)) => format!("{} {} ({have}/{need})", skill.label(), self.level(*skill)),
                None => format!("{} {} (max)", skill.label(), self.level(*skill)),
            })
            .collect();
        format!("Skills: {}", parts.join(", "))
    }
}

impl fmt::Display for SkillRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.describe())
    }
}

/// The highest level each skill can reach with the sources in the game: gathering, walking,
/// and the recipes in `recipes` (a recipe only counts once its own requirements are met, so
/// this follows chains such as "level 3 comes from level 2 gear"). Indexed like `SkillId::ALL`.
pub fn reachable_levels(recipes: &RecipeBook) -> [u32; 4] {
    let mut sources: Vec<(Award, Vec<SkillRequirement>)> =
        ResourceKind::ALL.iter().map(|kind| (gather_award(*kind), Vec::new())).collect();
    sources.push((walk_award(), Vec::new()));
    for recipe in recipes.all() {
        if let Some(award) = recipe.xp {
            sources.push((award, recipe.requires.clone()));
        }
    }
    let mut levels = [1u32; 4];
    loop {
        let mut changed = false;
        for (award, requires) in &sources {
            let usable = requires.iter().all(|r| levels[r.skill.index()] >= r.level);
            let reach = award.until.min(MAX_LEVEL);
            if usable && reach > levels[award.skill.index()] {
                levels[award.skill.index()] = reach;
                changed = true;
            }
        }
        if !changed {
            return levels;
        }
    }
}

/// The highest level `skill` can reach (see [`reachable_levels`]).
pub fn reachable_level(recipes: &RecipeBook, skill: SkillId) -> u32 {
    reachable_levels(recipes)[skill.index()]
}

/// Refuses content whose skill gates can never be opened: a recipe or a dialogue choice that
/// asks for more of a skill than anything in the game teaches. The message names the recipe or
/// the dialogue line to fix.
pub fn check_gates(recipes: &RecipeBook, dialogue: &DialogueBook) -> Result<(), String> {
    let reach = reachable_levels(recipes);
    for recipe in recipes.all() {
        for req in &recipe.requires {
            let top = reach[req.skill.index()];
            if req.level > top {
                return Err(format!(
                    "recipe `{}` (line {}) needs {}, but nothing in the game teaches {} past level {top}",
                    recipe.id,
                    recipe.line,
                    req.describe(),
                    req.skill.label()
                ));
            }
        }
    }
    for (line, req) in dialogue.skill_requirements() {
        let top = reach[req.skill.index()];
        if req.level > top {
            return Err(format!(
                "dialogue line {line} needs {}, but nothing in the game teaches {} past level {top}",
                req.describe(),
                req.skill.label()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_saved_round_trips_what_play_produces() {
        let mut played = Skills::new();
        played.add_xp(SkillId::Woodcraft, 130);
        played.add_xp(SkillId::Foraging, 31);
        played.walked(7);
        let xp = SkillId::ALL.map(|s| played.xp(s));
        let restored = Skills::from_saved(xp, played.walked_progress()).unwrap();
        assert_eq!(restored, played);
        assert_eq!(restored.walked_progress(), 7);
    }

    #[test]
    fn from_saved_refuses_impossible_values() {
        assert!(Skills::from_saved([MAX_XP + 1, 0, 0, 0], 0).unwrap_err().contains("Foraging"));
        assert!(Skills::from_saved([0; 4], WALK_TILES_PER_XP).unwrap_err().contains("walked"));
        // Wayfinding at its walking ceiling keeps no walked count.
        let ceiling = xp_for_level(WALK_UNTIL);
        assert!(Skills::from_saved([0, 0, 0, ceiling], 3).unwrap_err().contains("Wayfinding"));
        assert!(Skills::from_saved([0, 0, 0, ceiling], 0).is_ok());
        assert!(Skills::from_saved([MAX_XP; 4], 0).is_ok());
    }

    fn book() -> RecipeBook {
        RecipeBook::builtin().unwrap()
    }

    // ---- the curve ----

    #[test]
    fn the_curve_matches_the_documented_table() {
        let table = [0, 30, 120, 270, 480, 750, 1080, 1470, 1920, 2430];
        for (index, total) in table.iter().enumerate() {
            assert_eq!(xp_for_level(index as u32 + 1), *total, "level {}", index + 1);
        }
        assert_eq!(MAX_XP, 2430);
    }

    #[test]
    fn the_curve_always_rises_and_levels_round_trip() {
        for level in 1..MAX_LEVEL {
            assert!(xp_for_level(level + 1) > xp_for_level(level), "level {level}");
        }
        for level in 1..=MAX_LEVEL {
            assert_eq!(level_for_xp(xp_for_level(level)), level, "exactly at the threshold");
            if level > 1 {
                assert_eq!(level_for_xp(xp_for_level(level) - 1), level - 1, "one XP short");
            }
        }
    }

    #[test]
    fn out_of_range_levels_and_xp_are_clamped() {
        assert_eq!(xp_for_level(0), 0);
        assert_eq!(xp_for_level(99), MAX_XP);
        assert_eq!(level_for_xp(0), 1);
        assert_eq!(level_for_xp(u32::MAX), MAX_LEVEL);
    }

    // ---- skills ----

    #[test]
    fn a_new_character_is_level_one_everywhere() {
        let skills = Skills::new();
        for skill in SkillId::ALL {
            assert_eq!((skills.xp(skill), skills.level(skill)), (0, 1));
            assert_eq!(skills.progress(skill), Some((0, 30)));
        }
    }

    #[test]
    fn skill_keys_labels_and_descriptions_are_unique_and_round_trip() {
        for skill in SkillId::ALL {
            assert_eq!(SkillId::from_key(skill.key()), Some(skill));
            assert!(!skill.label().is_empty() && !skill.description().is_empty());
            assert_eq!(skill.key(), skill.label().to_lowercase());
        }
        let mut keys: Vec<_> = SkillId::ALL.iter().map(|s| s.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), SkillId::ALL.len());
        assert_eq!(SkillId::from_key("Foraging"), None, "keys are lower-case");
        assert_eq!(SkillId::key_list(), "foraging, woodcraft, herbalism, or wayfinding");
    }

    #[test]
    fn xp_is_tracked_per_skill() {
        let mut skills = Skills::new();
        skills.add_xp(SkillId::Woodcraft, 40);
        assert_eq!(skills.xp(SkillId::Woodcraft), 40);
        assert_eq!(skills.level(SkillId::Woodcraft), 2);
        assert_eq!(skills.level(SkillId::Foraging), 1);
        assert_eq!(skills.progress(SkillId::Woodcraft), Some((10, 90)));
    }

    #[test]
    fn a_level_up_is_reported_exactly_when_a_threshold_is_crossed() {
        let mut skills = Skills::new();
        let gain = skills.add_xp(SkillId::Herbalism, 29);
        assert_eq!((gain.gained, gain.level_before, gain.level_after), (29, 1, 1));
        assert!(!gain.leveled_up());
        assert_eq!(gain.message(), None);
        let gain = skills.add_xp(SkillId::Herbalism, 1);
        assert!(gain.leveled_up());
        assert_eq!(gain.message().as_deref(), Some("Your Herbalism level is now 2."));
        assert_eq!(skills.level(SkillId::Herbalism), 2);
    }

    #[test]
    fn one_big_award_can_cross_several_levels_and_reports_the_last() {
        let mut skills = Skills::new();
        let gain = skills.add_xp(SkillId::Foraging, 300);
        assert_eq!((gain.level_before, gain.level_after), (1, 4));
        assert_eq!(gain.message().as_deref(), Some("Your Foraging level is now 4."));
    }

    #[test]
    fn xp_stops_at_the_cap_and_only_counts_what_was_added() {
        let mut skills = Skills::at_level(MAX_LEVEL - 1);
        let gain = skills.add_xp(SkillId::Woodcraft, 10_000);
        assert_eq!(skills.xp(SkillId::Woodcraft), MAX_XP);
        assert_eq!(gain.gained, MAX_XP - xp_for_level(MAX_LEVEL - 1));
        assert_eq!(skills.level(SkillId::Woodcraft), MAX_LEVEL);
        assert_eq!(skills.progress(SkillId::Woodcraft), None, "nothing left to earn");
        let again = skills.add_xp(SkillId::Woodcraft, 5);
        assert_eq!((again.gained, again.leveled_up()), (0, false));
        assert_eq!(skills.add_xp(SkillId::Woodcraft, u32::MAX).gained, 0, "no overflow");
    }

    #[test]
    fn zero_xp_changes_nothing() {
        let mut skills = Skills::new();
        let gain = skills.add_xp(SkillId::Foraging, 0);
        assert_eq!((gain.gained, gain.leveled_up()), (0, false));
        assert_eq!(skills, Skills::new());
    }

    #[test]
    fn at_level_puts_every_skill_exactly_on_the_threshold() {
        for level in 1..=MAX_LEVEL {
            let skills = Skills::at_level(level);
            for skill in SkillId::ALL {
                assert_eq!(skills.level(skill), level);
                assert_eq!(skills.xp(skill), xp_for_level(level));
            }
        }
        assert_eq!(Skills::at_level(0), Skills::new());
        assert_eq!(Skills::at_level(99), Skills::at_level(MAX_LEVEL));
    }

    // ---- requirements ----

    #[test]
    fn requirements_are_met_at_or_above_the_level() {
        let req = SkillRequirement { skill: SkillId::Woodcraft, level: 3 };
        assert!(!Skills::new().meets(&req));
        assert!(!Skills::at_level(2).meets(&req));
        assert!(Skills::at_level(3).meets(&req));
        assert!(Skills::at_level(MAX_LEVEL).meets(&req));
        let mut mixed = Skills::new();
        mixed.add_xp(SkillId::Woodcraft, xp_for_level(3));
        assert!(mixed.meets(&req), "only the named skill matters");
        assert!(!mixed.meets(&SkillRequirement { skill: SkillId::Herbalism, level: 2 }));
    }

    #[test]
    fn unmet_lists_only_what_is_missing_in_order() {
        let reqs = [
            SkillRequirement { skill: SkillId::Woodcraft, level: 3 },
            SkillRequirement { skill: SkillId::Herbalism, level: 2 },
        ];
        let mut skills = Skills::new();
        assert_eq!(skills.unmet(&reqs), reqs);
        skills.add_xp(SkillId::Woodcraft, xp_for_level(3));
        assert_eq!(skills.unmet(&reqs), [reqs[1]]);
        skills.add_xp(SkillId::Herbalism, xp_for_level(2));
        assert!(skills.unmet(&reqs).is_empty());
        assert!(Skills::new().unmet(&[]).is_empty());
    }

    #[test]
    fn requirements_parse_and_describe() {
        let req = SkillRequirement::parse(4, "woodcraft 3").unwrap();
        assert_eq!(req, SkillRequirement { skill: SkillId::Woodcraft, level: 3 });
        assert_eq!(req.describe(), "Woodcraft level 3");
        assert_eq!(req.to_string(), "Woodcraft level 3");
        assert_eq!(SkillRequirement::parse(1, "  herbalism   10 ").unwrap().level, 10);
    }

    #[test]
    fn bad_requirements_are_rejected_with_the_line_number() {
        for (text, needle) in [
            ("woodcraft", "a skill and a level"),
            ("woodcraft 3 4", "a skill and a level"),
            ("", "a skill and a level"),
            ("carving 3", "unknown skill `carving`"),
            ("Woodcraft 3", "unknown skill `Woodcraft`"),
            ("woodcraft 1", "from 2 to 10"),
            ("woodcraft 0", "from 2 to 10"),
            ("woodcraft 11", "from 2 to 10"),
            ("woodcraft x", "from 2 to 10"),
            ("woodcraft -2", "from 2 to 10"),
        ] {
            let error = SkillRequirement::parse(7, text).unwrap_err();
            assert_eq!(error.line, Some(7), "{text}");
            assert!(error.message.contains(needle), "{text}: {error}");
        }
        let unknown = SkillRequirement::parse(1, "carving 3").unwrap_err();
        assert!(unknown.message.contains("foraging, woodcraft, herbalism, or wayfinding"), "{unknown}");
    }

    #[test]
    fn awards_parse_and_reject_nonsense() {
        let award = Award::parse(2, "woodcraft 25 5").unwrap();
        assert_eq!(award, Award { skill: SkillId::Woodcraft, xp: 25, until: 5 });
        for (text, needle) in [
            ("woodcraft 25", "a skill, an amount"),
            ("woodcraft 0 5", "above zero"),
            ("woodcraft x 5", "above zero"),
            ("woodcraft 25 1", "from 2 to 10"),
            ("woodcraft 25 11", "from 2 to 10"),
            ("carving 25 5", "unknown skill"),
        ] {
            let error = Award::parse(9, text).unwrap_err();
            assert_eq!(error.line, Some(9), "{text}");
            assert!(error.message.contains(needle), "{text}: {error}");
        }
    }

    // ---- awards and the no-grind ceiling ----

    #[test]
    fn an_award_stops_teaching_once_the_skill_reaches_its_ceiling() {
        let award = Award { skill: SkillId::Woodcraft, xp: 40, until: 3 };
        let mut skills = Skills::new();
        let mut rounds = 0;
        while skills.award(award).is_some() {
            rounds += 1;
            assert!(rounds < 100, "must stop");
        }
        assert_eq!(skills.level(SkillId::Woodcraft), 3);
        assert_eq!(skills.xp(SkillId::Woodcraft), 120, "3 awards of 40 land exactly on level 3");
        assert_eq!(skills.award(award), None);
        assert_eq!(skills.xp(SkillId::Woodcraft), 120, "a refused award changes nothing");
    }

    #[test]
    fn an_award_may_carry_the_total_slightly_past_the_ceiling_and_keeps_it() {
        let award = Award { skill: SkillId::Foraging, xp: 150, until: 2 };
        let mut skills = Skills::new();
        let gain = skills.award(award).unwrap();
        assert_eq!((gain.gained, gain.level_after), (150, 3), "one award may overshoot the ceiling level");
        assert_eq!(skills.award(award), None);
        assert_eq!(skills.xp(SkillId::Foraging), 150, "the overshoot is kept, and nothing more is added");
    }

    #[test]
    fn a_skill_already_past_the_ceiling_earns_nothing_and_a_zero_award_is_ignored() {
        let mut skills = Skills::at_level(5);
        assert_eq!(skills.award(Award { skill: SkillId::Herbalism, xp: 10, until: 5 }), None);
        assert_eq!(skills.award(Award { skill: SkillId::Herbalism, xp: 0, until: 9 }), None);
        assert_eq!(skills, Skills::at_level(5));
    }

    #[test]
    fn an_award_at_the_xp_cap_reports_nothing() {
        let mut skills = Skills::at_level(MAX_LEVEL);
        assert_eq!(skills.award(Award { skill: SkillId::Foraging, xp: 5, until: MAX_LEVEL }), None);
    }

    #[test]
    fn the_same_actions_always_give_the_same_skills() {
        let run = || {
            let mut skills = Skills::new();
            for kind in ResourceKind::ALL.iter().cycle().take(60) {
                skills.gathered(*kind);
            }
            skills.walked(137);
            skills
        };
        assert_eq!(run(), run());
    }

    // ---- gathering ----

    #[test]
    fn every_resource_teaches_a_skill_with_sane_numbers() {
        for kind in ResourceKind::ALL {
            let award = gather_award(kind);
            assert!(award.xp > 0 && award.xp < 30, "{kind:?} should not skip a level in one pick");
            assert!((2..=MAX_LEVEL).contains(&award.until), "{kind:?}");
        }
        assert_eq!(gather_award(ResourceKind::Tree).skill, SkillId::Woodcraft);
        assert_eq!(gather_award(ResourceKind::HerbPatch).skill, SkillId::Herbalism);
        assert_eq!(gather_award(ResourceKind::BerryBush).skill, SkillId::Foraging);
        assert_eq!(gather_award(ResourceKind::Moonpetal).skill, SkillId::Foraging);
    }

    #[test]
    fn gathering_trees_levels_woodcraft_and_then_stops_at_the_ceiling() {
        let mut skills = Skills::new();
        let mut chops = 0;
        let mut first_level_two = None;
        while let Some(gain) = skills.gathered(ResourceKind::Tree) {
            chops += 1;
            if gain.leveled_up() && first_level_two.is_none() {
                first_level_two = Some(chops);
            }
            assert!(chops < 200, "must stop");
        }
        assert_eq!(first_level_two, Some(4), "4 chops at 8 XP reach level 2 (30 XP)");
        assert_eq!(skills.level(SkillId::Woodcraft), gather_award(ResourceKind::Tree).until);
        assert_eq!(skills.gathered(ResourceKind::Tree), None, "easy work teaches nothing more");
        // The other skills were not touched.
        assert_eq!(skills.xp(SkillId::Foraging) + skills.xp(SkillId::Herbalism) + skills.xp(SkillId::Wayfinding), 0);
    }

    #[test]
    fn grinding_one_source_can_never_reach_the_top() {
        for kind in ResourceKind::ALL {
            let mut skills = Skills::new();
            for _ in 0..10_000 {
                skills.gathered(kind);
            }
            let skill = gather_award(kind).skill;
            assert!(skills.level(skill) < MAX_LEVEL, "{kind:?} alone reached level {}", skills.level(skill));
        }
    }

    // ---- walking ----

    #[test]
    fn walking_earns_wayfinding_xp_in_whole_steps_and_keeps_the_remainder() {
        let mut skills = Skills::new();
        assert_eq!(skills.walked(9), None);
        assert_eq!(skills.xp(SkillId::Wayfinding), 0);
        let gain = skills.walked(1).unwrap();
        assert_eq!(gain.gained, 1);
        assert_eq!(skills.xp(SkillId::Wayfinding), 1);
        assert_eq!(skills.walked(25).unwrap().gained, 2);
        assert_eq!(skills.walked(5).unwrap().gained, 1, "5 left over + 5 more = another point");
        assert_eq!(skills.xp(SkillId::Wayfinding), 4);
    }

    #[test]
    fn walking_one_tile_at_a_time_matches_walking_in_bulk() {
        let mut slow = Skills::new();
        for _ in 0..137 {
            slow.walked(1);
        }
        let mut fast = Skills::new();
        fast.walked(137);
        assert_eq!(slow, fast);
        assert_eq!(slow.xp(SkillId::Wayfinding), 13);
    }

    #[test]
    fn walking_teaches_wayfinding_only_up_to_its_ceiling() {
        let mut skills = Skills::new();
        let mut tiles = 0u32;
        while skills.level(SkillId::Wayfinding) < WALK_UNTIL {
            skills.walked(1);
            tiles += 1;
            assert!(tiles < 10_000, "must reach the ceiling");
        }
        assert_eq!(tiles, xp_for_level(WALK_UNTIL) * WALK_TILES_PER_XP, "120 XP at one point per 10 tiles");
        let xp = skills.xp(SkillId::Wayfinding);
        assert_eq!(skills.walked(500), None);
        assert_eq!(skills.xp(SkillId::Wayfinding), xp);
        assert_eq!(skills, {
            let mut same = Skills::new();
            same.add_xp(SkillId::Wayfinding, xp);
            same
        }, "no hidden tile count is kept once walking has nothing to teach");
    }

    #[test]
    fn walking_zero_tiles_does_nothing() {
        let mut skills = Skills::new();
        assert_eq!(skills.walked(0), None);
        assert_eq!(skills, Skills::new());
    }

    // ---- summary ----

    #[test]
    fn the_summary_line_shows_levels_and_progress() {
        assert_eq!(
            Skills::new().summary_line(),
            "Skills: Foraging 1 (0/30), Woodcraft 1 (0/30), Herbalism 1 (0/30), Wayfinding 1 (0/30)"
        );
        let mut skills = Skills::new();
        skills.add_xp(SkillId::Woodcraft, 40);
        skills.add_xp(SkillId::Herbalism, MAX_XP);
        assert_eq!(
            skills.summary_line(),
            "Skills: Foraging 1 (0/30), Woodcraft 2 (10/90), Herbalism 10 (max), Wayfinding 1 (0/30)"
        );
    }

    // ---- reachability ----

    #[test]
    fn shipped_content_can_reach_every_level_it_asks_for() {
        let reach = reachable_levels(&book());
        // Gathering teaches Woodcraft to 4 and Herbalism to 4; the recipes then carry them on.
        assert_eq!(reach[SkillId::Woodcraft.index()], 6, "the copper charm (needs 3) teaches to 6");
        assert_eq!(reach[SkillId::Herbalism.index()], 5);
        assert_eq!(reach[SkillId::Foraging.index()], 5);
        assert_eq!(reach[SkillId::Wayfinding.index()], WALK_UNTIL);
        assert_eq!(reachable_level(&book(), SkillId::Woodcraft), 6);
    }

    #[test]
    fn a_recipe_cannot_teach_a_level_that_only_it_can_unlock() {
        // Nothing but this recipe teaches Woodcraft past 4, and it needs level 5 itself.
        let text = "[recipe loop]\nname = Loop\ninput = timber 1\noutput = walking_staff 1\nrequire = woodcraft 5\nxp = woodcraft 10 9\n";
        let book = RecipeBook::parse(text).unwrap();
        assert_eq!(reachable_level(&book, SkillId::Woodcraft), 4, "the locked recipe never opens");
    }

    #[test]
    fn a_chain_of_recipes_opens_in_order() {
        let text = "\
[recipe one]\nname = One\ninput = timber 1\noutput = walking_staff 1\nrequire = woodcraft 3\nxp = woodcraft 10 6\n\n\
[recipe two]\nname = Two\ninput = timber 1\noutput = copper_charm 1\nrequire = woodcraft 6\nxp = woodcraft 10 8\n";
        let book = RecipeBook::parse(text).unwrap();
        assert_eq!(reachable_level(&book, SkillId::Woodcraft), 8);
    }

    #[test]
    fn requirements_across_skills_are_followed() {
        let text = "[recipe mixed]\nname = Mixed\ninput = timber 1\noutput = walking_staff 1\nrequire = herbalism 4\nrequire = woodcraft 4\nxp = foraging 10 9\n";
        let book = RecipeBook::parse(text).unwrap();
        assert_eq!(reachable_level(&book, SkillId::Foraging), 9);
        let text = text.replace("herbalism 4", "herbalism 6");
        let book = RecipeBook::parse(&text).unwrap();
        assert_eq!(reachable_level(&book, SkillId::Foraging), 5, "herbalism 6 is out of reach, so the recipe stays shut");
    }

    // ---- gate checks ----

    fn dialogue(text: &str) -> DialogueBook {
        DialogueBook::parse(text).unwrap()
    }

    #[test]
    fn shipped_recipes_and_dialogue_pass_the_gate_check() {
        let dialogue = dialogue(crate::dialogue::DEFAULT_DIALOGUE);
        check_gates(&book(), &dialogue).unwrap();
        // ...and they do contain gates, so the check is not vacuous.
        assert!(book().all().iter().any(|r| !r.requires.is_empty()));
        assert!(dialogue.skill_requirements().len() >= 2);
    }

    #[test]
    fn an_unreachable_recipe_gate_is_refused_with_the_recipe_and_line() {
        let text = "# c\n[recipe grand]\nname = Grand\ninput = timber 1\noutput = walking_staff 1\nrequire = woodcraft 9\n";
        let book = RecipeBook::parse(text).unwrap();
        let error = check_gates(&book, &dialogue(crate::dialogue::DEFAULT_DIALOGUE)).unwrap_err();
        assert!(error.contains("recipe `grand` (line 2)"), "{error}");
        assert!(error.contains("Woodcraft level 9"), "{error}");
        assert!(error.contains("past level 4"), "{error}");
    }

    #[test]
    fn an_unreachable_dialogue_gate_is_refused_with_its_line() {
        let text = DEFAULT_DIALOGUE_WITH_HIGH_GATE;
        let error = check_gates(&book(), &dialogue(text)).unwrap_err();
        assert!(error.starts_with("dialogue line "), "{error}");
        assert!(error.contains("Wayfinding level 9"), "{error}");
        assert!(error.contains("past level 3"), "{error}");
    }

    const DEFAULT_DIALOGUE_WITH_HIGH_GATE: &str = "\
[conversation guide]\nnpc = guide\nspeaker = G\nentry = hi\n\n\
[node hi]\ntext = Hello.\nchoice = Secret. -> end if skill wayfinding at_least 9\nchoice = Bye. -> end\n";

    #[test]
    fn below_conditions_are_never_requirements() {
        let text = "[conversation g]\nnpc = g\nspeaker = G\nentry = hi\n\n[node hi]\ntext = Hi.\nchoice = Newbie. -> end if skill wayfinding below 9\nchoice = Bye. -> end\n";
        let dialogue = dialogue(text);
        assert!(dialogue.skill_requirements().is_empty());
        check_gates(&book(), &dialogue).unwrap();
    }

    #[test]
    fn no_gates_at_all_is_fine() {
        let book = RecipeBook::parse("[recipe a]\nname = A\ninput = timber 1\noutput = walking_staff 1\n").unwrap();
        let dialogue = dialogue("[conversation g]\nnpc = g\nspeaker = G\nentry = hi\n\n[node hi]\ntext = Hi.\nchoice = Bye. -> end\n");
        assert_eq!(check_gates(&book, &dialogue), Ok(()));
    }
}
