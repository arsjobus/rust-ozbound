# Locations

## Purpose

This document is the canonical planning inventory for the world
locations in **Ozbound**, a single-player, local-first RPG built with
Rust and Bevy. It defines what locations exist, how they relate
geographically, what the player does there, and how each area should be
implemented and tested.

This is a planning document, not a promise that every location ships at
once. Keep the first playable world small, coherent, and testable, then
expand it in deliberate stages.

## Worldbuilding rules

1.  **Separate source material from game invention.** Mark each location
    as `1900 novel`, `later Oz books`, or `original game content`. The
    1900 novel is the primary reference for the first pass.
2.  **Do not silently rewrite Baum's geography.** Where the novel does
    not provide a precise map, record the relationship as approximate
    and treat the game's exact layout as an adaptation.
3.  **Build connected places, not disconnected levels.** Each location
    should have a clear entrance, exit, landmark, and reason to visit.
4.  **Keep exploration intentional.** No endless randomized feeds,
    opaque content systems, or mandatory online services. Discoveries
    should come from the world, characters, clues, and player choices.
5.  **Use data-driven definitions where practical.** Location identity,
    exits, interactables, discovery flags, and map references should be
    separable from rendering logic.
6.  **Make every new feature testable.** Each location implementation
    task must include unit tests as an explicit AI development goal,
    plus integration/regression coverage where relevant. Prefer
    test-first or tests-alongside implementation.
7.  **Keep the game original.** Use the public-domain 1900 novel as the
    foundation, while writing original dialogue, quests, map layouts,
    puzzles, and systems. Track later-book additions separately.

## Canonical source labels

-   **`NOVEL_1900`** --- explicitly present in *The Wonderful Wizard of
    Oz* (1900).
-   **`LATER_OZ`** --- introduced, named, or substantially developed in
    later Oz books.
-   **`GAME_ORIGINAL`** --- created for Ozbound; should fit the tone and
    geography without being presented as Baum canon.
-   **`ADAPTATION`** --- a game interpretation of a source location
    where the novel does not define a precise layout.

## World overview

The novel describes Oz as a country divided into regions associated with
the Munchkins, Winkies, and Quadlings, with the Emerald City near the
center. The Good Witch of the North appears in the original novel, but
**Gillikin Country is a later-book name** and should not be treated as a
named country in the 1900 text.

For game planning, use this high-level orientation while allowing the
playable route to differ where needed:

``` text
                          NORTH
                 Good Witch of the North
                    (region unnamed
                    in the 1900 novel)

             [Northern route / future content]

WEST                                                   EAST
Country of the Winkies        Emerald City       Land of the Munchkins
Wicked Witch of the West      (central hub)      Dorothy's arrival
             \                    |                    /
              \                   |                   /
               \          Yellow Brick Road          /
                \                 |                 /
                  Country of the Quadlings
                    Glinda, Good Witch of the South
                          SOUTH
```

This is a **design orientation, not a surveyed map**. The book's route
includes landmarks and detours that should be represented as connected
spaces within the broader regions.

## Location register

