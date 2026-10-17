//! Recipes and crafting.
//!
//! Pure Rust (no Bevy). Recipes are validated content (`RecipeBook`, usually parsed from
//! `assets/recipes/*.recipes`); crafting is a transaction on an [`Inventory`].
//!
//! Rules, in one place:
//! - A recipe has a unique id, a name, at least one input, and exactly one output. Inputs
//!   name distinct items, and the output is never also an input.
//! - `craft` needs every input in the bag. It then takes the inputs and adds the output on a
//!   copy of the bag, and only commits if the output fits. Consuming inputs can free the
//!   slot the output needs, so a bag with no spare slot may still craft.
//! - A failed craft changes nothing. Crafting does not count as gathering for quests.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;

use crate::inventory::{Inventory, ItemId};
use crate::quests::ItemAmount;
use crate::textdata::{check_id, parse_positive, read_lines, ContentError, Item};

/// The recipe file compiled into the game, identical to `assets/recipes/meadow.recipes`.
pub const DEFAULT_RECIPES: &str = include_str!("../assets/recipes/meadow.recipes");

/// Where the game looks for its recipe file, relative to the working directory.
pub const RECIPES_PATH: &str = "assets/recipes/meadow.recipes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub inputs: Vec<ItemAmount>,
    pub output: ItemAmount,
    /// Line of the `[recipe ...]` header when parsed from a file; 0 when built in code.
    pub line: usize,
}

/// An ingredient the bag does not have enough of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortfall {
    pub item: ItemId,
    pub needed: u32,
    pub have: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CraftError {
    UnknownRecipe(String),
    /// Lists every ingredient that is short, in recipe order.
    Missing(Vec<Shortfall>),
    /// The ingredients are there but the result would not fit.
    NoRoom,
}

impl CraftError {
    /// A short plain-language message for the player.
    pub fn message(&self) -> String {
        match self {
            Self::UnknownRecipe(_) => "You do not know how to make that.".to_string(),
            Self::Missing(short) => {
                let parts: Vec<String> = short
                    .iter()
                    .map(|s| format!("{} more {}", s.needed - s.have, s.item.label()))
                    .collect();
                format!("You still need {}.", parts.join(" and "))
            }
            Self::NoRoom => "Your inventory is full. Make room before crafting.".to_string(),
        }
    }
}

/// Whether a recipe can be made right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeStatus {
    Ready,
    MissingItems,
    NoRoom,
}

impl Recipe {
    /// Ingredients the bag is short of (empty when every input is on hand).
    pub fn shortfalls(&self, bag: &Inventory) -> Vec<Shortfall> {
        self.inputs
            .iter()
            .filter_map(|input| {
                let have = bag.count(input.item);
                (have < input.quantity).then_some(Shortfall { item: input.item, needed: input.quantity, have })
            })
            .collect()
    }

    /// The bag after crafting once, or why it cannot be done. Does not change `bag`.
    fn try_craft(&self, bag: &Inventory) -> Result<Inventory, CraftError> {
        let short = self.shortfalls(bag);
        if !short.is_empty() {
            return Err(CraftError::Missing(short));
        }
        let mut trial = bag.clone();
        for input in &self.inputs {
            trial.remove(input.item, input.quantity).map_err(|_| CraftError::Missing(self.shortfalls(bag)))?;
        }
        trial.add(self.output.item, self.output.quantity).map_err(|_| CraftError::NoRoom)?;
        Ok(trial)
    }

    pub fn status(&self, bag: &Inventory) -> RecipeStatus {
        match self.try_craft(bag) {
            Ok(_) => RecipeStatus::Ready,
            Err(CraftError::NoRoom) => RecipeStatus::NoRoom,
            Err(_) => RecipeStatus::MissingItems,
        }
    }