Status key: - `P0` --- required for the first playable
journey/prototype. - `P1` --- core story area, implement after the
prototype route works. - `P2` --- optional/expanded exploration. - `P3`
--- later-book or post-launch expansion.

  --------------------------------------------------------------------------------------------------
  ID                         Location         Source            Priority       Role
  -------------------------- ---------------- ----------------- -------------- ---------------------
  `KANSAS_FARM`              Dorothy's home   `NOVEL_1900`      P0             Opening and ending;
                             and Kansas                                        tutorial and
                             prairie                                           return-home context

  `MUNCHKIN_LANDS`           Land of the      `NOVEL_1900`      P0             Arrival region; first
                             Munchkins                                         friendly settlement
                                                                               and route onward

  `YELLOW_BRICK_ROAD_EAST`   Yellow Brick     `NOVEL_1900`      P0             Main navigation
                             Road, eastern                                     spine; travel
                             stretch                                           encounters

  `WOODS_FIRST`              Forest where the `NOVEL_1900` /    P0             First companion
                             Scarecrow is     `ADAPTATION`                     encounter
                             found                                             

  `TIN_WOODSMAN_CLEARING`    Clearing/route   `NOVEL_1900` /    P0             Companion encounter
                             where the Tin    `ADAPTATION`                     and tool/interaction
                             Woodman is found                                  tutorial

  `LION_FOREST`              Forest where the `NOVEL_1900` /    P0             Companion encounter
                             Cowardly Lion    `ADAPTATION`                     and courage-themed
                             joins                                             challenge

  `KALIDAH_FOREST`           Kalidah          `NOVEL_1900`      P1             Hazard encounter and
                             encounter area                                    traversal challenge

  `BROKEN_BRIDGE`            Broken bridge    `NOVEL_1900` /    P1             Teamwork and
                             across a ravine  `ADAPTATION`                     route-solving
                                                                               obstacle

  `WIDE_RIVER`               River crossing   `NOVEL_1900` /    P1             Environmental
                                              `ADAPTATION`                     traversal and
                                                                               companion utility

  `POPPY_FIELD`              Field of deadly  `NOVEL_1900`      P1             Sleep hazard and
                             poppies                                           rescue sequence

  `EMERALD_CITY_GATE`        Emerald City     `NOVEL_1900`      P0             Arrival checkpoint;
                             entrance and                                      city access condition
                             gate                                              

  `EMERALD_CITY`             Emerald City     `NOVEL_1900`      P0             Central hub,
                             streets and                                       services, NPCs, and
                             public spaces                                     Wizard storyline

  `WIZARD_PALACE`            Wizard's palace  `NOVEL_1900`      P0             Main story objective
                             and audience                                      and reveal
                             chambers                                          

  `WESTERN_ROAD`             Route into the   `NOVEL_1900` /    P1             Second major
                             Winkie Country   `ADAPTATION`                     expedition

  `WINKIE_COUNTRY`           Country of the   `NOVEL_1900`      P1             Western region and
                             Winkies                                           Witch storyline

  `WITCH_FORTRESS`           Wicked Witch of  `NOVEL_1900` /    P1             Infiltration, rescue,
                             the West's       `ADAPTATION`                     and climax
                             stronghold                                        

  `SOUTHERN_ROUTE`           Route toward the `NOVEL_1900` /    P1             Final journey and
                             Quadlings        `ADAPTATION`                     exploration

  `HAMMER_HEADS`             Hammer-Heads'    `NOVEL_1900`      P2             Hostile
                             territory                                         boundary/encounter;
                                                                               route obstacle

  `CHINA_COUNTRY`            Dainty China     `NOVEL_1900`      P2             Optional miniature
                             Country                                           region with
                                                                               fragile-world rules

  `QUADLING_COUNTRY`         Country of the   `NOVEL_1900`      P1             Southern region and
                             Quadlings                                         approach to Glinda

  `GLINDA_PALACE`            Glinda's         `NOVEL_1900` /    P1             Endgame guidance and
                             home/palace      `ADAPTATION`                     return-home
                                                                               resolution

  `NORTH_REGION`             Area around the  `NOVEL_1900` /    P2             Optional northern
                             Good Witch of    `ADAPTATION`                     story content; do not
                             the North                                         label it Gillikin
                                                                               Country in the
                                                                               1900-canon layer

  `GILLIKIN_COUNTRY`         Named northern   `LATER_OZ`        P3             Expansion only;
                             country                                           clearly marked as
                                                                               later-book material

  `OZ_VILLAGE_TEMPLATE`      Additional small `GAME_ORIGINAL`   P2             Reusable settlement
                             Oz village                                        template for side
                                                                               quests and services
  --------------------------------------------------------------------------------------------------

## Location specifications

### `KANSAS_FARM` --- Dorothy's home and Kansas prairie

-   **Source:** `NOVEL_1900`
-   **Priority:** `P0`
-   **Purpose:** Establish Dorothy's ordinary life, introduce basic
    movement and interaction, and frame the goal of returning home.
-   **Required features:** farmhouse interior or yard, prairie exterior,
    cyclone transition, clear starting point, readable route back to the
    farmhouse.
-   **Interactions:** inspect household objects; speak to key characters
    if included in the adaptation; trigger the cyclone/story transition.
-   **Exits:** `MUNCHKIN_LANDS` through the cyclone transition; return
    here at the story ending.
-   **Art direction:** restrained palette and open prairie, contrasting
    with Oz's stronger colors.
-   **Acceptance criteria:**
    -   The player can move, inspect designated objects, and trigger the
        transition.
    -   The transition cannot fire repeatedly or leave the player in an
        invalid world state.
    -   Returning to Kansas preserves the intended ending state.
-   **Unit-test goals:**
    -   Test the transition trigger's preconditions and one-time
        behavior.
    -   Test that the correct destination and story flag are set.
    -   Test save/load of the relevant progression state once
        persistence exists.

### `MUNCHKIN_LANDS` --- Land of the Munchkins

-   **Source:** `NOVEL_1900`
-   **Priority:** `P0`
-   **Purpose:** First Oz region; establish that the player has arrived
    somewhere strange but inhabited.
-   **Required features:** landing site, a small navigable area,
    Munchkin NPCs, a clear path toward the Yellow Brick Road, and an
    initial landmark.
-   **Interactions:** speak with residents; inspect the landing site;
    receive a route clue; find the Yellow Brick Road.
-   **Exits:** `YELLOW_BRICK_ROAD_EAST`; `KANSAS_FARM` only through a
    story transition if explicitly intended.
-   **Art direction:** blue accents are a useful regional cue, but avoid
    treating a later adaptation's visual details as mandatory canon.
-   **Acceptance criteria:**
    -   The player can orient themselves from the landing site and
        identify the main route.
    -   At least one NPC provides useful, non-repeating information.
    -   The region can be entered and exited without losing companion or
        quest state.
-   **Unit-test goals:**
    -   Test exit connectivity and destination IDs.
    -   Test NPC dialogue progression and its completed-state behavior.
    -   Test that the initial arrival event is not duplicated after
        re-entry.

### `YELLOW_BRICK_ROAD_EAST` --- Yellow Brick Road

-   **Source:** `NOVEL_1900`
-   **Priority:** `P0`
-   **Purpose:** Connect early journey locations and provide a
    recognizable navigation spine.
-   **Required features:** visible road tiles or path markers,
    junctions, readable landmarks, and controlled encounter points.
-   **Interactions:** inspect signs or clues; meet companions at
    connected sublocations; discover safe stopping points.
-   **Exits:** `MUNCHKIN_LANDS`, `WOODS_FIRST`, `TIN_WOODSMAN_CLEARING`,
    `LION_FOREST`, and eventually `EMERALD_CITY_GATE`, arranged as a
    coherent route rather than all direct doors.
-   **Art direction:** repeated yellow path tiles, with enough
    environmental variation to keep the route legible.
-   **Acceptance criteria:**
    -   The road is visually distinguishable from surrounding terrain.
    -   Every junction has a readable route choice or landmark.
    -   Encounters are authored and deterministic unless a specific
        system requires otherwise.
-   **Unit-test goals:**
    -   Test route graph connectivity and that all declared exits
        resolve to known location IDs.
    -   Test that blocked routes remain blocked until their requirements
        are satisfied.
    -   Test that authored encounters do not repeat when marked as
        one-time.

### `WOODS_FIRST` --- First forest and Scarecrow encounter

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P0`
-   **Purpose:** Introduce the Scarecrow and the companion system.
-   **Required features:** forest edge, distinctive pole/tree landmark,
    companion interaction, and a route back to the road.
-   **Interactions:** talk to the Scarecrow; invite him to join; inspect
    nearby environmental clues.
-   **Exits:** `YELLOW_BRICK_ROAD_EAST`.
-   **Acceptance criteria:**
    -   The Scarecrow joins only once and is represented correctly in
        party state.
    -   Dialogue reflects whether the companion has joined.
    -   Re-entering the area does not duplicate the companion.
-   **Unit-test goals:**
    -   Test party recruitment and duplicate prevention.
    -   Test the relevant quest/progression flag.
    -   Test that companion state survives a location transition.

### `TIN_WOODSMAN_CLEARING` --- Tin Woodman encounter

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P0`
-   **Purpose:** Introduce the Tin Woodman and demonstrate world
    interactions or item use.
-   **Required features:** a clear encounter landmark, an interaction
    sequence, and a route onward.
-   **Interactions:** speak to the Tin Woodman; resolve the encounter
    using the game's authored logic; recruit him.