    /// "3 timber", "2 copper ore + 1 timber".
    pub fn describe_inputs(&self) -> String {
        self.inputs
            .iter()
            .map(|i| format!("{} {}", i.quantity, i.item.label()))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// Validated recipes, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeBook {
    recipes: Vec<Recipe>,
}

struct Draft {
    id: String,
    line: usize,
    name: Option<String>,
    inputs: Vec<ItemAmount>,
    output: Option<ItemAmount>,
}

fn parse_amount(line: usize, field: &str, value: &str) -> Result<ItemAmount, ContentError> {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    if tokens.len() != 2 {
        return Err(ContentError::at(line, format!("`{field}` needs an item key and an amount, like `timber 3`")));
    }
    let item = ItemId::from_key(tokens[0])
        .ok_or_else(|| ContentError::at(line, format!("unknown item `{}`", tokens[0])))?;
    let quantity = parse_positive(line, "amount", tokens[1])?;
    Ok(ItemAmount { item, quantity })
}

impl Draft {
    fn finish(self) -> Result<Recipe, ContentError> {
        let at = self.line;
        let name = self
            .name
            .ok_or_else(|| ContentError::at(at, format!("recipe `{}` has no `name`", self.id)))?;
        if self.inputs.is_empty() {
            return Err(ContentError::at(at, format!("recipe `{}` has no `input`", self.id)));
        }
        let output = self
            .output
            .ok_or_else(|| ContentError::at(at, format!("recipe `{}` has no `output`", self.id)))?;
        if self.inputs.iter().any(|i| i.item == output.item) {
            return Err(ContentError::at(
                at,
                format!("recipe `{}` makes {} from itself", self.id, output.item.label()),
            ));
        }
        Ok(Recipe { id: self.id, name, inputs: self.inputs, output, line: at })
    }
}

impl RecipeBook {
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let mut recipes: Vec<Recipe> = Vec::new();
        let mut ids = BTreeSet::new();
        let mut current: Option<Draft> = None;

        for line in read_lines(text)? {
            let number = line.number;
            match line.item {
                Item::Section { kind, arg } => {
                    if let Some(done) = current.take() {
                        recipes.push(done.finish()?);
                    }
                    if kind != "recipe" {
                        return Err(ContentError::at(number, format!("unknown section `[{kind}]`; expected `[recipe id]`")));
                    }
                    check_id(number, "recipe id", &arg)?;
                    if !ids.insert(arg.clone()) {
                        return Err(ContentError::at(number, format!("recipe `{arg}` is defined twice")));
                    }
                    current = Some(Draft { id: arg, line: number, name: None, inputs: Vec::new(), output: None });
                }
                Item::Field { key, value } => {
                    let Some(draft) = current.as_mut() else {
                        return Err(ContentError::at(number, format!("`{key}` is outside any `[recipe id]` section")));
                    };
                    match key.as_str() {
                        "name" => {
                            if value.is_empty() {
                                return Err(ContentError::at(number, "`name` is empty"));
                            }
                            if draft.name.replace(value).is_some() {
                                return Err(ContentError::at(number, "`name` is given twice"));
                            }
                        }
                        "input" => {
                            let amount = parse_amount(number, "input", &value)?;
                            if draft.inputs.iter().any(|i| i.item == amount.item) {
                                return Err(ContentError::at(
                                    number,
                                    format!("`{}` is listed twice as an input; give one line with the total", amount.item.key()),
                                ));
                            }
                            draft.inputs.push(amount);
                        }
                        "output" => {
                            let amount = parse_amount(number, "output", &value)?;
                            if draft.output.replace(amount).is_some() {
                                return Err(ContentError::at(number, "a recipe has exactly one `output`"));
                            }
                        }
                        other => {
                            return Err(ContentError::at(
                                number,
                                format!("unknown field `{other}` (use name, input, output)"),
                            ));
                        }
                    }
                }
            }
        }
        if let Some(done) = current.take() {
            recipes.push(done.finish()?);
        }
        if recipes.is_empty() {
            return Err(ContentError::at(0, "the recipe file defines no recipes"));
        }
        Ok(Self { recipes })
    }