-   **Exits:** `YELLOW_BRICK_ROAD_EAST`.
-   **Acceptance criteria:**
    -   The encounter has a clear start, resolution, and completed
        state.
    -   The Tin Woodman joins only once.
    -   Required items or actions, if any, are communicated clearly.
-   **Unit-test goals:**
    -   Test encounter prerequisites and completion.
    -   Test party recruitment and duplicate prevention.
    -   Test item consumption only when the action succeeds.

### `LION_FOREST` --- Cowardly Lion encounter

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P0`
-   **Purpose:** Introduce the Lion and a challenge built around fear,
    courage, or protecting companions.
-   **Required features:** readable encounter space, a safe return path,
    and companion dialogue.
-   **Interactions:** complete the authored encounter; recruit the Lion;
    optionally revisit companion-specific dialogue.
-   **Exits:** `YELLOW_BRICK_ROAD_EAST`.
-   **Acceptance criteria:**
    -   The encounter cannot trap the player in a softlock.
    -   Recruitment updates the party and story state once.
    -   Later dialogue acknowledges recruitment.
-   **Unit-test goals:**
    -   Test encounter state transitions.
    -   Test party recruitment and relevant dialogue conditions.
    -   Test that failure or cancellation leaves the player in a valid
        state.

### `KALIDAH_FOREST` --- Kalidah encounter area

-   **Source:** `NOVEL_1900`
-   **Priority:** `P1`
-   **Purpose:** Turn the forest crossing into a clear hazard or
    tactical obstacle.
-   **Required features:** recognizable threat, telegraphed danger, and
    a traversable route after the obstacle is resolved.
-   **Interactions:** avoid, distract, or overcome the threat using
    authored game mechanics.
-   **Exits:** road/forest route in and out.
-   **Acceptance criteria:**
    -   The player can understand the threat before an unavoidable
        failure.
    -   The encounter has a defined resolution and cannot block progress
        permanently.
-   **Unit-test goals:**
    -   Test encounter preconditions and outcome branches.
    -   Test hazard collision and safe-state transitions.
    -   Test that resolved hazards stay resolved if intended.

### `BROKEN_BRIDGE` --- Broken bridge across a ravine

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P1`
-   **Purpose:** A traversal puzzle that rewards examining the
    environment and using the party's abilities.
-   **Required features:** visible gap, clear interaction point,
    feedback for invalid attempts, and a safe reset point.
-   **Interactions:** inspect the bridge; choose a repair, crossing, or
    teamwork solution supported by the game design.
-   **Exits:** connected road segments on either side.
-   **Acceptance criteria:**
    -   The player understands why crossing is currently blocked.
    -   A valid solution unlocks the crossing without requiring
        frame-perfect input.
    -   The player cannot walk across an unrepaired gap unless a
        deliberate alternative is provided.
-   **Unit-test goals:**
    -   Test puzzle prerequisites and completion.
    -   Test collision/navigation changes after repair.
    -   Test that the puzzle state persists through leaving and
        returning.

### `WIDE_RIVER` --- River crossing

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P1`
-   **Purpose:** Environmental traversal and a reason for the party to
    cooperate.
-   **Required features:** clear banks, water boundary, crossing
    solution, and failure-safe positioning.
-   **Interactions:** use the authored crossing method; inspect the
    banks and nearby objects.
-   **Exits:** connected routes on both sides of the river.
-   **Acceptance criteria:**
    -   Water boundaries prevent accidental walking into invalid space.
    -   Crossing is possible after the intended requirements are met.
    -   Failed crossing attempts return the player to a safe position.
-   **Unit-test goals:**
    -   Test collision boundaries and crossing prerequisites.
    -   Test success/failure transitions.
    -   Test no invalid coordinates or stuck states after crossing.

### `POPPY_FIELD` --- Field of Deadly Poppies

-   **Source:** `NOVEL_1900`
-   **Priority:** `P1`
-   **Purpose:** A distinctive environmental hazard and rescue sequence.
-   **Required features:** visibly marked hazard area, escalating
    warning, state change for affected characters, and a rescue route.
-   **Interactions:** plan a safe crossing; rescue affected companions
    according to the chosen adaptation.
-   **Exits:** routes before and after the field.
-   **Acceptance criteria:**
    -   The danger is signposted before the player commits.
    -   The hazard never creates an unrecoverable party state.
    -   The rescue sequence can be completed and is not duplicated.
-   **Unit-test goals:**
    -   Test hazard application and immunity/exception rules if any.
    -   Test rescue prerequisites and completion.
    -   Test party recovery and quest-state consistency after reload.

### `EMERALD_CITY_GATE` --- Emerald City entrance

-   **Source:** `NOVEL_1900`
-   **Priority:** `P0`
-   **Purpose:** Establish the Emerald City as a major milestone and
    control access to the central hub.
-   **Required features:** recognizable gate, gatekeeper interaction,
    clear access requirements, and a visible city beyond.
-   **Interactions:** speak with the gatekeeper; complete any required
    story step; enter the city.
-   **Exits:** `YELLOW_BRICK_ROAD_EAST`, `EMERALD_CITY`.
-   **Acceptance criteria:**
    -   The gate explains any access condition.
    -   Once access is granted, re-entry works consistently.
    -   The gate cannot send the player to an invalid destination.
-   **Unit-test goals:**
    -   Test gate conditions before and after access is granted.
    -   Test exit resolution and destination loading.
    -   Test that the access flag persists.

### `EMERALD_CITY` --- Emerald City

-   **Source:** `NOVEL_1900`
-   **Priority:** `P0`
-   **Purpose:** Central hub for story progression, services,
    characters, and preparation for the western journey.
-   **Required features:** legible streets, a few memorable landmarks,
    NPCs with useful roles, and access to the Wizard's palace.
-   **Interactions:** talk to residents; explore; obtain the
    audience/story objective; revisit NPCs after major milestones.
-   **Exits:** `EMERALD_CITY_GATE`, `WIZARD_PALACE`, and later unlocked
    routes as appropriate.
-   **Art direction:** strong green identity, but avoid making every
    surface identical or sacrificing readability.
-   **Acceptance criteria:**
    -   The player can navigate between the gate, services, and palace
        without confusion.
    -   NPCs have distinct purposes and do not repeat identical generic
        dialogue unnecessarily.
    -   Story progress updates the relevant conversations.
-   **Unit-test goals:**
    -   Test city exits and interactable IDs.
    -   Test conditional dialogue after key quest milestones.
    -   Test service interactions and inventory changes, if services are
        implemented.

### `WIZARD_PALACE` --- Wizard's palace

-   **Source:** `NOVEL_1900`
-   **Priority:** `P0`
-   **Purpose:** Deliver the central audience with the Wizard and
    establish the next major objective.
-   **Required features:** palace entrance, audience chamber, a
    controlled sequence of interactions, and a clear exit back to the
    city.
-   **Interactions:** request an audience; receive the objective to
    confront the Wicked Witch of the West; return after the western
    story.
-   **Exits:** `EMERALD_CITY`.
-   **Acceptance criteria:**
    -   The audience sequence occurs in the correct story order.
    -   The player receives a clear objective and can review it later.
    -   Returning to the palace after the western quest produces the
        correct changed state.
-   **Unit-test goals:**
    -   Test story prerequisites and objective assignment.
    -   Test that the audience sequence is not accidentally repeated or
        skipped.
    -   Test post-quest dialogue and quest completion state.

### `WESTERN_ROAD` --- Route into the west

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P1`
-   **Purpose:** Transition from the central hub to the western story
    region.
-   **Required features:** a clear departure point, route landmarks, and
    escalating environmental cues.
-   **Interactions:** inspect route clues; handle authored obstacles;
    prepare for the Witch's territory.
-   **Exits:** `EMERALD_CITY` or its outward route, `WINKIE_COUNTRY`.
-   **Acceptance criteria:**
    -   The route is unavailable until the story permits departure, if
        that gate is used.
    -   The player can return to the city unless the design explicitly
        establishes a one-way sequence.
-   **Unit-test goals:**
    -   Test route unlock requirements.
    -   Test both directions of travel where return is allowed.
    -   Test that departure does not erase quest or party state.

### `WINKIE_COUNTRY` --- Country of the Winkies

-   **Source:** `NOVEL_1900`
-   **Priority:** `P1`
-   **Purpose:** Western region under the Wicked Witch of the West's
    power.