    /// The copy compiled into the game.
    pub fn builtin() -> Result<Self, ContentError> {
        Self::parse(DEFAULT_RECIPES)
    }

    pub fn all(&self) -> &[Recipe] {
        &self.recipes
    }

    pub fn get(&self, id: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }

    /// Makes one batch of recipe `id`. All or nothing: on any error `bag` is unchanged.
    /// Returns what was made.
    pub fn craft(&self, id: &str, bag: &mut Inventory) -> Result<ItemAmount, CraftError> {
        let recipe = self.get(id).ok_or_else(|| CraftError::UnknownRecipe(id.to_string()))?;
        let after = recipe.try_craft(bag)?;
        *bag = after;
        Ok(recipe.output)
    }
}

/// Recipes plus a plain-language note when the built-in copy had to be used.
#[derive(Debug, Clone)]
pub struct LoadedRecipes {
    pub book: RecipeBook,
    pub note: Option<String>,
}

/// Loads recipes with the same rules as the level and quest files: a custom path must load;
/// the default path is used when it exists; when it is simply absent the identical built-in
/// copy is used (with a note) so the game starts from any directory. A file that exists but
/// is invalid is never replaced by the built-in copy: the error names the file and line.
pub fn load(custom: Option<&Path>, default: &Path) -> Result<LoadedRecipes, String> {
    let (text, label, note) = if let Some(path) = custom {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read recipe file {}: {e}", path.display()))?;
        (text, path.display().to_string(), None)
    } else {
        match std::fs::read_to_string(default) {
            Ok(text) => (text, default.display().to_string(), None),
            Err(e) if e.kind() == ErrorKind::NotFound => (
                DEFAULT_RECIPES.to_string(),
                "built-in recipes".to_string(),
                Some(format!("note: {} not found; using the built-in recipes", default.display())),
            ),
            Err(e) => return Err(format!("could not read recipe file {}: {e}", default.display())),
        }
    };
    let book = RecipeBook::parse(&text).map_err(|e| {
        if label.starts_with("built-in") {
            format!("{label} are invalid: {e}")
        } else {
            format!("invalid recipe file {label}: {e}")
        }
    })?;
    Ok(LoadedRecipes { book, note })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> RecipeBook {
        RecipeBook::builtin().expect("shipped recipes parse")
    }

    fn bag(items: &[(ItemId, u32)], capacity: usize) -> Inventory {
        let mut bag = Inventory::new(capacity);
        for (item, n) in items {
            bag.add(*item, *n).unwrap();
        }
        bag
    }

    fn err(text: &str) -> ContentError {
        RecipeBook::parse(text).unwrap_err()
    }

    #[test]
    fn shipped_file_matches_the_built_in_copy_and_parses() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/recipes/meadow.recipes");
        assert_eq!(std::fs::read_to_string(path).unwrap(), DEFAULT_RECIPES);
        let book = book();
        let ids: Vec<&str> = book.all().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["walking_staff", "copper_charm", "mint_tea"]);
    }

    #[test]
    fn shipped_recipes_are_internally_sensible() {
        let book = book();
        for recipe in book.all() {
            assert!(!recipe.name.is_empty());
            assert!(!recipe.inputs.is_empty());
            assert!(recipe.line > 0);
            // Every recipe can be made from an empty-handed start plus its own inputs.
            let mut bag = Inventory::new(20);
            for input in &recipe.inputs {
                bag.add(input.item, input.quantity).unwrap();
            }
            assert_eq!(recipe.status(&bag), RecipeStatus::Ready, "{}", recipe.id);
        }
    }

    #[test]
    fn parses_every_field_with_comments_and_crlf() {
        let text = "# hi\r\n[Recipe tea]\r\nName = Tea = good\r\ninput = meadowmint 2\r\ninput = honeyberry 1\r\noutput = mint_tea 3\r\n";
        let book = RecipeBook::parse(text).unwrap();
        let r = book.get("tea").unwrap();
        assert_eq!(r.name, "Tea = good");
        assert_eq!(r.inputs.len(), 2);
        assert_eq!(r.output, ItemAmount { item: ItemId::MintTea, quantity: 3 });
        assert_eq!(r.line, 2);
        assert_eq!(r.describe_inputs(), "2 meadowmint + 1 honeyberry");
    }

    #[test]
    fn syntax_and_content_errors_carry_line_numbers() {
        let ok = "[recipe a]\nname = A\ninput = timber 1\noutput = walking_staff 1\n";
        assert!(RecipeBook::parse(ok).is_ok());

        let e = err("[recipe a]\nname = A\ninput = timbre 1\noutput = walking_staff 1\n");
        assert_eq!((e.line, e.message.as_str()), (Some(3), "unknown item `timbre`"));
        let e = err("[recipe a]\nname = A\ninput = timber\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("item key and an amount"), "{e}");
        let e = err("[recipe a]\nname = A\ninput = timber 0\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("above zero"), "{e}");
        let e = err("[recipe a]\nname = A\ninput = timber x\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(3));
        let e = err("[recipe a]\nname = A\ninput = timber 1 2\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(3));
        let e = err("[recipe a]\nname = A\ninput = timber 1\noutput = walking_staff 1\ncolour = red\n");
        assert_eq!(e.line, Some(5));
        assert!(e.message.contains("unknown field"), "{e}");
        let e = err("name = A\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("outside"), "{e}");
        let e = err("[potion a]\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("unknown section"), "{e}");
        let e = err("[recipe Bad Id]\n");
        assert!(e.message.contains("recipe id"), "{e}");
        let e = err("[recipe]\n");
        assert!(e.message.contains("recipe id"), "{e}");
    }

    #[test]
    fn structural_errors_name_the_recipe_header_line() {
        let e = err("\n[recipe a]\ninput = timber 1\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(2));
        assert!(e.message.contains("no `name`"), "{e}");
        let e = err("[recipe a]\nname = A\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("no `input`"), "{e}");
        let e = err("[recipe a]\nname = A\ninput = timber 1\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("no `output`"), "{e}");
        let e = err("[recipe a]\nname = A\ninput = timber 1\noutput = timber 2\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("from itself"), "{e}");
        let e = err("[recipe a]\nname = A\nname = B\n");
        assert_eq!(e.line, Some(3));
        let e = err("[recipe a]\nname =\n");
        assert_eq!(e.line, Some(2));
        let e = err("[recipe a]\nname = A\ninput = timber 1\ninput = timber 2\noutput = walking_staff 1\n");
        assert_eq!(e.line, Some(4));
        assert!(e.message.contains("listed twice"), "{e}");
        let e = err("[recipe a]\nname = A\ninput = timber 1\noutput = walking_staff 1\noutput = copper_charm 1\n");
        assert_eq!(e.line, Some(5));
    }

    #[test]
    fn duplicate_ids_and_empty_files_are_rejected() {
        let one = "[recipe a]\nname = A\ninput = timber 1\noutput = walking_staff 1\n";
        let e = err(&format!("{one}{one}"));
        assert_eq!(e.line, Some(5));
        assert!(e.message.contains("defined twice"), "{e}");
        let e = err("# nothing here\n");
        assert_eq!(e.line, None);
        assert!(e.message.contains("no recipes"), "{e}");
        assert!(RecipeBook::parse("").is_err());
    }

    #[test]
    fn crafting_takes_inputs_and_adds_the_output() {
        let mut b = bag(&[(ItemId::Timber, 5)], 4);
        let made = book().craft("walking_staff", &mut b).unwrap();
        assert_eq!(made, ItemAmount { item: ItemId::WalkingStaff, quantity: 1 });
        assert_eq!(b.count(ItemId::Timber), 2);
        assert_eq!(b.count(ItemId::WalkingStaff), 1);
    }

    #[test]
    fn crafting_with_several_inputs_uses_exact_amounts() {
        let mut b = bag(&[(ItemId::CopperOre, 3), (ItemId::Timber, 1)], 4);
        book().craft("copper_charm", &mut b).unwrap();
        assert_eq!(b.count(ItemId::CopperOre), 1);
        assert_eq!(b.count(ItemId::Timber), 0);
        assert_eq!(b.count(ItemId::CopperCharm), 1);
    }

    #[test]
    fn missing_ingredients_are_all_listed_and_nothing_changes() {
        let mut b = bag(&[(ItemId::CopperOre, 1)], 4);
        let before = b.clone();
        let error = book().craft("copper_charm", &mut b).unwrap_err();
        assert_eq!(
            error,
            CraftError::Missing(vec![
                Shortfall { item: ItemId::CopperOre, needed: 2, have: 1 },
                Shortfall { item: ItemId::Timber, needed: 1, have: 0 },
            ])
        );
        assert_eq!(error.message(), "You still need 1 more copper ore and 1 more timber.");
        assert_eq!(b, before);
    }

    #[test]
    fn a_full_bag_refuses_the_output_and_keeps_the_inputs() {
        // Two slots: one for timber (3) and one for moonpetals. Crafting a staff takes all the
        // timber, which frees a slot, so this succeeds...
        let mut b = bag(&[(ItemId::Timber, 3), (ItemId::Moonpetal, 1)], 2);
        assert!(book().craft("walking_staff", &mut b).is_ok());
        // ...but with timber left over after the craft no slot is freed and the staff has
        // nowhere to go.
        let mut b = bag(&[(ItemId::Timber, 4), (ItemId::Moonpetal, 1)], 2);
        let before = b.clone();
        assert_eq!(book().craft("walking_staff", &mut b), Err(CraftError::NoRoom));
        assert_eq!(b, before, "inputs are not consumed when the output cannot be stored");
        assert_eq!(CraftError::NoRoom.message(), "Your inventory is full. Make room before crafting.");
    }

    #[test]
    fn status_reports_ready_missing_and_no_room() {
        let book = book();
        let staff = book.get("walking_staff").unwrap();
        assert_eq!(staff.status(&Inventory::new(4)), RecipeStatus::MissingItems);
        assert_eq!(staff.status(&bag(&[(ItemId::Timber, 3)], 4)), RecipeStatus::Ready);
        assert_eq!(staff.status(&bag(&[(ItemId::Timber, 4), (ItemId::Moonpetal, 1)], 2)), RecipeStatus::NoRoom);
    }

    #[test]
    fn status_and_craft_agree_and_status_never_mutates() {
        let book = book();
        let cases = [
            bag(&[], 3),
            bag(&[(ItemId::Timber, 3)], 1),
            bag(&[(ItemId::Timber, 4)], 1),
            bag(&[(ItemId::Meadowmint, 2), (ItemId::Honeyberry, 1)], 2),
            bag(&[(ItemId::Meadowmint, 2), (ItemId::Honeyberry, 2)], 2),
            bag(&[], 0),
            bag(&[(ItemId::CopperOre, 2), (ItemId::Timber, 1)], 2),
        ];
        for recipe in book.all() {
            for case in &cases {
                let before = case.clone();
                let status = recipe.status(case);
                assert_eq!(*case, before);
                let mut copy = case.clone();
                let result = book.craft(&recipe.id, &mut copy);
                assert_eq!(status == RecipeStatus::Ready, result.is_ok(), "{} with {:?}", recipe.id, case);
                if result.is_err() {
                    assert_eq!(copy, before);
                }
            }
        }
    }

    #[test]
    fn unknown_recipes_are_refused() {
        let mut b = Inventory::new(2);
        assert_eq!(book().craft("nothing", &mut b), Err(CraftError::UnknownRecipe("nothing".into())));
        assert!(CraftError::UnknownRecipe("x".into()).message().contains("do not know"));
    }

    #[test]
    fn crafting_is_repeatable_until_ingredients_run_out() {
        let mut b = bag(&[(ItemId::Timber, 7)], 3);
        let book = book();
        assert!(book.craft("walking_staff", &mut b).is_ok());
        assert!(book.craft("walking_staff", &mut b).is_ok());
        assert_eq!(book.craft("walking_staff", &mut b).map_err(|e| matches!(e, CraftError::Missing(_))), Err(true));
        assert_eq!(b.count(ItemId::WalkingStaff), 2);
        assert_eq!(b.count(ItemId::Timber), 1);
    }

    #[test]
    fn multi_item_outputs_use_the_right_number_of_slots() {
        let text = "[recipe bulk]\nname = Bulk\ninput = moonpetal 1\noutput = timber 150\n";
        let book = RecipeBook::parse(text).unwrap();
        // 150 timber needs two slots; the moonpetal slot is freed by crafting.
        let mut tight = bag(&[(ItemId::Moonpetal, 1)], 2);
        assert!(book.craft("bulk", &mut tight).is_ok());
        assert_eq!(tight.count(ItemId::Timber), 150);
        let mut tighter = bag(&[(ItemId::Moonpetal, 1)], 1);
        assert_eq!(book.craft("bulk", &mut tighter), Err(CraftError::NoRoom));
        assert_eq!(tighter.count(ItemId::Moonpetal), 1);
    }

    // ---- loading ----

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("ozbound-recipes-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn shipped_default_path_loads_without_a_note() {
        // Tests run with the package root as the working directory.
        let loaded = load(None, Path::new(RECIPES_PATH)).unwrap();
        assert!(loaded.note.is_none());
        assert_eq!(loaded.book, RecipeBook::builtin().unwrap());
    }

    #[test]
    fn an_absent_default_file_falls_back_to_the_built_in_recipes_with_a_note() {
        let dir = scratch();
        let loaded = load(None, &dir.join("none.recipes")).unwrap();
        assert!(loaded.note.as_deref().unwrap().contains("built-in recipes"));
        assert_eq!(loaded.book, RecipeBook::builtin().unwrap());
    }

    #[test]
    fn an_existing_default_file_is_used_over_the_built_in_copy() {
        let dir = scratch();
        let path = dir.join("mine.recipes");
        std::fs::write(&path, "[recipe only]\nname = Only\ninput = timber 1\noutput = walking_staff 1\n").unwrap();
        let loaded = load(None, &path).unwrap();
        assert!(loaded.note.is_none());
        assert_eq!(loaded.book.all().len(), 1);
    }

    #[test]
    fn an_invalid_default_file_is_not_replaced_by_the_built_in_copy() {
        let dir = scratch();
        let path = dir.join("broken.recipes");
        std::fs::write(&path, "[recipe a]\nname = A\ninput = timbre 1\n").unwrap();
        let error = load(None, &path).unwrap_err();
        assert!(error.starts_with("invalid recipe file"), "{error}");
        assert!(error.contains("broken.recipes") && error.contains("line 3"), "{error}");
    }

    #[test]
    fn a_custom_path_must_exist_and_names_itself_in_errors() {
        let dir = scratch();
        let missing = dir.join("custom.recipes");
        let error = load(Some(&missing), Path::new(RECIPES_PATH)).unwrap_err();
        assert!(error.starts_with("could not read recipe file") && error.contains("custom.recipes"), "{error}");
        std::fs::write(&missing, "nonsense\n").unwrap();
        let error = load(Some(&missing), Path::new(RECIPES_PATH)).unwrap_err();
        assert!(error.starts_with("invalid recipe file") && error.contains("line 1"), "{error}");
    }
}