-   **Required features:** distinctive regional palette, inhabited
    spaces or traces of inhabitants, a navigable route to the Witch's
    stronghold, and optional discoveries.
-   **Interactions:** speak with residents where appropriate; gather
    clues; resolve obstacles related to the Witch's control.
-   **Exits:** `WESTERN_ROAD`, `WITCH_FORTRESS`.
-   **Acceptance criteria:**
    -   The region feels distinct from the eastern lands.
    -   The player's objective is understandable without a waypoint-only
        solution.
    -   Regional events update consistently after the Witch storyline.
-   **Unit-test goals:**
    -   Test region exits and access requirements.
    -   Test region-state changes after major story events.
    -   Test NPC and quest state after re-entry.

### `WITCH_FORTRESS` --- Wicked Witch of the West's stronghold

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P1`
-   **Purpose:** Resolve the western quest and create a memorable story
    climax.
-   **Required features:** clear approach, controlled encounter spaces,
    captives or obstacles only if used by the adaptation, and a readable
    finale.
-   **Interactions:** infiltrate or approach the stronghold; resolve the
    Witch encounter; trigger the region's aftermath.
-   **Exits:** `WINKIE_COUNTRY`.
-   **Acceptance criteria:**
    -   The player can understand the main objective and encounter
        rules.
    -   The climax cannot repeat in a way that duplicates rewards or
        breaks story flags.
    -   The aftermath is reflected in relevant NPCs and route states.
-   **Unit-test goals:**
    -   Test encounter state transitions and completion.
    -   Test one-time rewards and event triggers.
    -   Test that the western quest updates the Wizard objective and
        regional state correctly.

### `SOUTHERN_ROUTE` --- Route toward the Quadlings

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P1`
-   **Purpose:** Connect the central/western journey to the southern
    region and the final return-home guidance.
-   **Required features:** clear direction, landmarks, route obstacles,
    and an understandable reason to continue south.
-   **Interactions:** examine clues; resolve optional obstacles;
    discover regional context.
-   **Exits:** `QUADLING_COUNTRY`, plus previously visited areas where
    the world map allows return travel.
-   **Acceptance criteria:**
    -   The route becomes available at the appropriate story stage.
    -   The player is not forced into a long sequence without clear
        progress feedback.
-   **Unit-test goals:**
    -   Test route unlock conditions and destination validity.
    -   Test backtracking and progression flags.
    -   Test that optional encounters do not block the main route
        unintentionally.

### `HAMMER_HEADS` --- Hammer-Heads' territory

-   **Source:** `NOVEL_1900`
-   **Priority:** `P2`
-   **Purpose:** Optional hostile territory that makes the geography
    feel larger and less uniformly safe.
-   **Required features:** recognizable boundary, clear threat cues, and
    either a safe bypass or a fair, authored resolution.
-   **Interactions:** evade, negotiate, or overcome the obstacle
    according to the game design.
-   **Exits:** the southern route or adjacent area selected by the final
    map.
-   **Acceptance criteria:**
    -   The encounter is optional or clearly signposted as a required
        challenge.
    -   The player has a fair way to retreat or recover.
-   **Unit-test goals:**
    -   Test encounter outcomes and retreat.
    -   Test collision and damage/state transitions if combat exists.
    -   Test that bypassing the area does not corrupt quest progression.

### `CHINA_COUNTRY` --- Dainty China Country

-   **Source:** `NOVEL_1900`
-   **Priority:** `P2`
-   **Purpose:** A compact optional area with a fragile, uncanny
    environment and a distinct visual identity.
-   **Required features:** miniature scale cues, china
    inhabitants/objects, and rules that are communicated before
    consequences occur.
-   **Interactions:** talk with inhabitants; inspect fragile objects;
    choose whether to help or leave.
-   **Exits:** `SOUTHERN_ROUTE` or another adjacent route chosen during
    map design.
-   **Acceptance criteria:**
    -   The area feels mechanically and visually different without
        requiring a new engine subsystem.
    -   Any breakage or fragility rule is explained and reversible where
        appropriate.
-   **Unit-test goals:**
    -   Test area-specific interaction rules.
    -   Test object state changes and persistence.
    -   Test that leaving and returning does not duplicate or reset
        permanent outcomes unexpectedly.

### `QUADLING_COUNTRY` --- Country of the Quadlings

-   **Source:** `NOVEL_1900`
-   **Priority:** `P1`
-   **Purpose:** Southern region and approach to Glinda.
-   **Required features:** recognizable regional identity, at least one
    landmark, a clear route to Glinda's home, and optional local
    interactions.
-   **Interactions:** speak with residents; gather context; follow clues
    to `GLINDA_PALACE`.
-   **Exits:** `SOUTHERN_ROUTE`, `GLINDA_PALACE`.
-   **Acceptance criteria:**
    -   The region is clearly distinguished from the Munchkin and Winkie
        lands.
    -   The player can find the next story destination through world
        clues.
-   **Unit-test goals:**
    -   Test exits and destination resolution.
    -   Test any region-specific dialogue and quest flags.
    -   Test state consistency after backtracking.

### `GLINDA_PALACE` --- Glinda's home

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P1`
-   **Purpose:** Provide the final guidance needed to resolve Dorothy's
    journey home.
-   **Required features:** recognizable arrival point, dialogue
    sequence, story resolution, and a return-home transition.
-   **Interactions:** speak with Glinda; learn the return method;
    complete the ending sequence.
-   **Exits:** `QUADLING_COUNTRY`; `KANSAS_FARM` through the ending
    transition.
-   **Acceptance criteria:**
    -   The ending requires the intended story prerequisites.
    -   The return-home transition is reliable and preserves the
        completed ending state.
    -   The game does not strand the player after the finale.
-   **Unit-test goals:**
    -   Test ending prerequisites and transition behavior.
    -   Test completion flags and repeat-entry behavior.
    -   Test save/load around the ending if persistence is available.

### `NORTH_REGION` --- Northern area around the Good Witch of the North

-   **Source:** `NOVEL_1900` / `ADAPTATION`
-   **Priority:** `P2`
-   **Purpose:** Optional northern content grounded in the Good Witch of
    the North's presence in the original novel.
-   **Required features:** a distinct landmark and a clear reason to
    visit if this region becomes playable.
-   **Canon note:** The 1900 novel does not name this region Gillikin
    Country.
-   **Acceptance criteria:**
    -   The location is labeled as an adaptation where exact geography
        is invented.
    -   It does not contradict the primary route or falsely attribute
        later-book names to the original novel.
-   **Unit-test goals:**
    -   Test all declared exits and story gates.
    -   Test source/canon metadata if location records expose it to
        in-game codex or developer tools.

### `GILLIKIN_COUNTRY` --- Later-book northern country

-   **Source:** `LATER_OZ`
-   **Priority:** `P3`
-   **Purpose:** Future expansion for the broader Oz literary setting.
-   **Required features:** a clearly separate content package or region
    definition; references to later books should be documented.
-   **Acceptance criteria:**
    -   It is never mislabeled as a named region from the 1900 novel.
    -   It can be omitted entirely from the initial release without
        leaving broken references.
-   **Unit-test goals:**
    -   Test that optional region data can be absent without breaking
        map loading.
    -   Test that links to this region are validated when the expansion
        is enabled.

## Location data model

Prefer stable IDs rather than using display names as keys. The exact
Rust type should fit the current project architecture, but a location
record should conceptually include:

``` rust
struct LocationDefinition {
    id: LocationId,
    name: String,
    source: SourceTag,
    priority: ContentPriority,
    summary: String,
    exits: Vec<LocationId>,
    map_asset: Option<String>,
}
```

This is illustrative, not a requirement to adopt these exact types.
Avoid adding dependencies solely to support this data shape.

Recommended invariants:

-   Every `LocationId` is unique.
-   Every declared exit references an existing location or an explicitly
    optional expansion.
-   Bidirectional travel is declared intentionally; do not assume all
    exits are automatically reversible.
-   A location's discovery state, completed encounters, and story flags
    are separate from its static definition.
-   Rendering assets can be missing during early development without
    crashing the entire game; show a deliberate placeholder and a useful
    diagnostic.
-   Content definitions should not depend on Bevy entities being alive,
    so core graph validation can be unit-tested without launching the
    game.

## Implementation stages

### Stage 1 --- Location graph and graybox route

Implement the smallest coherent route: 1. `KANSAS_FARM` 2.
`MUNCHKIN_LANDS` 3. `YELLOW_BRICK_ROAD_EAST` 4. `WOODS_FIRST` 5.
`EMERALD_CITY_GATE` 6. `EMERALD_CITY` 7. `WIZARD_PALACE`

Use simple placeholder tiles and interactables. Confirm that the player
can move between locations, interact, and maintain progression state.

**AI development goals and tests** - Add unit tests for unique IDs,
valid exits, and route reachability. - Add tests for transitions,
arrival events, and quest-state changes. - Add an integration test for
the complete graybox route. - Run the existing test suite and report
regressions before marking the stage complete.

### Stage 2 --- Companions and journey obstacles

Add the Tin Woodman and Lion encounters, then the Kalidahs, bridge,
river, and poppy field.

**AI development goals and tests** - Add unit tests for companion
recruitment, encounter completion, hazard application, and crossing
prerequisites. - Add regression tests for duplicate recruitment,
repeated rewards, softlocks, and invalid transitions. - Add integration
coverage for reaching the Emerald City with the expected party state.

### Stage 3 --- Emerald City and western quest

Build the city, gatekeeper, Wizard's palace, western route, Winkie
Country, and Witch stronghold.

**AI development goals and tests** - Add unit tests for access
conditions, dialogue state, quest prerequisites, and one-time rewards. -
Add integration tests for the Wizard objective through western quest
completion. - Test returning to prior locations after story state
changes.

### Stage 4 --- Southern journey and ending

Add the southern route, optional Hammer-Heads and China Country areas,
Quadling Country, and Glinda's home.

**AI development goals and tests** - Add unit tests for optional route
handling, ending prerequisites, and return-home transitions. - Add
integration tests for the full main-story path. - Test backtracking,
save/load (when implemented), and replay-safe completion flags.

### Stage 5 --- Optional and expanded regions

Add northern content and later-book locations only after the core route
is playable and stable.

**AI development goals and tests** - Add unit tests for optional content
being absent or enabled. - Test unknown or missing destination
handling. - Add regression tests ensuring expansion content cannot break
the original-novel route.

## Definition of done for every location

A location is not complete merely because a map exists. Before marking
it done, verify:

-   [ ] Stable location ID and source label are documented.
-   [ ] The purpose and player-facing landmark are clear.
-   [ ] Entry and exit routes are defined and validated.
-   [ ] Collision and navigation boundaries are implemented.
-   [ ] Interactables and NPCs have explicit state transitions.
-   [ ] Quest, party, and item effects are defined.
-   [ ] Re-entry behavior is intentional.
-   [ ] Missing assets fail gracefully with a useful diagnostic.
-   [ ] Unit tests cover the location's logic.
-   [ ] Integration/regression tests cover relevant transitions and
    story state.
-   [ ] Acceptance criteria pass.
-   [ ] README/ROADMAP/design documentation is updated if architecture
    or player-facing scope changes.

## Open design decisions

Resolve these as the world map becomes concrete; do not block the first
graybox route on optional content.

-   [ ] **Map topology:** one connected overworld with sub-areas, or
    distinct maps connected by explicit exits?
-   [ ] **Movement scale:** tile-based grid, free movement, or a hybrid?
-   [ ] **World map:** should the player see a map, discover one, or
    navigate through landmarks alone?
-   [ ] **Companion behavior:** followers in the world, party
    represented abstractly, or a hybrid?
-   [ ] **Combat:** turn-based, real-time, nonviolent encounters, or a
    mixture?
-   [ ] **Fast travel:** whether it exists and which locations it can
    connect.
-   [ ] **Canon policy:** which later-book elements are allowed in the
    initial game, if any?
-   [ ] **Art pipeline:** tile size, map format, collision format, and
    naming convention for assets.
-   [ ] **Persistence:** which discoveries, doors, puzzles, NPCs, and
    encounters retain state after leaving or reloading.

## Reference

Primary source: L. Frank Baum, *The Wonderful Wizard of Oz* (1900),
available through Project Gutenberg: https://www.gutenberg.org/ebooks/55

Use the novel as the source for its named places and journey episodes.
When a map layout, exact position, or additional detail is invented for
gameplay, label it as an adaptation or original game content rather than
presenting it as a fact from the book.
