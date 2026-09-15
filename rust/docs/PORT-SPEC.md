# ONE PIECE GRAND LINE TCG ("TCGOP") — Definitive Rust Port Specification

Source of truth: the TypeScript engine at `/home/user/TCGOP/src` (types, engine, data, hooks, lib, components).
This document is the merged, exhaustive port specification. Where the TypeScript code and its own
comments disagree, **the code is authoritative** and the divergence is recorded in §8.

Guiding rule for the port: **zero behavioural drift**. Every constant, iteration order, tie-break,
clamp, rounding, log string and apparent bug documented here must be reproduced unless §8 explicitly
marks it as a decision point and the porter records the decision.

---

## 1. Overview & Architecture

### 1.1 Design constraints

| Constraint | Consequence for the Rust port |
|---|---|
| Deterministic | All randomness (`Math.random`) and all clocks (`Date.now()`) must be replaced by an injected `Rng` and an injected id counter. No `SystemTime`, no `rand::thread_rng`. |
| Externally drivable (WASM-friendly) | Engine core is a **pure library crate** with no I/O, no threads, no globals except an explicitly-passed registry. All public state types are `serde::Serialize + Deserialize`. |
| Immutable-update semantics | The TS engine uses `immer.produce`: every engine function takes a state and returns a (possibly new) state, and a thrown error discards all partial mutations of that call. In Rust: take `&GameState`, clone-on-write internally, return `Result<GameState, EngineError>`; on `Err` the caller keeps the original state. Cheap alternative: `fn apply(&mut self, …) -> Result<(), E>` where the caller snapshots first — but the *transactional* semantic (partial mutations discarded on error) is mandatory. |
| UI-facing API | `apply(state, action) -> Result<GameState, EngineError>`, `valid_actions(state, player) -> Vec<GameAction>`, plus an append-only `log: Vec<LogEntry>` and a derived announcement builder. |

### 1.2 Crate layout

```
tcgop-engine/                (pure library, no_std-friendly except alloc; serde on all state types)
  src/
    lib.rs                   re-exports; Engine facade: apply / valid_actions / new_game
    types.rs                 §2 — every enum & struct mirroring src/types/index.ts
    utils.rs                 constants, slot tables, adjacency, shuffle, instance-id generator
    rng.rs                   trait Rng { fn next_f64(&mut self) -> f64; } + Xoshiro/PCG impl + draw-order docs
    registry.rs              CardRegistry / CaptainRegistry (src/engine/cardRegistry.ts + init.ts)
    state.rs                 GameState construction + turn lifecycle (src/engine/gameState.ts)
    board.rs                 board queries, effective stats, deploy/equip/ship/move/remove, targeting
    combat.rs                declare / counter window / resolve pipeline
    turn.rs                  action dispatcher + valid-action generator + events/ships/support/haki
    passives.rs              start-of-turn passives, aura recalculation, on-KO hook
    fruits.rs                devil-fruit equip/awaken/traits
    haki.rs                  observation / armament / king
    will.rs                  Volonté (gain / spend / afford / KO bonus)
    captain.rs               flip (engage), entry effects, captain base attack
    ai.rs                    difficulty levels, scoring heuristic, evaluation, 1-ply lookahead
    announce.rs              PlayAnnouncement builder + reveal durations (UI pacing contract)
    decks.rs                 the four 50-card DeckDefs
    cards/
      mod.rs                 all_cards() = mugiwara ++ marines ++ baroque ++ redhair ++ tokens
      mugiwara.rs            ST01 (28 cards)
      marines.rs             ST02 (28 cards)
      baroque.rs             ST03 (28 cards)
      redhair.rs             ST04 (27 cards)
      tokens.rs              3 token bodies
      captains.rs            4 CaptainDefs (order: Luffy, Akainu, Crocodile, Shanks)
```

**23 modules.** `tcgop-wasm` (thin `wasm-bindgen` shim: JSON in / JSON out) and `tcgop-cli` (headless
self-play harness for differential testing against the TS engine) are separate binaries/crates and are
not part of the engine module count.

### 1.3 Public API surface

```rust
pub struct Engine { registry: Registry }

impl Engine {
    pub fn new() -> Self;                                   // = initializeRegistry()
    pub fn new_game(&self, p1: &DeckDef, p2: &DeckDef, rng: &mut dyn Rng, ids: &mut IdGen)
        -> GameState;                                       // = createGame(): init + startTurn
    pub fn apply(&self, s: &GameState, a: &GameAction, rng: &mut dyn Rng, ids: &mut IdGen)
        -> Result<GameState, EngineError>;                   // = executeAction()
    pub fn valid_actions(&self, s: &GameState, p: PlayerId) -> Vec<GameAction>;
    pub fn ai_choose(&self, s: &GameState, p: PlayerId, d: Difficulty, rng: &mut dyn Rng)
        -> GameAction;
    pub fn evaluate(&self, s: &GameState, p: PlayerId) -> f64;
    pub fn build_announcement(&self, a: &GameAction, pre: &GameState, human: PlayerId)
        -> Option<PlayAnnouncement>;
    pub fn check_win(&self, s: &GameState) -> Option<PlayerId>;

    // --- required by the reference UI; NOT derivable from a serialised GameState ---
    pub fn effective_atk(&self, s: &GameState, id: &str) -> i32;   // = getEffectiveAtk (§4.3)
    pub fn effective_def(&self, s: &GameState, id: &str) -> i32;   // = getEffectiveDef (§4.3)
    pub fn card_def(&self, id: &str) -> &CardDef;                  // = getCardDef, panics/errors on miss
    pub fn captain_def(&self, id: &str) -> &CaptainDef;            // = getCaptainDef
}

// Slot tables the UI lays the board out with (src/engine/utils.ts:43,46)
pub const FRONT_SLOTS: [Slot; 3] = [Slot::V1, Slot::V2, Slot::V3];
pub const BACK_SLOTS:  [Slot; 3] = [Slot::A1, Slot::A2, Slot::A3];
pub const ALL_SLOTS:   [Slot; 6] = [Slot::V1, Slot::V2, Slot::V3, Slot::A1, Slot::A2, Slot::A3];
```

**The eight game-loop methods alone are not a sufficient public surface.** The reference UI imports
engine queries and *static-registry* accessors directly, and the card catalogue is **not** part of
`GameState`, so nothing in a serialised state can stand in for them:

| UI import | Call site | Purpose |
|---|---|---|
| `getEffectiveAtk` | `ActionMenu.tsx:4,27`, `FullCard.tsx:4,42` | live ATK line, attack preview, the `"ATK 0"` disable reason |
| `getEffectiveDef` | `FullCard.tsx:4,43` | live DEF line |
| `getCardDef` | `FullCard.tsx:5`, `ActionMenu.tsx:5`, `Game.tsx:7`, `CardDetail.tsx:4`, `PlayRevealLayer.tsx:4`, `useCombatVfx.ts:5` | every card art / name / cost / text lookup; `CardDetail` resolves attached-object defs, `PlayRevealLayer` the revealed-card face, `useCombatVfx` the printed `pv` used as the HP-bar denominator when diffing snapshots |
| `getCaptainDef` | `Game.tsx:7`, `useCombatVfx.ts:5` | command-card faces; `useCombatVfx` also needs it **outside `Game.tsx`** for the captain PV denominator in its damage/KO diffing |
| `initializeRegistry` | `Game.tsx:8,29` (module scope, before first render) | populates the registry; = `Engine::new()` |
| `FRONT_SLOTS` / `BACK_SLOTS` | `Game.tsx:25` | board row layout |

(`src/lib/announce.ts:7` imports `getCardDef` as well, but it is the TS source of `buildAnnouncement`
and is therefore subsumed by the `build_announcement` engine method above, not an extra UI consumer.)

For the WASM shim of §1.2 this means the "JSON in / JSON out" surface must additionally export
`effective_atk`/`effective_def` (state JSON + instance id → number) and a **one-shot catalogue dump**
(`all_card_defs()` / `all_captain_defs()` → JSON `Record<id, Def>`, fetched once after
`Engine::new()`), because the JS side cannot hold a `&CardDef` across the boundary. `Engine::new()`
subsumes `initializeRegistry()`; the slot tables are plain constants and can also be re-exported as
JSON arrays.

`EngineError` is a plain enum whose `Display` reproduces the **exact** English/French error strings of
the TS engine (they are listed verbatim in §4); several are user-visible through `console.error`.

Notes on ordering guarantees the port must preserve:

* `PlayerState.board` must iterate **V1, V2, V3, A1, A2, A3** everywhere. TS relies on JS object key
  insertion order; Rust must use a fixed `[Slot; 6]`-backed map or always iterate `ALL_SLOTS`.
* `deck` is a queue whose **top is index 0** (`shift()` draws, `push` appends to the bottom).
* `hand` is ordered; "oldest" = index 0 (auto-discard and hand-limit discard take from the front).
* `graveyard` is append-ordered; `removeFromBoard` pushes attached objects first, then the character.
* `valid_actions` output order is load-bearing (AI tie-breaks are "first wins"): the exact generation
  order is specified in §4.9.

### 1.4 Determinism contract

| TS source of nondeterminism | Where | Rust replacement |
|---|---|---|
| `Math.random()` in `shuffle` | deck shuffle at game creation | `Rng`; Fisher–Yates `for i in (1..len).rev() { j = floor(rng*(i+1)); swap }` on a **copy** |
| `Math.random()` in `entryDiscardRandom` (deploy) | board.rs | `idx = floor(rng * hand.len())` |
| `Math.random()` in `discardOpponentRandom` (captain entry) | captain.rs | same |
| `Math.random()` in AI beginner | ai.rs | exact draw order in §4.11 |
| `Date.now()` in `generateInstanceId` | utils.rs | id = `format!("{defId}_{counter}_{suffix}")`, counter pre-incremented from 0, `suffix` a fixed per-game string |
| `Date.now()` in modifier ids | passives/turn/captain/combat | deterministic counter; ids are never matched on except by prefix (see §4.7) |

---

## 2. Data Model

All names in doc comments are the original TS names. `Option<T>` is mandatory wherever TS has `?`;
do **not** collapse `undefined` to a default unless this document says the engine does.

### 2.1 Scalar enums

```rust
/// TS: PlayerId
pub enum PlayerId { Player1, Player2 }          // "player1" | "player2"

/// TS: CardType
pub enum CardType { Character, Object, Ship, Event, Counter }

/// TS: ObjectSubtype
pub enum ObjectSubtype { Weapon, Fruit, Accessory }

/// TS: Element
pub enum Element { Fire, Water, Thunder, Ice, Sand, Poison }

/// TS: Trait
pub enum Trait {
    Shield,     // taps to block for 1 adjacent ally
    Range,      // attacks from the back row, free target
    Stealth,    // untargetable while a non-stealth ally exists
    Rush,       // ignores summoning sickness (attacks only)
    Cursed,     // Devil Fruit bearer, vulnerable to Water/Granite
    Logia,      // intangibility: ignores damage without Haki/Water/Granite
    Piercing,   // all attacks halve DEF
    Conqueror,  // access to King's Haki (T10+)
}

/// TS: AttackTrait   (overlaps Trait on Range/Piercing — keep them distinct enums)
pub enum AttackTrait { Range, Piercing, Zone, Total, Impact }

/// TS: HakiType
pub enum HakiType { Observation, Armament, King }

/// TS: Slot   (V* = front / "Avant", A* = back / "Arrière")
pub enum Slot { V1, V2, V3, A1, A2, A3 }

/// TS: Row  — only used by CardDef.preferredRow (AI placement hint)
pub enum Row { Front, Back }

/// TS: Phase — only Main and End are ever assigned by the engine
pub enum Phase { Untap, Draw, WillGain, Main, End }

/// TS: Faction
pub enum Faction { Pirate, Marine, Revolutionary, Independent }

/// TS: CardDef.rarity
pub enum Rarity { C, U, R, SR, L, CAP }

/// TS: CardDef.grantsTraits is (Trait | AttackTrait)[]
pub enum GrantedTrait { Char(Trait), Attack(AttackTrait) }
```

### 2.2 Static catalogue types

```rust
/// TS: BaseAction
pub struct BaseAction {
    pub name: String,
    pub atk: i32,                          // ATK for attacks, 0 for support actions
    pub attack_traits: Option<Vec<AttackTrait>>,
    pub element: Option<Element>,
    pub is_support: Option<bool>,
    pub heal_amount: Option<i32>,
    pub immobilize: Option<bool>,
    pub cannot_be_dodged: Option<bool>,    // NEVER read by combat.ts for base attacks
    pub strip_stealth: Option<bool>,
    pub scry: Option<i32>,                 // Nami "Prévisions"
    pub bluff: Option<bool>,               // Usopp "Bluff": enemy DEF <= 1 loses next action
    pub buff_ally_atk: Option<i32>,        // Brook "Mélodie"
    pub description: Option<String>,
}

/// TS: SpecialAttack   (Total ATK = effective base ATK + atkBonus)
pub struct SpecialAttack {
    pub name: String,
    pub cost: i32,                         // Volonté
    pub atk_bonus: i32,
    pub attack_traits: Option<Vec<AttackTrait>>,
    pub element: Option<Element>,
    pub once_per_game: Option<bool>,
    pub ignore_def: Option<i32>,
    pub is_support: Option<bool>,
    pub heal_amount: Option<i32>,
    pub immobilize: Option<bool>,
    pub sleep: Option<bool>,
    pub cannot_be_dodged: Option<bool>,
    pub ignore_shield: Option<bool>,
    pub pushback: Option<bool>,
    pub pushback_slots: Option<i32>,       // declared, never propagated by combat.ts
    pub strip_stealth: Option<bool>,
    pub conditional_bonus: Option<ConditionalBonus>,
    pub permanent_pv_loss: Option<i32>,    // not implemented by combat.ts
    pub two_targets: Option<bool>,         // approximated as AttackTrait::Zone
    pub no_heal: Option<bool>,             // not implemented by combat.ts
    pub transform: Option<Transform>,
    pub description: Option<String>,
}
pub struct ConditionalBonus { pub vs_trait: Option<Trait>, pub vs_faction: Option<Faction>, pub amount: i32 }
pub struct Transform { pub atk: i32, pub def: i32, pub pv: i32, pub turns: i32 }

/// TS: SynergyDef
pub struct SynergyDef { pub partner_id: String, pub atk_bonus: i32, pub on_partner_ko: Option<i32> }

/// TS: PassiveDef
pub struct PassiveDef { pub name: String, pub description: String, pub effects: Vec<PassiveEffect> }

/// TS: AllyFilter — all fields optional; empty filter matches everything
pub struct AllyFilter { pub faction: Option<Faction>, pub tag: Option<String>, pub trait_: Option<Trait> }

/// TS: CardDef — ONE flat struct for all five card types (mirror it flat; do not tag it)
pub struct CardDef {
    pub id: String, pub name: String, pub type_: CardType, pub cost: i32,
    pub faction: Faction, pub rarity: Rarity, pub set: String,
    pub is_token: Option<bool>,
    // character
    pub atk: Option<i32>, pub def: Option<i32>, pub pv: Option<i32>,
    pub traits: Option<Vec<Trait>>, pub preferred_row: Option<Row>,
    pub tags: Option<Vec<String>>, pub natural_haki: Option<Vec<HakiType>>,
    pub base_action: Option<BaseAction>, pub special_attack: Option<SpecialAttack>,
    pub passive: Option<PassiveDef>, pub synergies: Option<Vec<SynergyDef>>,
    // object
    pub subtype: Option<ObjectSubtype>,
    pub bonus_atk: Option<i32>, pub bonus_def: Option<i32>,
    pub restriction: Option<String>,             // character-name fragment OR tag; engine-defined match
    pub grants_traits: Option<Vec<GrantedTrait>>,
    pub grants_element: Option<Element>,
    pub equip_effect: Option<String>,            // FREE TEXT
    pub fruit_effects: Option<FruitEffects>,
    // ship
    pub ship_passive: Option<String>,            // FREE TEXT, regex-parsed (see §4.4)
    pub ship_active: Option<ShipActive>,
    pub ship_destroy_effect: Option<ShipDestroyEffect>,
    // event / counter
    pub event_effect: Option<EventEffect>,
    pub counter_effect: Option<CounterEffect>,
}

pub struct FruitEffects { pub base: FruitBase, pub awakening: Option<FruitAwakening> }
pub struct FruitBase {
    pub grants_traits: Option<Vec<Trait>>, pub passive_description: Option<String>,
    pub atk_bonus: Option<i32>, pub def_bonus: Option<i32>,
}
pub struct FruitAwakening {
    pub porteur_legitime: String,   // bearer NAME fragment allowed to awaken
    pub min_turns: i32, pub vol_cost: i32,
    pub grants_traits: Option<Vec<Trait>>, pub atk_bonus: Option<i32>, pub def_bonus: Option<i32>,
    pub passive_description: Option<String>,
    pub special_attack: Option<FruitSpecialAttack>,
}
/// NOT the same shape as SpecialAttack: `description` is REQUIRED and it lacks
/// isSupport, healAmount, cannotBeDodged, pushbackSlots, conditionalBonus,
/// permanentPvLoss, twoTargets, noHeal, transform.
pub struct FruitSpecialAttack {
    pub name: String, pub cost: i32, pub atk_bonus: i32, pub description: String,
    pub once_per_game: Option<bool>, pub attack_traits: Option<Vec<AttackTrait>>,
    pub element: Option<Element>, pub ignore_def: Option<i32>,
    pub immobilize: Option<bool>, pub sleep: Option<bool>, pub pushback: Option<bool>,
    pub ignore_shield: Option<bool>, pub strip_stealth: Option<bool>,
}
pub struct ShipActive { pub name: String, pub cost: i32, pub description: String, pub once_per_game: Option<bool> }
pub struct ShipDestroyEffect {
    pub heal_all: Option<i32>, pub draw: Option<i32>,
    pub buff_atk: Option<i32>, pub buff_def: Option<i32>,   // DECLARED BUT NEVER HANDLED
    pub deploy_token: Option<String>,
}

/// TS: CaptainDef
pub struct CaptainDef {
    pub id: String, pub name: String, pub faction: Faction,
    pub tags: Option<Vec<String>>, pub traits: Option<Vec<Trait>>,   // top-level traits
    pub recto: CaptainRecto,
    pub flip_condition: FlipCondition,
    pub verso: CaptainVerso,
}
pub struct CaptainRecto {                 // NO baseAction, NO traits, NO naturalHaki
    pub pv: i32, pub atk: i32, pub def: i32,
    pub passive: PassiveDef,
    pub attacks: Vec<SpecialAttack>,      // always [] in the shipped data
    pub surcharge: Option<SpecialAttack>, // never populated in the shipped data
}
pub struct FlipCondition {                // every field optional; {} is legal
    pub cost: Option<i32>,
    pub auto_if_allies_lte: Option<i32>,
    pub free_if_ally_ko: Option<bool>,
    pub free_if_enemy_cursed: Option<bool>,   // NEVER read by captain.ts
    pub free_if_allies_gte: Option<i32>,      // NEVER read by captain.ts
    pub free_if_turn_gte: Option<i32>,        // NEVER read by captain.ts
}
pub struct CaptainVerso {
    pub pv: i32, pub atk: i32, pub def: i32,
    pub passive: PassiveDef, pub entry_effect: EntryEffect,
    pub base_action: BaseAction, pub special_attack: SpecialAttack,
    pub surcharge: Option<SpecialAttack>,
    pub traits: Option<Vec<Trait>>, pub natural_haki: Option<Vec<HakiType>>,
}

/// TS: DeckEntry / DeckDef
pub struct DeckEntry { pub card_id: String, pub count: i32 }
pub struct DeckDef { pub name: String, pub captain_id: String, pub cards: Vec<DeckEntry> }
```

### 2.3 Runtime state types

```rust
/// TS: Modifier — note `stat` here has a 4th value "haki" that no effect ever emits
pub struct Modifier {
    pub id: String, pub stat: ModStat, pub amount: i32,
    pub source: String,                     // prefix-matched; see §4.7
    pub duration: ModDuration, pub turns_remaining: Option<i32>,  // never decremented anywhere
}
pub enum ModStat { Atk, Def, Pv, Haki }
pub enum ModDuration { Permanent, Turn, NextTurn }

/// TS: StatusEffect — all 4 fields REQUIRED (damagePerTurn is 0 for non-damaging types)
pub struct StatusEffect { pub type_: StatusType, pub turns_remaining: i32, pub damage_per_turn: i32, pub source: String }
pub enum StatusType { Burn, Poison, Freeze, Desiccation, Trap, Immobilize, Sleep, LoseAction, SelfKO, NoStealth, NoHeal }
// turnsRemaining == -1 means permanent (used by poison and by trap)

/// TS: CardInstance — used for EVERY card type incl. objects/ships/events/counters
pub struct CardInstance {
    pub instance_id: String, pub def_id: String, pub owner: PlayerId,
    pub zone: Zone, pub slot: Option<Slot>, pub tapped: bool, pub current_pv: i32,
    pub attached_objects: Vec<String>,       // instanceIds
    pub modifiers: Vec<Modifier>, pub status_effects: Vec<StatusEffect>,
    pub deployed_turn: Option<i32>,
    pub used_base_action: bool, pub used_special_attack: bool,
    pub logia_used_this_turn: Option<bool>,
    pub used_once_abilities: Vec<String>,    // values: SpecialAttack.name, "survived", "strawhat", ShipActive.name
    pub is_awakened: Option<bool>,
}
pub enum Zone { Deck, Hand, Board, Graveyard, Banished }

/// TS: CaptainInstance — no instanceId, no zone, no attachedObjects, no logiaUsedThisTurn
pub struct CaptainInstance {
    pub def_id: String, pub owner: PlayerId, pub flipped: bool, pub current_pv: i32,
    pub slot: Option<Slot>, pub tapped: bool,
    pub modifiers: Vec<Modifier>, pub status_effects: Vec<StatusEffect>,
    pub deployed_turn: Option<i32>,
    pub used_base_action: bool, pub used_special_attack: bool, pub used_once_abilities: Vec<String>,
}

/// TS: PlayerState
pub struct PlayerState {
    pub id: PlayerId, pub captain: CaptainInstance,
    pub deck: Vec<String>,      // instanceIds, top = index 0
    pub hand: Vec<String>, pub graveyard: Vec<String>,
    pub board: BoardMap,        // Slot -> Option<instanceId>, ALWAYS all six keys
    pub active_ship: Option<String>,
    pub volonte: i32,
    pub used_free_move: bool, pub has_drawn: bool,
    pub observation_used: bool, pub armament_used: bool,
    //   armament_used is WRITTEN but never READ. Write sites (both required for parity):
    //     * `armamentUsed: false` in `createPlayerState` (gameState.ts:92) — initial value;
    //     * `player.armamentUsed = false` in `resetTurnFlags` (gameState.ts:312), i.e. every
    //       `startTurn` for the incoming player only — see §4.2 step 2, which already lists it.
    //   No engine site ever reads it (contrast `observation_used` / `king_used`), but it IS part of
    //   the serialised `PlayerState` the reference UI receives, so the field MUST be kept and must
    //   round-trip; dropping it as "unused" breaks serialisation parity with the TS engine.
    pub king_used: bool,
    pub ally_koed_this_turn: Option<bool>,
    pub char_koed_this_game: Option<bool>,
    pub haki_this_turn: Option<bool>,
}

/// TS: PendingAttack  (+ an undeclared `survivePlayed` flag attached by combat.ts via a cast)
pub struct PendingAttack {
    pub attacker_id: String,      // instanceId OR the literal "captain_player1" / "captain_player2"
    pub target_id: String,        // instanceId OR "captain_<pid>" when target_is_captain
    pub target_is_captain: bool, pub is_special: bool,
    pub raw_damage: i32, pub attack_power: Option<i32>,
    pub element: Option<Element>, pub attack_traits: Vec<AttackTrait>, pub has_haki: bool,
    pub ignore_shield: Option<bool>, pub cannot_be_dodged: Option<bool>,
    pub immobilize: Option<bool>, pub sleep: Option<bool>,
    pub pushback: Option<bool>, pub pushback_slots: Option<i32>,  // never set by the engine
    pub strip_stealth: Option<bool>,
    pub survive_played: Option<bool>,   // set by applyCounterSurvive
}

/// TS: LogEntry / GameState
pub struct LogEntry { pub turn: i32, pub player: PlayerId, pub message: String }
pub struct GameState {
    pub cards: HashMap<String, CardInstance>,   // ALL zones, BOTH players
    pub players: PlayersMap,                    // exactly two keys
    pub turn_number: i32,
    pub current_player: PlayerId,
    pub phase: Phase,
    pub pending_attack: Option<PendingAttack>,
    pub log: Vec<LogEntry>,
    pub winner: Option<PlayerId>,
    pub first_player: PlayerId,                 // always Player1; never changed
}
```

### 2.4 `GameAction` — all 18 variants

```rust
pub enum GameAction {
    DeployCharacter { instance_id: String, slot: Slot },
    EquipObject { object_instance_id: String, target_instance_id: String },
    DeployShip { instance_id: String },
    BaseAttack { attacker_instance_id: String, target_instance_id: String, target_is_captain: Option<bool> },
    SpecialAttack { attacker_instance_id: String, target_instance_id: String, target_is_captain: Option<bool> },
    BaseSupportAction { instance_id: String, target_instance_id: Option<String> },
    PlayEvent { instance_id: String, targets: Option<Vec<String>> },   // `targets` IGNORED by the reducer
    PlayCounter { instance_id: String },
    UseShield { blocker_instance_id: String },
    PassCounter,
    FlipCaptain { slot: Slot },
    CaptainAttack { target_instance_id: String, target_is_captain: Option<bool>, is_special: Option<bool> }, // is_special IGNORED
    UseHaki { haki_type: HakiType, target_instance_id: Option<String> },  // target IGNORED
    MoveCharacter { instance_id: String, target_slot: Slot },
    ActivateShip { ship_instance_id: String },
    AwakenFruit { fruit_instance_id: String },
    FruitSpecialAttack { attacker_instance_id: String, fruit_instance_id: String,
                         target_instance_id: String, target_is_captain: Option<bool> },
    EndTurn,
}
```

Actions carry no player id: the actor is always `state.current_player`, except `PlayCounter`,
`UseShield` and `PassCounter` which act on `state.pending_attack` (the defender is
`opponent(current_player)`).

### 2.5 Effect unions

```rust
/// TS: PassiveEffect — 31 variants
pub enum PassiveEffect {
    BuffAlly { stat: BuffStat3, amount: i32, filter: Option<AllyFilter> },  // stat: atk|def|pv
    HealAdjacent { amount: i32 },
    OnAllyKO { effect: OnAllyKoKind /* only BonusWill */, amount: i32 },
    NaturalHaki { haki_type: HakiType },
    ThreeWeaponSlots, TwoAccessorySlots, CannotAttackFemale,
    Revive { pv: i32 },
    LogiaIntangibility, ImmuneImpact,
    StartTurnBuffAlly { stat: BuffStat2, amount: i32 },                     // stat: atk|def
    NoDodge,
    BlockDamageReduction { amount: i32 },
    StripStealthOnAttack,
    DebuffAdjacentEnemies { amount: i32 },
    DebuffOneEnemy { amount: i32 },
    BanishOnKO,
    ExplodeOnKO { amount: i32 },
    CopyAtkOnDeploy, EntryDiscardRandom, ImmuneControl,
    EndTurnDesiccation { amount: i32 },
    TwoWeaponSlots, AttacksIgnoreShield, GrantObservationAll,
    SelfBuffOnAllyKO { stat: BuffStatAtk, amount: i32, max: i32, filter: Option<AllyFilter> },
    MeleeRecoil { amount: i32 },
    CostReduction { filter: Option<AllyFilter>, amount: i32 },
    HakiCostReduction { amount: i32 },
    IgnoreEnemyStealth,
    Custom { id: String },                    // NOTE: no description, unlike Event/Entry custom
}

/// TS: EventEffect — 15 variants
pub enum EventEffect {
    GainWill { amount: i32 },
    Draw { amount: i32, discard: Option<i32> },
    HealAlly { amount: i32, all_allies: Option<bool> },
    BuffAllies { stat: BuffStat2, amount: i32, filter: Option<AllyFilter>, duration: EffDuration },
    DamageEnemies { amount: i32, target: DamageTarget4, cursed_bonus: Option<i32>,
                    sand: Option<bool>, destroy_ships: Option<bool> },
    DodgeAll,
    Rally { atk: i32, def: i32, heal: i32 },
    BuffSingle { stat: BuffStat2, amount: i32, duration: EffDuration, requires_own_ko: Option<bool> },
    RushBuff { atk: i32 },
    Tutor { filter_tag: Option<String>, max_cost: Option<i32> },
    DeployTokens { token_id: String, count: i32 },
    HealAllBuff { heal: i32, atk: i32 },
    DebuffAllEnemies { atk: i32, immobilize_max_def: Option<i32> },
    GrantHakiAll { atk: Option<i32> },
    Custom { id: String, description: String },
}
pub enum DamageTarget4 { AllFront, AllCursed, All, Single }
pub enum EffDuration { Turn, Permanent }

/// TS: CounterEffect — 4 variants
pub enum CounterEffect {
    Survive { description: String },
    ReduceDamage { amount: i32, captain_bonus: Option<i32> },   // no description field
    Cancel { description: String, max_attacker_atk: Option<i32>,
             self_captain_damage: Option<i32>, once: Option<bool> },
    Untargetable { description: String },
}

/// TS: EntryEffect — 9 variants (recursive); DISTINCT from EventEffect
pub enum EntryEffect {
    BuffAllies { stat: BuffStat2, amount: i32, duration: EntryDurationTurnOnly },  // no filter
    Draw { amount: i32 },                                                          // no discard
    DamageEnemies { amount: i32, target: DamageTarget2, cursed_bonus: Option<i32>, sand: Option<bool> },
    GrantSelfRush,
    Haoshoku { immobilize_max_def: i32, debuff_atk: i32 },
    DebuffAllEnemies { atk: i32 },
    DiscardOpponentRandom { amount: i32 },
    Multi { effects: Vec<EntryEffect> },
    Custom { id: String, description: String },
}
pub enum DamageTarget2 { AllFront, Single }
```

### 2.6 Constants

```rust
pub const VOLONTE_CAP: i32 = 10;
pub const STARTING_HAND_SIZE: usize = 6;
pub const HAND_LIMIT: usize = 10;
pub const ALLY_KO_BONUS_VOL: i32 = 2;
pub const ALL_SLOTS:   [Slot; 6] = [V1, V2, V3, A1, A2, A3];
pub const FRONT_SLOTS: [Slot; 3] = [V1, V2, V3];
pub const BACK_SLOTS:  [Slot; 3] = [A1, A2, A3];
// ADJACENCY (orthogonal only, symmetric, no diagonals) — ORDER MATTERS
// V1 -> [V2, A1]      V2 -> [V1, V3, A2]     V3 -> [V2, A3]
// A1 -> [A2, V1]      A2 -> [A1, A3, V2]     A3 -> [A2, V3]
pub const HAKI_THRESHOLDS: observation = 5, armament = 7, king = 10;   // vs state.turn_number
pub const ARMAMENT_UNIVERSAL_TURN: i32 = 7;   // combat: turn_number >= 7 => hasHaki
pub const KING_HAKI_DEF_THRESHOLD: i32 = 3;   // KOs enemies with effective DEF <= 3
```

---

## 3. Effect Taxonomy

70 effect discriminators total: 31 `PassiveEffect` + 15 `EventEffect` + 4 `CounterEffect`
+ 9 `EntryEffect` + 11 `StatusEffect` types. "IMPLEMENTED" = some engine file acts on it;
"DATA-ONLY" = declared in the type union but no engine file reads it.

**IMPLEMENTED ≠ reachable — a third, orthogonal axis.** "**UNREACHABLE**" below means the engine
implements the branch, but **no card in the shipped catalogue of §5 ever produces it**, so the branch
is dead with the shipped data. Ports must still implement unreachable branches (custom decks / future
data sets can hit them), but must NOT expect the differential-test harness of §1.2 (`tcgop-cli`) to
cover them: they carry **zero** parity signal, and are precisely where a silent divergence can hide
undetected. Every unreachable variant is tagged inline.

### 3.1 PassiveEffect (31)

| Variant | Trigger timing | Semantics (as implemented) |
|---|---|---|
| `buffAlly{stat,amount,filter?}` | Continuous; recomputed by `recalculatePassiveBuffs` on deploy / equip / KO / flip / awaken | Captain version buffs **all** own board characters matching the filter (`id: captain_<stat>_<charId>`, `source: captain_<capDefId>`). Character version buffs all **other** own board characters (`id: passive_<srcId>_<stat>_<targetId>`, `source: passive_<srcId>`). Duration `permanent`. |
| `healAdjacent{amount}` | Start of owner's turn (`applyStartOfTurnPassives`) | Heals each adjacent ally by `amount`, capped at the **printed** `CardDef.pv` (so a unit above printed PV is *lowered*). Logs `"<src> soigne <n> PV aux adjacents"` even if nothing was healed. |
| `onAllyKO{effect:"bonusWill",amount}` | On any own character KO (`applyOnKOEffects`) | **UNREACHABLE** — 0 occurrences in the shipped catalogue: no captain face (recto or verso) of the four captains of §5.3 carries it, so no player ever gains the uncapped +Vol and the byte-for-byte log string here is never emitted. Semantics if reached: captain passive only. `players[koOwner].volonte += amount`, **uncapped**. Logs `"Passif Capitaine : +<n> Vol. (allie KO)"`. |
| `naturalHaki{hakiType}` | — | **DATA-ONLY** in the passive form. Combat reads `CardDef.naturalHaki[]` instead. |
| `threeWeaponSlots` | Equip validation | **UNREACHABLE** — 0 occurrences in the shipped catalogue, so the weapon cap of §4.4 is **always 1 or 2, never 3**. `board.ts:368` comments `// Check for exceptions (Zoro 3 weapons, Franky 2 accessories)`, but **MG-001 Zoro** (`mugiwara.ts:7`) declares no `passive` field at all. Semantics if reached: max 3 weapon-subtype objects on bearer. |
| `twoAccessorySlots` | Equip validation | **UNREACHABLE** — 0 occurrences in the shipped catalogue, so the accessory cap is **always 1**. Same stale `board.ts:368` comment: **MG-007 Franky** (`mugiwara.ts:84`) likewise declares no `passive` field. Semantics if reached: max 2 accessory-subtype objects. |
| `cannotAttackFemale` | Valid-action generation only | Attack actions against targets whose `CardDef.tags` include `"female"` are not generated (base + special; **not** applied to captain targets, and not applied to `fruitSpecialAttack`). Not re-checked by the reducer. |
| `revive{pv}` | — | **DATA-ONLY**. |
| `logiaIntangibility` | — | **DATA-ONLY** as a passive; combat uses `hasTrait(logia)` for characters and `verso.traits.includes("logia")` for captains. |
| `immuneImpact` | Attack resolution, pushback step | Negates `PendingAttack.pushback`. |
| `startTurnBuffAlly{stat,amount}` | Start of owner's turn | Picks the **other** own board character with the highest effective ATK (strict `>`, first in ALL_SLOTS order wins ties); pushes `{id: vantardise_<tid>_<n>, stat, amount, source: passive_<srcId>, duration:"turn"}`, **then pushes one log entry** `{turn: turnNumber, player: playerId, message: "<srcName> : Vantardise — un allié gagne +<amount> <STAT> ce tour."}` (`<srcName>` = `sourceDef.name`; `<STAT>` = `effect.stat` **upper-cased**, i.e. `ATK`/`DEF`; the dash is an em dash `—`). If there is **no** other ally the state is returned unchanged — no modifier **and no log**. Byte-for-byte parity of this string is required (§8.60). |
| `noDodge` | Declaration (base + special) | Sets `PendingAttack.cannotBeDodged = true`. |
| `blockDamageReduction{amount}` | Shield block | **UNREACHABLE** — 0 occurrences in the shipped catalogue, so the shield-block `blockRed` term of §4.6 is **always 0**. `combat.ts:622` comments `// Some blockers reduce the damage further (Sentomaru, Garp).`, but both named cards carry only `naturalHaki`: `marines.ts:43` **Garde du Corps** → `effects: [{ type: "naturalHaki", hakiType: "armament" }]` and `marines.ts:60` **Poing de Garp** → the same. Semantics if reached: summed and subtracted in the shield-block damage recomputation. |
| `stripStealthOnAttack` | Declaration (base + special) | ORs `PendingAttack.stripStealth`. |
| `debuffAdjacentEnemies{amount}` | Aura, recomputed by `applyEnemyDebuffAuras` | **Approximation:** −amount ATK to the *entire enemy front row* V1–V3 (not true adjacency), summed across all sources on that side (board characters + active captain face). `id: debuffAura_adj_<id>`, `source: "debuffAura"`, duration `permanent`. |
| `debuffOneEnemy{amount}` | Aura | −amount ATK to the enemy board character with the highest **printed** `CardDef.atk` (strict `>`, first in ALL_SLOTS order). `max()` across sources (not summed). `id: debuffAura_one_<id>`, `source:"debuffAura"`. |
| `banishOnKO` | `applyOnKOEffects` step C | Only when the **killer's captain is flipped** and its active (verso) passive has this effect: scan the victim's graveyard from the **end** backwards for the first entry with `defId == koDefId`, set `zone = banished`, splice it out. Logs `"<name> est banni (Justice Implacable) !"` under the killer, **even if nothing was found**. |
| `explodeOnKO{amount}` | `applyOnKOEffects` step D | Sum of all `explodeOnKO` amounts on the KO'd card's def. Target = the board character of `killerPlayerId` with the **lowest currentPv** (strict `<`, first in ALL_SLOTS order). Direct `currentPv -= amount` (ignores DEF/Logia/shields). Log `"Corps Explosif : <n> dégâts à <name> !"` under `koPlayerId`. If the victim drops to ≤ 0: log `"<name> est KO !"` under its owner and `removeFromBoard` — **no** KO bonus, **no** recursive on-KO, **no** `charKOedThisGame`. |
| `copyAtkOnDeploy` | On deploy (before passive recalc) | `bestAtk = max(def.atk ?? 0, max effective ATK of own OTHER board characters)`; `delta = bestAtk − (def.atk ?? 0)`; if `delta > 0` push `{id: manemane_<instId>, stat:"atk", amount: delta, source: passive_<instId>, duration:"permanent"}`. Computed once, never updated. |
| `entryDiscardRandom` | On deploy | If the opponent's hand is non-empty, remove one uniformly random card to graveyard; log `"<name> : l'adversaire défausse une carte."` attributed to the **deploying** player. |
| `immuneControl` | On-hit control step; `debuffAllEnemies` immobilize; haoshoku immobilize | Blocks `immobilize` and `sleep` application (but **not** the ATK debuff of haoshoku / debuffAllEnemies). |
| `endTurnDesiccation{amount}` | `endTurn`, before phase switch | Summed over the **active** captain passive; only applied when `captain.flipped`. Picks the **first** (V1..A3 order) enemy board character whose `currentPv < printed CardDef.pv`; `currentPv -= amount`; log `"Déshydratation : <name> perd <n> PV permanent."`; if it reaches ≤ 0 → `removeFromBoard` only (no KO bonus, no on-KO effects, no KO log). |
| `twoWeaponSlots` | Equip validation | max 2 weapons (only when `threeWeaponSlots` is absent). |
| `attacksIgnoreShield` | — | **DATA-ONLY** (combat only reads `PendingAttack.ignoreShield`, which is set from `SpecialAttack.ignoreShield`). |
| `grantObservationAll` | — | **DATA-ONLY** (haki.ts does not consult it). |
| `selfBuffOnAllyKO{stat,amount,max,filter?}` | `applyOnKOEffects` step B2 | If the KO'd card's def matches the filter and the sum of existing `source == "captainSelfKO"` modifiers on the captain is `< max`, push `{id: captainSelfKO_<n>, stat, amount, source:"captainSelfKO", duration:"permanent"}`. The check is **before** adding, so the total may overshoot `max`. Log `"<capName> : +<n> ATK permanent (Mugiwara KO)."` (text hardcoded). |
| `meleeRecoil{amount}` | Attack resolution step B | Summed over the **original** target's def. Applied when the attack has no `range` AttackTrait, the attacker is not a captain (`!id.starts_with("captain_")`), the attacker is still `zone == board`, and the attacker's def traits do **not** include `range`. Applies even if the hit dealt 0 damage or was Logia-blocked. Log `"<atkName> subit <n> dégâts (Épines) !"`; on death: log `"<atkName> est KO (Épines) !"`, `grantKOBonus(attackerOwner)`, `removeFromBoard`, `applyOnKOEffects(attackerOwner, opponent(attackerOwner), atkDefId)`. |
| `costReduction{filter?,amount}` | `deployCost` (characters only) | **UNREACHABLE** — 0 occurrences in the shipped catalogue, so `deployCost`'s **entire passive loop is a no-op** and only the ship cost reduction can ever change a cost. `board.ts:183` comments `/** Effective deploy cost after costReduction passives (Sengoku) ... */`, but Sengoku's passive (`marines.ts:84`, **Stratège Suprême**) is `effects: [{ type: "buffAlly", stat: "def", amount: 1, filter: { faction: "marine" } }]`. Semantics if reached: for each own board character with this passive whose filter matches the **deploying** card's def (faction / tag / base traits), `cost -= amount`. Stacks. |
| `hakiCostReduction{amount}` | — | **DATA-ONLY** (no Haki costs Volonté in this engine). |
| `ignoreEnemyStealth` | — | **DATA-ONLY** (targeting does not consult it). |
| `custom{id}` | — | **DATA-ONLY**; no `custom` passive id is handled anywhere. |

### 3.2 EventEffect (15) — resolved by `resolveEventEffect`, all **after** the cost is paid and **while the event card is still in hand**

| Variant | Semantics |
|---|---|
| `gainWill{amount}` | `volonte += amount` written **directly** (bypasses any cap). No log. |
| `draw{amount,discard?}` | `drawCard` × amount; then discard `discard` cards from the **front** of hand (oldest first) while the hand is non-empty. Logs `"Defausse automatique de <discard> carte(s) (plus ancienne en main)."` with the requested count. |
| `healAlly{amount,allAllies?}` | **Only** the `allAllies == true` branch does anything: heal every own board card, capped at `def.pv ?? (cur+amount)` (uncapped when the def has no pv). Otherwise a no-op. No log. |
| `buffAllies{stat,amount,filter?,duration}` | Push `{id: event_<cardName>_<n>, stat, amount, source: cardName, duration}` on **every** own board card. `filter` is **IGNORED**. No log. |
| `damageEnemies{amount,target,cursedBonus?,sand?,destroyShips?}` | Iterate the enemy board; `allFront` restricts to V1/V2/V3; `allCursed` restricts to `def.traits.includes("cursed")`; `all` **and `single`** hit everything (single is not implemented). `dmg = (cursed && cursedBonus) ? cursedBonus : amount` (bonus **replaces**; `cursedBonus == 0` is falsy → amount). `sand` adds +1. Direct `currentPv -=` (ignores DEF/Logia/counters/shields). Then `destroyShips` (only when `opp.activeShip != null`) sets `cards[activeShip].zone = "graveyard"`, pushes the id onto `opp.graveyard` **and sets `opp.activeShip = null`**, with **no** shipDestroyEffect. This is the **one** place that nulls `activeShip`: it is the exception to the `removeFromBoard` convention of §4.4/§8.30 (which graveyards an active ship *without* clearing the pointer). Getting this wrong leaves the destroyed ship’s `shipPassive` buffs live in `recalculatePassiveBuffs`, its cost reduction live in `deployCost`, and `activateShip` still offered by `getValidActions`. Then `sweepKOs(opponent, self)`. |
| `dodgeAll` | **No-op** (unimplemented). |
| `rally{atk,def,heal}` | Every own board card: `+atk` (turn) `id: rally_atk_<inst>_<n>`, `+def` (turn) `id: rally_def_<inst>_<n>`, heal capped at `def.pv ?? cur` (**no heal** if the def has no pv). No noHeal/desiccation check. No log. |
| `buffSingle{stat,amount,duration,requiresOwnKO?}` | If `requiresOwnKO` and `!charKOedThisGame` → throw `"Flashback: aucun de vos personnages n'a été KO ce match"` (aborts the whole action; the generator does **not** pre-check this). Target = own board card with the highest effective ATK (strict `>`, board order). Push `{id: flashback_<n>, stat, amount, source: cardName, duration}`. No log. |
| `rushBuff{atk}` | Same strongest-ally selection; push `{id: burst_<n>, stat:"atk", amount: atk, source: cardName, duration:"turn"}` **and** set `deployedTurn = -1` (clears summoning sickness). Does not grant the `rush` trait. No log. |
| `tutor{filterTag?,maxCost?}` | First deck card (from the **top**) whose def is a `character` and matches `filterTag` (in `tags`) and `cost <= maxCost`; splice → hand. **No shuffle.** Always logs `"<cardName> : recherche un personnage."` |
| `deployTokens{tokenId,count}` | `deployToken` × count (first empty slot each time; silently skipped when the board is full). |
| `healAllBuff{heal,atk}` | For each own board card **without** a `noHeal` or `desiccation` status: heal capped at `def.pv ?? cur`, and push `{id: feast_<inst>_<n>, stat:"atk", amount: atk, source: cardName, duration:"turn"}`. Cards with those statuses get neither. No log. |
| `debuffAllEnemies{atk,immobilizeMaxDef?}` | Every enemy board card: `{id: intim_<inst>_<n>, stat:"atk", amount: -atk, source: cardName, duration:"turn"}`. If `immobilizeMaxDef` is `Some` (0 is valid) and the card has no `immuneControl` and its **effective DEF** ≤ threshold → push status `immobilize{turnsRemaining:2, damagePerTurn:0, source: cardName}` (stacks). No log. |
| `grantHakiAll{atk?}` | `players[self].hakiThisTurn = true`; if `atk` is truthy, `{id: haki_<inst>_<n>, stat:"atk", amount: atk, source: cardName, duration:"turn"}` on every own board card. Log `"<cardName> : Haki de l'Armement ce tour !"`. |
| `custom{id,description}` | `execute3` / `execute4`: among enemy board characters with `currentPv <= 3` (resp. `4`), take the one with the **highest** currentPv (first on ties), log `"<name> est exécuté !"` **under the opponent**, set `currentPv = 0`, `sweepKOs`. `coordinatedFire`: `marines` = number of own board characters tagged `"marine"`; if > 0, hit the enemy with the **lowest effective DEF** (first on ties) for `marines` damage, log `"Ordre de Tir : <n> dégâts coordonnés."`, `sweepKOs`. Any **other** id (`embargo`, `betrayal`, `noHeal2`): log `"<cardName> : <description>"` only — **not enforced**. |

### 3.3 CounterEffect (4)

| Variant | Semantics |
|---|---|
| `survive{description}` | Defender pays the counter's cost, discards it, sets `pendingAttack.survivePlayed = true`, and immediately tags the protected card with `usedOnceAbilities += "survived"`. Throws `"Survive only protects allies"` when the target is a captain, and `"This character already survived once"` if already tagged. On resolution, after subtracting damage: `if survivePlayed && currentPv <= 0 { currentPv = 1 }`. Does **not** protect against element KOs, thunder, spread hits or recoil. Log `"Joue <name> : survie a 1 PV !"`. |
| `reduceDamage{amount,captainBonus?}` | `reduction = (targetIsCaptain && captainBonus) ? captainBonus : amount` — the captain value **replaces** (does not add); `captainBonus == 0` is falsy. `pendingAttack.rawDamage = max(0, rawDamage − reduction)`. Log `"Joue <name> : reduit les degats de <n>"`. |
| `cancel{description,maxAttackerAtk?,selfCaptainDamage?,once?}` | If `maxAttackerAtk` is `Some` and `(attackPower ?? 0) > maxAttackerAtk` → throw `"Attacker is too strong for this counter"`. Pay, discard, `pendingAttack = None`, log `"<name> : attaque annulée !"`. If `selfCaptainDamage` is truthy: own captain `currentPv -= n`, log `"<name> : votre Capitaine subit <n> dégâts."`, then `checkWinCondition` → `state.winner`. `once` is **never enforced**. |
| `untargetable{description}` | Routed to the **same handler as `cancel`** (`applyCounterCancel`); `maxAttackerAtk`/`selfCaptainDamage` are absent so it is a plain cancel. |

The attacker keeps its tapped/used flags and its spent Volonté when an attack is cancelled.

### 3.4 EntryEffect (9) — fired on captain flip, after the lethal-flip check

| Variant | Semantics |
|---|---|
| `buffAllies{stat,amount,duration:"turn"}` | Every own board card gets `{id: entry_<n>, stat, amount, source: <captain defId>, duration:"turn"}` (all share one id in TS). Log `"Effet d'entree : tous allies +<n> <STAT> ce tour !"` (stat upper-cased), emitted even with an empty board. |
| `draw{amount}` | `drawCard` × amount; log `"Effet d'entree : pioche <amount> carte(s)"` (requested count). |
| `damageEnemies{amount,target:"allFront"}` | Flat `currentPv -= amount` to enemy V1/V2/V3. **No KO processing at all** (cards may stay on the board at ≤ 0 PV). Log `"Effet d'entree : <n> degats a toute la Ligne Avant ennemie !"`. `cursedBonus`/`sand` ignored on this path. |
| `damageEnemies{amount,target:"single"}` | Target = highest **effective ATK** enemy, preferring the front row if any front-row enemy exists (strict `>`, first on ties). `dmg = (cursed && cursedBonus) ? cursedBonus : amount`; `+1` if `sand`. Log `"Effet d'entree : <dmg> degats a <name> !"`. If it dies: log `"<name> est KO !"` (attributed to the **opponent**), `grantKOBonus(opponentId)` ← *the victim's owner gets the +2*, `removeFromBoard`, `applyOnKOEffects(opponentId, self, koDefId)`. If there are **no** enemy characters at all: `opponent.captain.currentPv -= amount` (cursedBonus/sand not applied, no win check), log `"Effet d'entree (Gear 2) : <amount> degats au Capitaine !"`. |
| `grantSelfRush` | `captain.deployedTurn = -1`. Log `"Effet d'entree : Gear 2 — le Capitaine peut agir immédiatement (Rush)."` |
| `haoshoku{immobilizeMaxDef,debuffAtk}` | Every enemy board card gets `{id: haoshoku_<instId>_<n>, stat:"atk", amount: -debuffAtk, source:"entry_haoshoku", duration:"turn"}` — even control-immune ones. Additionally, if not `immuneControl` and the **printed** `CardDef.def ?? 0` ≤ `immobilizeMaxDef`, push status `immobilize{2,0,"haoshoku"}` (stacks). Log `"Effet d'entree : Haoshoku Haki !"`. Does not touch `kingUsed`. |
| `debuffAllEnemies{atk}` | `{id: entrydebuff_<instId>_<n>, stat:"atk", amount:-atk, source:"entry", duration:"turn"}` on every enemy board card. **No log.** |
| `discardOpponentRandom{amount}` | Up to `amount` uniformly-random cards from the opponent's hand → graveyard. **No log.** |
| `multi{effects}` | Resolve sub-effects sequentially in array order. |
| `custom{id,description}` | Log `"Effet d'entree special : <description>"` only — no mechanical effect. |

### 3.5 StatusEffect types (11)

| Type | Created by | Ticked / consumed |
|---|---|---|
| `burn` | fire element on hit (`{2, 1}`) — characters and captains | Start of owner's turn: `currentPv -= damagePerTurn` (can kill). |
| `poison` | poison element on hit (`{-1, 1}`) — characters only | Start of owner's turn: damage, then `if currentPv < 1 { currentPv = 1 }` (per effect). `turnsRemaining == -1` = permanent. |
| `freeze` | ice element on hit (`{2, 0}`) — characters and captains | Blocks base/special/support/fruit actions and captain actions (valid-action generation). |
| `desiccation` | (never created by the engine) | Board cards: `currentPv -= damagePerTurn` at start of turn (no floor). **Captains take no desiccation damage.** Also excludes from `healAllBuff`. |
| `trap` | **UNREACHABLE** — support action whose `baseAction.description` contains `"piege"`/`"Piege"` (`{-1, 3}`), `turnManager.ts:746`: `if (ba.description?.includes("piege") \|\| ba.description?.includes("Piege")) {` — the only creation site. **No card in the shipped catalogue has such a description** (`grep -rni 'piege\|piège' src/data/cards/*.ts` → 0 hits); the sole Usopp support action (`mugiwara.ts:52`) is `baseAction: { name: "Bluff", atk: 0, isSupport: true, bluff: true, description: "Un ennemi de DEF ≤ 1 perd sa prochaine action (il a peur)." }`, which contains no `"piege"`. **Consequence:** the creation branch, the whole trap-consumption path in `declareBaseAttack`/`declareSpecialAttack` (§4.6, §8.12) and the generator's trap sub-branch (§4.9 item 5) are all dead with the shipped data and get **no** coverage from the §1.2 differential harness — implement them from this spec alone. | Consumed when the bearer **declares** a base or special attack (not fruit specials): `currentPv -= damagePerTurn`, **all** trap effects removed, log `"Piege ! <name> subit <n> degats en attaquant !"`; if lethal, log `"<name> est KO par le piege !"`, `removeFromBoard`, and the attack is aborted with **no KO bonus and no on-KO effects**. |
| `immobilize` | on-hit `{2,0}`; support immobilize `{2,0}`; `debuffAllEnemies` `{2,0}`; haoshoku `{2,0}` | Blocks support/base/special/fruit actions and captain actions. |
| `sleep` | **UNREACHABLE** — on-hit `{3,0}`, `combat.ts:743`: `draft.cards[pending.targetId].statusEffects.push({ type: "sleep", turnsRemaining: 3, damagePerTurn: 0, source: pending.attackerId });` — the only creation site. It fires only when `PendingAttack.sleep` is set, and `PendingAttack.sleep` is only ever populated from `SpecialAttack.sleep` (§4.6 step 9) or a fruit-awakening special's `sleep`; `grep -c 'sleep:' src/data/cards/*.ts` → **0** across all seven card files (`mugiwara`, `marines`, `baroque`, `redhair`, `tokens`, `captains`, `index`). So the sleep status can never be applied with the shipped catalogue — this is the global conclusion §5.1 hints at when it notes MG-016's awakening special has "no structured sleep". **Consequence:** the generator's "sleep blocks support/base/special" rule (§4.9 items 5/6/7) is dead code and gets **no** coverage from the §1.2 differential harness. | Blocks support/base/special (**not** fruit specials in the generator). |
| `loseAction` | Usopp-style `bluff` support `{2,0}` | Blocks support/base/special (not fruit specials). |
| `selfKO` | `SpecialAttack.transform` `{turns, 0}` | Skipped by the status tick; decremented in `startTurn` step 6a; at `turnsRemaining <= 1` the card is logged `"<name> retombe (fin de transformation) — KO."` and `removeFromBoard`d with **no KO bonus and no on-KO effects**. |
| `noStealth` | on-hit `stripStealth` `{2,0}` (no log) | Makes the bearer count as non-stealth for targeting. |
| `noHeal` | (never created by the engine) | Excludes from `healAllBuff`. |

Status tick (start of the **owner's** turn only, for board cards and the captain): damage is applied
**before** the decrement, so `turnsRemaining == 1` deals damage once then expires; `turnsRemaining == 0`
also deals damage once and is dropped; `-1` persists forever; any other negative is dropped.

---

## 4. Mechanics (ordered algorithms)

### 4.1 Game creation (`createGame`)

1. `initializeRegistry()` — idempotent, process-global in TS; in Rust it is the `Engine`'s registry.
   Registration order: `mugiwara, marines, baroque, redhair, tokens` then `allCaptains`
   (`[Luffy, Akainu, Crocodile, Shanks]`). Later duplicate ids overwrite earlier ones.
2. `createInitialState(p1Deck, p2Deck)`:
   * `createPlayerState("player1", …)` **first**, then `"player2"` (so player1 owns the lower
     instance-id counters).
   * For each player: `captainDef = getCaptainDef(deck.captainId)` (throws `Captain not found: <id>`).
   * For each `DeckEntry` **in array order**, `count` times: `instanceId = generateInstanceId(cardId)`
     (counter pre-incremented, so the first id ever has counter 1) — note the id is generated
     **before** `getCardDef` (throws `Card not found: <id>`), so the counter advances even on error.
     `CardInstance { instanceId, defId, owner, zone: Deck, tapped: false, currentPv: def.pv ?? 0,
     attachedObjects: [], modifiers: [], statusEffects: [], usedBaseAction: false,
     usedSpecialAttack: false, usedOnceAbilities: [] }`; `slot/deployedTurn/logiaUsedThisTurn/isAwakened`
     stay absent. Insert into the shared `cards` map, push the id.
   * `shuffle` (Fisher–Yates on a copy, `i` from `len-1` down to `1`, `j = floor(rng * (i+1))`, swap).
   * `hand = shuffled.splice(0, 6)`; each hand card's `zone = Hand`. Deck keeps the remaining 44.
   * `captain = { defId, owner, flipped:false, currentPv: recto.pv, tapped:false, modifiers:[],
     statusEffects:[], usedBaseAction:false, usedSpecialAttack:false, usedOnceAbilities:[] }`.
   * `board = {V1:null,V2:null,V3:null,A1:null,A2:null,A3:null}`, `activeShip: null`, `volonte: 0`,
     all per-turn flags `false`; the three optional flags absent.
   * `GameState { cards, players, turnNumber: 1, currentPlayer: Player1, phase: Main,
     pendingAttack: None, log: [], winner: None, firstPlayer: Player1 }`.
3. `startTurn(state)` — a fresh game has already run the full start-of-turn pipeline for player 1
   turn 1 (no draw, Volonté gained, passives recalculated).

### 4.2 Turn lifecycle

**`startTurn(state)`** on `cp = state.currentPlayer`:

1. `untapAll`: every non-null board card of `cp` → `tapped = false`; then `cp.captain.tapped = false`
   unconditionally. The opponent is untouched.
2. `resetTurnFlags` on `cp`: `usedFreeMove = hasDrawn = observationUsed = armamentUsed = false`,
   `allyKOedThisTurn = false`, `hakiThisTurn = false`. **Not** reset: `kingUsed`, `charKOedThisGame`,
   `volonte`. Every board card: `usedBaseAction = usedSpecialAttack = false`,
   `logiaUsedThisTurn = false`. Captain: `usedBaseAction = usedSpecialAttack = false`. Then a second
   pass strips all modifiers with `duration == Turn` from every board card and from the captain.
   (`nextTurn` duration and `Modifier.turnsRemaining` are **never** processed anywhere in the engine.)
3. If **not** (`turnNumber == 1 && currentPlayer == firstPlayer`) → `drawCard(cp)`.
   (Player 2 therefore *does* draw on its first turn, which is also `turnNumber == 1`.)
4. `gainVolonte`: `players[currentPlayer].volonte = min(turnNumber, VOLONTE_CAP=10)` — Volonté is
   **replaced**, not accumulated; any surplus from KO bonuses is lost.
5. `phase = Main`.
6. **Sandai Kitetsu curse**: for each non-null board card of `cp`, if any attached object has
   `defId == "MG-010"` → `currentPv -= 1` and push log `"Malédiction du Sandai Kitetsu : 1 dégât."`
   (exactly 1 damage regardless of how many copies are attached).
7. `processStartOfTurnEffects` (status tick) — see §3.5. Board cards first (all six slots), then the
   captain. Captain branch has **no** `desiccation` damage and **no** `selfKO` skip.
8. **selfKO timers** (board snapshot taken once): for each board card with a `selfKO` status,
   if `turnsRemaining <= 1` → log `"<name> retombe (fin de transformation) — KO."`, `removeFromBoard`
   (no bonus, no on-KO); else decrement its `turnsRemaining` by 1. No `zone` check on this path.
9. **KO sweep** (board snapshot): for each card still `zone == Board` with `currentPv <= 0`:
   log `"<name> est KO (brulure/effet) !"` (under `cp`), `grantKOBonus(owner)`, `removeFromBoard`,
   `applyOnKOEffects(owner, opponent(owner), defId)`.
10. `applyStartOfTurnPassives(cp)` — see §4.7.
11. `recalculatePassiveBuffs(cp)`, `recalculatePassiveBuffs(opponent(cp))`, `applyEnemyDebuffAuras()`.

No win check happens in `startTurn` (a captain killed by burn is only detected later by
`checkWinCondition`).

**`endTurn(state)`**:

1. Crocodile desiccation: `capPassive = flipped ? verso.passive : recto.passive`;
   `desicc = Σ endTurnDesiccation.amount`. **Only if `captain.flipped && desicc > 0`**: the first
   enemy board character (V1..A3) with `currentPv < printed def.pv` loses `desicc` PV, log
   `"Déshydratation : <name> perd <n> PV permanent."`; if ≤ 0 → `removeFromBoard` only.
2. `phase = End`.
3. Hand limit: if the ending player's hand is longer than `HAND_LIMIT = 10`, splice
   `len - 10` cards off the **front** (oldest) into the graveyard and push
   `"Limite de main : <n> carte(s) défaussée(s)."`
4. Switch: `nextPlayer = other(currentPlayer)`; **if `currentPlayer == Player2` then `turnNumber += 1`**;
   `currentPlayer = nextPlayer`. `endTurn` does **not** call `startTurn` — the action reducer does
   (`executeAction(EndTurn) = startTurn(endTurn(state))`).

`turnNumber` is therefore a **round** counter (increments after player 2), even though the type
comment claims an overall turn count. All turn thresholds (Haki 5/7/10, captain flip ≥ 4/5/6/7,
fruit `minTurns`, AI flip scoring) use this round counter.

**`drawCard(state, player)`**: if the deck is empty → `state.winner = opponent(player)` and return
(no draw, no log, `hasDrawn` untouched). Otherwise pop index 0 → hand, `zone = Hand`,
`hasDrawn = true`. No hand-limit check, no log.

**`checkWinCondition(state) -> Option<PlayerId>`** (pure, does not write `winner`):
`state.winner` if set; else if **both** captains are at ≤ 0 PV → `opponent(currentPlayer)`
(the active player loses simultaneous death); else `player2` if p1's captain ≤ 0; `player1` if
p2's captain ≤ 0; else `None`.

**`addLog(state, player, msg)`** pushes `{turn: state.turnNumber, player, message}`.

### 4.3 Board queries and effective stats

* `getBoardCharacters(state, p)` — iterate `ALL_SLOTS`, push `cards[id]` when the slot is non-null and
  the instance exists (dangling ids silently skipped). Ships and the captain are never included.
* `getEmptySlots(state, p)` — `ALL_SLOTS` filtered on `board[s] == null`.
* `getSlotOf(state, id)` — `None` unless `zone == Board`; then `card.slot`.
* `getAdjacentSlots(slot)` — the `ADJACENCY` table (order matters).
* `hasFrontRow(state, p)` — `true` if any of V1/V2/V3 is non-null, **or** the captain is flipped, has a
  slot, and that slot is a front slot.
* **`getEffectiveAtk(state, id)`**: `0` if missing; else
  `atk = def.atk ?? 0`; `+= Σ getCardDef(obj).bonusAtk ?? 0` over `attachedObjects` (in array order,
  missing instances skipped); `+= Σ mod.amount` for every modifier with `stat == Atk`
  (**duration and turnsRemaining are ignored**); finally `max(0, atk)`.
  Captains are not in `cards` → this returns 0 for them.
* **`getEffectiveDef`** — identical with `def.def`, `bonusDef`, `stat == Def`. Piercing is **not**
  applied here.
* **`hasTrait(state, id, t)`**: `def.traits.contains(t)` → true; else for each attached object,
  `fruitEffects.base.grantsTraits.contains(t)` → true, or (`objCard.isAwakened == true` and
  `fruitEffects.awakening.grantsTraits.contains(t)`) → true. **`CardDef.grantsTraits` on non-fruit
  equipment is never consulted.**
* **`hasSummoningSickness`**: `card.deployedTurn == state.turnNumber` → `!hasTrait(rush)`;
  otherwise `false` (absent `deployedTurn` is never sick).

**Exported TS symbols intentionally dropped from the port.** These **seven** are exported by the engine
but have **no live caller anywhere in `src/`** (dead at the time of the port): five have zero references
of any kind, and `getSlotOf` is *imported but never referenced in the importing file's body*. They are
recorded here so a porter does not rediscover them and assume an omission; none of them affects
behaviour, and **none of them carries parity signal in the §1.2 differential harness**:

| TS export | Source | Disposition in the port |
|---|---|---|
| `getCharacterInSlot(state, p, slot)` | `src/engine/board.ts:35` | Drop. Trivially a `board[slot]` lookup followed by `state.cards.get(id)`; callers use `getBoardCharacters` / direct board indexing instead. |
| `getAllCardDefs() -> Record<id, CardDef>` | `src/engine/cardRegistry.ts:31` | Keep **only** as the WASM catalogue dump of §1.3; not part of the engine's Rust API. |
| `getAllCaptainDefs() -> Record<id, CaptainDef>` | `src/engine/cardRegistry.ts:35` | Same. |
| `resetInstanceCounter()` | `src/engine/utils.ts:18` | Drop. The port's id generator is an explicit `IdGen` value (§1.4), so "reset" is just constructing a fresh one — there is no global counter to clear. |
| `deepClone<T>(obj)` (`JSON.parse(JSON.stringify(x))`) | `src/engine/utils.ts:23` | Drop. Replaced by `#[derive(Clone)]`; note the TS version would silently drop `undefined` fields and is *not* equivalent to `Clone`. |
| `getFruitTraits(state, instId)` | `src/engine/fruits.ts:164` | **Drop — dead.** `grep -rn getFruitTraits src/` returns exactly **one** hit: that definition line. Nothing calls it; in particular `hasTrait` (above) re-derives fruit-granted traits inline, and neither `hasConquerorInPlay` nor `matchesFilter` consults it. Its semantics are still transcribed in §4.8 for completeness, but implementing it buys **no** parity coverage — do not write differential tests against it. |
| `getSlotOf(state, id)` | `src/engine/board.ts:55` | **Keep only if convenient — effectively dead.** Its single import site, `src/engine/passives.ts:4` (`import { getBoardCharacters, getAdjacentSlots, getSlotOf } from "./board";`), never references it in the file body; `passives.ts` uses the `slot` carried on its own `boardChars` entries instead. The function is still documented as a board query above because it is trivial and harmless, but no caller exercises it. |

**`deployCost(state, p, def)`** (characters only; ships/objects/events pay raw `def.cost`):
```
cost = def.cost
matches(f) = f.is_none() || (faction ok && tag ok (def.tags) && trait ok (def.traits BASE only))
for slot in ALL_SLOTS:                       // panics/throws on a dangling board id (no null check)
    for e in getCardDef(board[slot]).passive.effects where e is costReduction && matches(e.filter):
        cost -= e.amount
if activeShip:
    sp = shipPassive.to_lowercase()
    if (sp.contains("cout") || sp.contains("coût")) && sp.contains("-1"):
        factionOk = (sp.contains("marine") && def.faction == Marine)
                 || (sp.contains("mugiwara") && def.tags.contains("mugiwara"))
                 || (!sp.contains("marine") && !sp.contains("mugiwara"))
        if factionOk { cost -= 1 }           // always exactly 1
return max(1, cost)                          // a cost-0 card still costs 1
```

### 4.4 Board mutations

**`deployCharacter(state, p, instanceId, slot)`** — validation in order, each an error:
`Card not found: <id>` / `"Not your card"` / `"Card not in hand"` / `"Not a character card"` /
`Slot <slot> is occupied` / `Cannot afford <name> (cost <n>)`.
Then: `spendVolonte(cost)`; hand→board (`zone = Board`, `slot`, `deployedTurn = turnNumber`,
`currentPv = def.pv ?? 0`; **tapped, modifiers, statusEffects, used\* flags are NOT reset**);
then the at-deploy **ship buff**: if `shipPassive.to_lowercase()` contains `"deploiement"`/`"déploiement"`
and the faction gate passes (same three-branch rule as `deployCost` but with mugiwara checked first),
apply `/\+(\d+)\s*pv/` → `currentPv += n` **and** a `{id: shipdep_pv_<inst>, stat: Pv, amount: n,
source: ship_<shipDefId>, duration: Permanent}` modifier, and `/\+(\d+)\s*def/` →
`{id: shipdep_def_<inst>, stat: Def, …}`. (There is no `+N atk` parsing.)
Then log `"Deploie <name> en <slot>"`; then `copyAtkOnDeploy`; then `entryDiscardRandom`;
then `recalculatePassiveBuffs(p)` and `applyEnemyDebuffAuras()`.

**`equipObject(state, p, objId, targetId)`** — errors: `Object not found: <id>` / `"Not your card"` /
`"Object not in hand"` / `"Not an object card"` / `Target not found: <id>` / `"Not your character"` /
`"Target on board"`→`"Target not on board"`. The target is **not** checked to be a character, and
`CardDef.restriction` is **never enforced**.
Cost: `effectiveCost = objDef.cost`, except `objDef.id == "MG-012"` (Clima-Tact) and the board
contains both `MG-003` and `MG-004` → `0`. `Cannot afford <name> (cost <n>)`.
Slot caps (only when `objDef.subtype` is `Some`): `maxSlots = 1`; weapons → 3 with `threeWeaponSlots`,
else 2 with `twoWeaponSlots`; accessories → 2 with `twoAccessorySlots`; fruits always 1. Error:
`<targetName> already has max <subtype> equipped`.
Then: `spendVolonte` (no-op at 0); hand → `zone = Board`, `obj.slot = target.slot`,
`target.attachedObjects.push(objId)`.
**Wielder bonus table** (matched on `targetDef.name.contains(wielder)` — substring):
`MG-009`→(“Zoro”, DEF +1), `MR-013`→(“Tashigi”, ATK +1), `RH-011`→(“Ben Beckman”, ATK +1),
`RH-013`→(“Yasopp”, ATK +1); modifier `{id: wield_<objInst>, source: equip_<objDefId>, duration: Permanent}`
(never removed by anything).
Then log `"Equipe <objName> sur <targetName>"`; if `subtype == Fruit && fruitEffects` →
`applyFruitBaseEffects`; then `recalculatePassiveBuffs(p)` (**no** `applyEnemyDebuffAuras`).

**`deployShip(state, p, instanceId)`** — errors `Card not found` / `"Not your card"` /
`"Card not in hand"` / `"Not a ship card"` / `Cannot afford <name> (cost <n>)` (raw cost).
`spendVolonte(def.cost)`. If an `activeShip` exists and its instance exists: run its
`shipDestroyEffect` — (i) `healAll`: every own board card `currentPv = min(cur + healAll, def.pv ?? cur)`
(**caps at printed PV, so a card above printed PV is lowered**); (ii) `draw`: up to N from the top —
a **raw `deck.shift()` loop, NOT `drawCard`** (`board.ts:476-481`:
`if (de.draw && p.deck.length > 0) { for (let i = 0; i < de.draw && p.deck.length > 0; i++) { const id = p.deck.shift()!; draft.cards[id].zone = "hand"; p.hand.push(id); } }`).
Unlike §3.2 (EventEffect `draw`) and §3.4 (EntryEffect `draw`), which both say "`drawCard` × amount",
this loop **(a) does NOT set `state.winner = opponent` when the deck runs dry** — it just stops early —
and **(b) does NOT set `player.hasDrawn`** (contrast `gameState.ts:356-364`). **Do not reuse
`draw_card()` here for tidiness:** replacing a Going Merry (`MG-020`, `{healAll: 2, draw: 1}`) or a
Navire du Nouveau Monde (`RH-018`, `{draw: 1}`) while the deck is empty must **not** end the game and
must **not** mark the player as having drawn;
(iii) `deployToken`: first empty slot in ALL_SLOTS order, create a full CardInstance
(`currentPv = tokenDef.pv ?? 1`, `deployedTurn = turnNumber`, `zone = Board`), no log of its own;
(`buffAtk`/`buffDef` are **ignored**); then log `"<oldShipName> : effet de destruction."`.
Then the old ship → graveyard (`zone = Graveyard`, pushed). Then hand → `activeShip = instanceId`,
`zone = Board`, **no slot**. Log `"Deploie navire <name>"`. **No passive recalculation.**

**`moveCharacter(state, p, id, targetSlot)`** — errors `"Free move already used this turn"` /
`"Card not on board"` / `"Not your card"` / `"Card has no slot"` /
`<target> is not adjacent to <current>` / `Slot <target> is occupied`. Then swap the board entries,
`card.slot = targetSlot`, `usedFreeMove = true`. **No log, no passive recalc, no status/tapped checks,
and attached objects keep their stale `slot`.**

**`removeFromBoard(state, instanceId)`** — no-op if the card is missing or `zone != Board`.
Clears `board[slot]` for `card.owner`. **Vivre Card**: if any attached object has `defId == "MG-019"`,
find the first deck card (from the top) whose def is a character, has tag `"mugiwara"` and
`cost <= 3`; splice it into hand and log `"Vivre Card : <name> rejoint la main."`.
Then every attached object → graveyard (in array order; their `slot` is **not** cleared),
`attachedObjects = []`; then the character itself → `zone = Graveyard`, `slot = None`, pushed to
graveyard. **Graveyard order: objects first, then the character.** `modifiers`, `statusEffects`,
`currentPv`, `tapped`, `deployedTurn` and the used-flags are **not** reset. No KO side effects, no log.
Passing an active ship graveyards it but does **not** clear `player.activeShip`.

### 4.5 Targeting (`getValidTargets(state, attackerId, forSpecial)`)

```
attacker missing || attacker.slot missing            -> { [], canTargetCaptain: false }
hasRange = attackerDef.traits.contains(range)
        || (forSpecial      && attackerDef.specialAttack.attackTraits.contains(range))
        || (!forSpecial     && attackerDef.baseAction.attackTraits.contains(range))
        // NOTE: reads the DEF directly — fruit/equipment-granted range is IGNORED here
if isBackSlot(attackerSlot) && !hasRange           -> { [], false }
opponentHasFront = hasFrontRow(opponent)            // includes a flipped captain in a front slot
targetable = getBoardCharacters(opponent)
if opponentHasFront && !hasRange { keep only front-row targets }
isStealthed(c) = hasTrait(c, stealth) && !c.statusEffects.any(noStealth)   // uses hasTrait -> fruits count
if targetable.any(!isStealthed) { keep only !isStealthed }                 // evaluated on the row-filtered set
canTargetCaptain =
   if opponent.captain.flipped && opponent.captain.slot.is_some() {
       (!opponentHasFront || hasRange) || isFrontSlot(captain.slot)
   } else {
       getBoardCharacters(opponent).is_empty()      // no enemy characters ANYWHERE (front or back)
   }
```
No check of the attacker's tapped/sickness/status here (the generator does that).

### 4.6 Attack pipeline

#### Step 1 — declaration

**`declareBaseAttack(attackerId, targetId, targetIsCaptain)`**
1. `"Attacker not found"` / `"Attacker is tapped"` / `"Base action already used"` /
   `"Character has summoning sickness"` / `"Character has 0 ATK — cannot attack"` (uses
   `getEffectiveAtk <= 0`; em dash).
2. **Trap trigger** (see §3.5) — may abort the whole action.
3. `atk = getEffectiveAtk(attacker)` (recomputed after the trap);
   `attackTraits = baseAction.attackTraits ?? []`.
4. `attackElement = baseAction.element`, then for each attached object **in order**, if its def has
   `grantsElement` → overwrite (**last one wins**).
5. Target DEF: captain → `(flipped ? verso.def : recto.def) + Σ captain.modifiers[stat==Def].amount`;
   character → `getEffectiveDef(targetId)`.
6. Piercing: `attackTraits.contains(Piercing) || attackerDef.traits.contains(Piercing)` →
   `def = floor(def / 2)`.
7. `rawDamage = max(0, atk - def)`.
8. `hasHaki = attackerDef.naturalHaki.map(non-empty) || turnNumber >= 7 || attackElement == Water
   || players[owner].hakiThisTurn`.
9. `pendingAttack = { attackerId, targetId, targetIsCaptain, isSpecial: false, rawDamage,
   attackPower: atk, element: attackElement, attackTraits, hasHaki,
   cannotBeDodged: attackerNoDodge(attacker), immobilize: baseAction.immobilize,
   stripStealth: baseAction.stripStealth || attackerStripsStealth(attacker) }`.
   `ignoreShield`, `sleep`, `pushback`, `pushbackSlots` stay `None`. **`baseAction.cannotBeDodged`
   is never read.**
10. `tapped = true; usedBaseAction = true; usedSpecialAttack = true` (one action per turn).
11. Log `"<name> attaque <targetName> (ATK <atk> vs DEF <postPiercingDef> = <raw> degats)"`,
    `targetName = "Capitaine"` when targeting a captain.

`attackerNoDodge(id)` = the attacker's def passive contains `noDodge`, **or** any attached object has
`defId == "MG-013"` (Kabuto). `attackerStripsStealth(id)` = def passive contains `stripStealthOnAttack`.

**`declareSpecialAttack`**
1. `"Attacker not found"`; `"Special already used"` (**`tapped` and `usedBaseAction` are NOT checked**);
   `"Character has summoning sickness"`.
2. Trap trigger (fires **before** the remaining validation — a later throw silently discards it).
3. `"Character has no special attack"`; `Already used this ability (1x/game)`;
   `Cannot afford special (cost <n>)`.
4. **Transform branch** (`spec.transform`): `spendVolonte(cost)`; `usedBaseAction = usedSpecialAttack =
   tapped = true`; if `oncePerGame` push `spec.name`; push
   `{id: transform_<specName>_<n>, stat: Atk, amount: max(0, t.atk - currentEffectiveAtk),
   source: "transform", duration: Permanent}`; push status `selfKO{turnsRemaining: t.turns,
   damagePerTurn: 0, source: spec.name}`; log
   `"<name> : <specName> ! ATK <t.atk> pendant <t.turns> tours, puis KO."`. **No pendingAttack, no
   Rush granted, `t.def` and `t.pv` ignored, ATK is never lowered.**
5. Otherwise: `spendVolonte(cost)`;
   `totalAtk = getEffectiveAtk(attacker, pre-spend state) + spec.atkBonus + conditionalAtkBonus`.
   `conditionalAtkBonus`: `None` → 0. If the target is a captain: only `vsFaction` is checked against
   the captain def's faction (**`vsTrait` is ignored for captains**). Otherwise `vsFaction` against the
   target def's faction, else `vsTrait` via `hasTrait`. Returned **at most once**.
6. `attackTraits = spec.attackTraits ?? []`, plus `Zone` if `spec.twoTargets` and Zone is not already
   present.
7. Target DEF as above; piercing halving (`attackTraits` **or** attacker `def.traits`), **then**
   `if spec.ignoreDef { def = max(0, def - ignoreDef) }` — piercing first, ignoreDef second.
8. `rawDamage = max(0, totalAtk - def)`;
   `hasHaki = naturalHaki non-empty || turnNumber >= 7 || spec.element == Water || hakiThisTurn`
   (equipment `grantsElement` does **not** apply to specials).
9. `pendingAttack = { …, isSpecial: true, attackPower: totalAtk, element: spec.element, attackTraits,
   hasHaki, cannotBeDodged: spec.cannotBeDodged || attackerNoDodge, ignoreShield: spec.ignoreShield,
   // UNREACHABLE left disjunct: `spec.cannotBeDodged` is never populated — see the note below.
   immobilize: spec.immobilize, sleep: spec.sleep,
   pushback: spec.pushback || (spec.pushbackSlots ?? 0) > 0, stripStealth: spec.stripStealth ||
   attackerStripsStealth }` (`pushbackSlots` itself is **not** copied).
10. `tapped = usedSpecialAttack = usedBaseAction = true`; `oncePerGame` → push `spec.name`.
11. Log `"<name> utilise <specName> sur <targetName> (ATK <total> vs DEF <def> = <raw> degats)"`.

**UNREACHABLE — `SpecialAttack.cannotBeDodged`.** `combat.ts:322` reads
`cannotBeDodged: spec.cannotBeDodged || attackerNoDodge(state, attackerInstanceId)`, but
`grep -c cannotBeDodged src/data/cards/*.ts` is **0 across all seven card files** (`mugiwara`,
`marines`, `baroque`, `redhair`, `tokens`, `captains`, `index`) — the field is declared on
`SpecialAttack` (§2.2) and never set by any shipped card. So with the shipped catalogue the left
disjunct is always `undefined` and **`PendingAttack.cannotBeDodged` on a special is always exactly
`attackerNoDodge(...)`** — i.e. the attacker's `noDodge` passive or an attached MG-013 (Kabuto), the
same source a *base* attack uses. Consequence: the disjunction carries **zero parity signal** in the
§1.2 differential harness (no shipped state distinguishes it from plain `attackerNoDodge`), and the
observation-Haki refusal `"This attack cannot be dodged"` (§4.6 step 2) can only ever be triggered
via `attackerNoDodge` on a special, never by card data. Implement the disjunction from this spec
alone — it must still be reproduced, because a *future* card populating the field would change
behaviour. Contrast **`BaseAction.cannotBeDodged`**, which is a different failure: it is never
*read* at all (§4.6 step 9, §8.38), so populating it would change nothing.

**`declareFruitSpecialAttack(attackerId, fruitId, targetId, targetIsCaptain)`**
Only two guards: `"Attacker not found"` and `"Character has summoning sickness"` — **no tapped /
usedBaseAction / usedSpecialAttack check and no trap trigger.** Then `"Fruit not awakened"`
(`!fruitCard.isAwakened`; the fruit is **not** verified to be attached to the attacker),
`"Fruit has no awakening special attack"`, `Already used this fruit ability (1x/game)` (tracked on the
**attacker's** `usedOnceAbilities` by `spec.name`), `Cannot afford fruit special (cost <n>)`.
`totalAtk = effectiveAtk + spec.atkBonus` (no conditional bonus). Piercing halving uses **only**
`attackTraits` (the attacker's `def.traits` piercing is ignored), then `ignoreDef`.
`hasHaki = naturalHaki non-empty || turnNumber >= 7 || spec.element == Water` (**`hakiThisTurn` is not
consulted**). `cannotBeDodged` is **not set**; `attackerNoDodge`/`attackerStripsStealth` are not used.
Taps and sets both used-flags; same log format as a special.

#### Damage formula (consolidated)

```
attackPower = effectiveAtk(attacker)                                   (base)
            = effectiveAtk + spec.atkBonus + conditionalBonus          (special)
            = effectiveAtk + spec.atkBonus                             (fruit special)
            = verso.atk + Σ captain.modifiers[Atk]                     (captain base attack)
targetDef   = captain ? (flipped?verso.def:recto.def) + Σ captain.modifiers[Def]
                      : getEffectiveDef(target)
A) piercing  -> targetDef = floor(targetDef / 2)      // attackTraits, and for base/special also def.traits
B) ignoreDef -> targetDef = max(0, targetDef - n)     // special & fruit special only
C) rawDamage = max(0, attackPower - targetDef)
counter reduceDamage: rawDamage = max(0, rawDamage - reduction)
shield block:        rawDamage = max(0, attackPower - blockerDef' - Σ blockDamageReduction)
                     // recomputed FROM attackPower: discards prior reductions AND ignoreDef;
                     // blockerDef' halved only if pending.attackTraits contains piercing
resolution: currentPv -= rawDamage                     (no further DEF)
spread hit: dmg = max(0, attackPower - effectiveDef(secondary) [halved if attackTraits piercing])
            // counters and ignoreDef are NOT applied to spread hits
water vs cursed: apply the same damage a SECOND time
thunder:    max(1, floor(rawDamage / 2)) to the first occupied adjacent enemy slot, ignoring DEF
```
`floor` on a negative DEF rounds toward −∞ (increasing damage); the engine does not clamp DEF at 0.

#### Step 2 — the counter window

`getEligibleCounters(state, p)` = `[]` if no pending attack; else the player's hand filtered to
`def.type == Counter && canAfford(def.cost)`, and for `cancel` counters with `maxAttackerAtk`,
`(attackPower ?? 0) <= maxAttackerAtk`.

* `applyCounterReduce` / `applyCounterCancel` / `applyCounterSurvive` — see §3.3. Errors:
  `"No pending attack"`, `"Counter not found"`, `"Not a counter card"`,
  `"Not a damage reduction counter"` / `"Not a cancel counter"` / `"Not a survive counter"`,
  `"Cannot afford counter"`, `"Attacker is too strong for this counter"`,
  `"Survive only protects allies"`, `"This character already survived once"`.
  None of them verifies that the counter is in the owner's hand or that the owner is the defender.
* **`applyShieldBlock(blockerId)`**: `"No pending attack"` / `"This attack ignores Bouclier"`
  (`pending.ignoreShield`) / `"Blocker not found"` / `"A tapped character cannot use Bouclier"` /
  `"Not a Bouclier character"` (`hasTrait(shield)`). Then
  `blockerDef = getEffectiveDef(blocker)`, halved if `pending.attackTraits` contains `piercing`;
  `blockRed = Σ blockDamageReduction on the blocker's def passive`;
  `atkPower = pending.attackPower ?? pending.rawDamage`;
  `rawDamage = max(0, atkPower - blockerDef - blockRed)`; blocker `tapped = true`;
  `pending.targetId = blockerId`, `pending.targetIsCaptain = false`.
  Log `"🛡 <blockerName> bloque l'attaque ! (Bouclier)"`. Costs 0 Volonté. **No adjacency, ownership,
  zone or self-target validation.** A captain-targeted attack can be redirected onto a Shield character.
* **Observation Haki dodge** (`useObservationHaki`, haki.rs): requires `turnNumber >= 5` and
  `!observationUsed`; errors `"Observation Haki not available"`, `"No pending attack to dodge"`,
  `"This attack cannot be dodged"` (`pending.cannotBeDodged`). Sets `observationUsed = true`,
  `pendingAttack = None`, logs `"Haki de l'Observation ! Attaque esquivee !"`. It does **not** verify
  that the player is the defender.

#### Step 3 — `resolveAttack(state)` (errors `"No pending attack"`)

A. **Primary damage** — `applyCaptainDamage` or `applyCharacterDamage`.
   * *Captain*: if the captain is **flipped** and `verso.traits` contains `logia` and `!hasHaki`
     and `rawDamage > 0` → log `"⚠ <capName> : INTANGIBILITE LOGIA ! L'attaque passe a travers.
     Utilisez le Haki (T7+) ou l'Eau pour le toucher."` and return (captain Logia is **unlimited**,
     recto captains are never Logia, top-level `CaptainDef.traits` is never consulted).
     Else `currentPv -= rawDamage`; `survivePlayed && currentPv <= 0` → `currentPv = 1`;
     log `"Capitaine <name> subit <n> degats (PV: <pv>)"` under the attacker's owner.
   * *Character*: if `hasTrait(logia) && !hasHaki && rawDamage > 0`: if `!logiaUsedThisTurn` →
     set it and log `"⚠ <name> : INTANGIBILITE LOGIA ! Utilisez le Haki (T7+) ou l'Eau."`, return;
     otherwise fall through and take damage (Logia negates **one hit per turn**).
     `currentPv -= rawDamage`; `survivePlayed && <=0` → 1; log
     `"<name> subit <n> degats (PV: <pv>)"`. If `currentPv <= 0`: **Chapeau de Paille** — if an
     attached object has `defId == "RH-014"` and `"strawhat"` is not in `usedOnceAbilities`, set
     `currentPv = 1`, push `"strawhat"`, log `"<name> survit grâce au Chapeau de Paille (1 PV) !"`
     and return. Otherwise log `"<name> est KO !"`, `grantKOBonus(target.owner)` (**the loser gets
     +2 Volonté**), `removeFromBoard`, `applyOnKOEffects(koOwner, attackerOwner, koDefId)`.
B. **Melee recoil (Épines)** — only when `!targetIsCaptain` and `!attackTraits.contains(range)`;
   the recoil is read from the **original pre-resolution** target's def. See §3.1 `meleeRecoil`.
C. **Element effects** (`applyElementEffects`) — no-op without an element; captain targets route to
   `applyElementToCaptain`; returns immediately if the primary target is no longer `zone == Board`
   (so thunder does **not** propagate when the primary hit killed its target).
   * `fire` → push `burn{2, 1, attackerId}`.
   * `ice` → push `freeze{2, 0}`.
   * `thunder` → for the target's adjacent slots in `ADJACENCY` order, the **first occupied** enemy slot
     takes `max(1, floor(rawDamage/2))` directly (ignores DEF/Logia/shield), then `break`. No log.
   * `poison` → push `poison{-1, 1}`.
   * `sand` → `currentPv -= 1`. No log.
   * `water` → if `hasTrait(target, cursed)` and it is still on the board, `currentPv -= rawDamage`
     a second time (×2). No log. This bypasses the Logia check.
   * Captain version implements **only** `fire` (burn) and `ice` (freeze); thunder/poison/sand/water on
     a captain do nothing.
D. **Element-caused KO of the primary target**: if it is still on the board at `currentPv <= 0` →
   log `"<name> est KO (effet elementaire) !"`, `grantKOBonus(owner)`, `removeFromBoard`,
   `applyOnKOEffects`. Straw hat and survive do **not** protect here.
E. **Thunder adjacent KO**: for `element == thunder` and a character target, walk the original target's
   adjacent slots in order; at the **first occupied** slot (regardless of KO) check `currentPv <= 0` →
   log `"<name> est KO (foudre) !"`, KO bonus, remove, on-KO; then `break`.
F. **On-hit control** on a surviving primary target (`ctrlImmune` = target def passive has
   `immuneControl`):
   * `immobilize && !ctrlImmune` → status `immobilize{2, 0, attackerId}`, log `"<name> est immobilisé !"`.
   * `sleep && !ctrlImmune` → status `sleep{3, 0}`, log `"<name> est endormi !"`.
   * `stripStealth` → status `noStealth{2, 0}`, **no log**.
   * `pushback && !immuneImpact` → map `V1→A1, V2→A2, V3→A3`; only if the destination exists and is
     empty: move the card, log `"<name> est repoussé en <dest> (Impact) !"`. Back-row targets and
     occupied destinations do nothing, silently. **Only one slot ever; `pushbackSlots` is unused.**
   These all fire even if Logia blocked the damage or `rawDamage` was 0.
G. **Zone / Total spread**: only for character targets when `attackTraits` contains `zone` or `total`.
   `total` → every other enemy board character in `ALL_SLOTS` order; else `zone` → the enemy cards in
   the **original** target slot's adjacent slots (excluding the primary target). `total` wins if both
   are present. Then `applySpreadHit` for each in order:
   * skip if missing / not on board / `currentPv <= 0`;
   * skip silently if `hasTrait(logia) && !hasHaki` (does **not** consume `logiaUsedThisTurn`);
   * `dmg = max(0, (attackPower ?? rawDamage) - effectiveDef [halved if attackTraits piercing])`
     — **counters and ignoreDef do not apply**;
   * `currentPv -= dmg`; log `"<name> subit <n> degats (Zone/Total) (PV: <pv>)"` under the attacker;
   * `applyElementEffects` with `element = None` when the element was thunder (no chaining), otherwise
     the same element and `rawDamage = dmg`;
   * if `currentPv <= 0`: log `"<name> est KO !"`, KO bonus, remove, on-KO (no straw hat, no survive).
H. `pendingAttack = None`.
I. `checkWinCondition` → write `state.winner` if it returns a player.

`getAttackerOwner(id)` = `id.strip_prefix("captain_")` parsed as a PlayerId, else `cards[id].owner`
(throws `Attacker not found: <id>`).

**Common KO order** for every non-trap character KO:
`addLog(under the KO'd card's owner) → grantKOBonus(koOwner) → removeFromBoard → applyOnKOEffects`.

### 4.7 Passives, auras and the on-KO hook

**`applyStartOfTurnPassives(state, p)`**: iterate `ALL_SLOTS` (board membership read from the
**original** state), for each card with a passive, resolve each effect in definition order.
Only `healAdjacent` and `startTurnBuffAlly` have implementations; every other type is a no-op.
Captain start-of-turn passives are a **stub** (nothing happens).

**`recalculatePassiveBuffs(state, p)`** — a single pass over player `p` only:
1. **Strip**: from every board card, remove modifiers whose `source` starts with `"passive_"`,
   `"captain_"` or `"synergy_"`. The captain's own modifiers are never stripped.
2. `boardChars` = the board in `ALL_SLOTS` order.
3. **Captain buffAlly**: `capPassive = flipped ? verso.passive : recto.passive`; for each `buffAlly`,
   for each board char matching the filter → `{id: captain_<stat>_<charId>, source: captain_<capDefId>,
   duration: Permanent}`.
4. **Character buffAlly**: for each board char with a `buffAlly`, for every **other** board char
   matching the filter → `{id: passive_<srcId>_<stat>_<targetId>, source: passive_<srcId>,
   duration: Permanent}`.
5. **Ship passive**: `desc = shipPassive.to_lowercase()`; `atk = /\+(\d+)\s*atk/i`,
   `def = /\+(\d+)\s*def/i` (a `pv` regex is computed and **unused**). For each board char,
   `factionMatch = (desc.contains("mugiwara") && tags.contains("mugiwara"))
   || (desc.contains("marine") && tags.contains("marine"))
   || (!desc.contains("mugiwara") && !desc.contains("marine"))`; push
   `{id: ship_passive_atk_<charId> / ship_passive_def_<charId>, source: passive_ship_<shipDefId>,
   duration: Permanent}` when the parsed bonus is `> 0`. **Note this ship rule matches the `marine`
   tag, whereas `deployCost` matches the `marine` faction.**
6. **Synergies**: for each board char with `synergies`, for each entry whose `partnerId` equals the
   `defId` of any board char (including the captain id is *not* possible — only `cards` are scanned),
   push `{id: synergy_<charId>_<partnerId>, stat: Atk, amount: syn.atkBonus,
   source: synergy_<partnerId>, duration: Permanent}`.
   *(Consequence: `RH-004`'s `CAP-SHANKS` synergy never triggers, because captains are not board cards.)*
No logs.

**`applyEnemyDebuffAuras(state)`** — operates on both players:
1. Strip every modifier with `source == "debuffAura"` from both boards.
2. For `pid` in `[player1, player2]` (in that order), with `opp` the other side: collect the passives of
   every board card **plus** the active captain face; `adj = Σ debuffAdjacentEnemies.amount`,
   `one = max(debuffOneEnemy.amount)`. If `adj > 0`, push `{id: debuffAura_adj_<id>, amount: -adj}` on
   each enemy card in V1/V2/V3. If `one > 0`, push `{id: debuffAura_one_<id>, amount: -one}` on the
   enemy card with the highest **printed** `def.atk` (strict `>`, `ALL_SLOTS` order). Enemy captains are
   never debuffed. Both modifiers can land on the same card.

**`applyOnKOEffects(state, koPlayerId, killerPlayerId, koDefId)`** — the caller is expected to have
already removed the card (and, for banish, to have placed it in the graveyard):
A. `players[koPlayerId].charKOedThisGame = true` (**`allyKOedThisTurn` is set by `grantKOBonus`, not here**).
B. The KO'd player's active captain passive, in effect order:
   `onAllyKO/bonusWill` (uncapped Volonté + log) and `selfBuffOnAllyKO` (see §3.1).
C. `banishOnKO` on the **killer's flipped** captain (see §3.1).
D. `explodeOnKO` on the KO'd card's def (see §3.1).
E. **Synergy rage**: for each of the KO'd player's board chars with a synergy whose `partnerId == koDefId`
   and `onPartnerKO` truthy → push `{id: synrage_<charId>_<n>, stat: Atk, amount: onPartnerKO,
   source: synergy_rage_<koDefId>, duration: Turn}` and log `"<name> : rage ! +<n> ATK (<koName> KO)"`.
F. `recalculatePassiveBuffs(koPlayerId)` — **which immediately strips the rage modifiers from (E)**
   because `"synergy_rage_…".starts_with("synergy_")`. `applyEnemyDebuffAuras` is **not** called.

**`sweepKOs(state, victimPlayerId, killerPlayerId)`** (used by events/ships, not by combat):
snapshot the victim's board ids once; for each still `zone == Board` with `currentPv <= 0`:
log `"<name> est KO !"` (under the victim), `grantKOBonus(victim)`, `removeFromBoard`, `applyOnKOEffects`.

### 4.8 Devil fruits, Haki, Volonté

**`applyFruitBaseEffects(state, fruitInstId, bearerInstId)`** — called from `equipObject`.
If the fruit def has `fruitEffects`: **only when `base.grantsTraits` is present (truthy — an empty
array counts)** push `{id: fruit_atk_<fruitInst>, stat: Atk, amount: base.atkBonus, source:
fruit_<fruitDefId>, duration: Permanent}` and/or the `def` equivalent (each only if the bonus is
truthy, so 0 is skipped). Traits are **not** stored as modifiers — they are derived at query time.
Always logs `"<bearerName> mange le <fruitName> ! <base.passiveDescription ?? "">"` under the
**fruit's** owner. No passive recalculation here.

**`canAwakenFruit(state, p, fruitInstId)`** — all must hold: the fruit exists and is not already
awakened; its def has `fruitEffects.awakening`; a bearer exists (`Object.values(cards)` order: first
card with `zone == Board`, `owner == p`, `attachedObjects.contains(fruitInstId)`);
`porteurLegitime` empty **or** `bearerDef.name.contains(porteurLegitime)` (case-sensitive substring);
`state.turnNumber >= minTurns`; `canAfford(volCost)`.

**`awakenFruit`** — errors `"Cannot awaken this fruit"`, `"No bearer found"`. `spendVolonte(volCost)`;
`fruit.isAwakened = true`; push `{id: fruit_awaken_atk_<inst> / fruit_awaken_def_<inst>,
source: fruit_awaken_<fruitDefId>, duration: Permanent}` for the awakening bonuses (unconditionally —
no `grantsTraits` gate here); log `"⭐ EVEIL ! <bearer> eveille le <fruit> ! <passiveDescription>"`;
`recalculatePassiveBuffs(p)`. **Irreversible.**

**`getFruitTraits(state, instId)`** — **DEAD CODE, zero callers.** `grep -rn getFruitTraits src/`
returns exactly one hit, `src/engine/fruits.ts:164` — its own definition. It is listed in the
"intentionally dropped" table of §4.3 and is **not** live engine behaviour: no code path in the
shipped build ever calls it, so it contributes **no parity signal** to the §1.2 differential harness
and a porter should not implement or test it. Semantics, recorded only so the omission is not
rediscovered: concatenates, for each attached object of subtype `fruit`, `base.grantsTraits`, plus
`awakening.grantsTraits` when the fruit instance `isAwakened`. Duplicates are not removed. It is
**not** used by `hasConquerorInPlay` or `matchesFilter` — nor by anything else; the live trait query
is `hasTrait` (§4.3), which re-derives the same information inline.

**Haki**
* `isHakiAvailable(state, p, t)`: `turnNumber >= {observation:5, armament:7, king:10}` and then
  `observation → !observationUsed`, `armament → **always false**` (armament is a passive from T7),
  `king → !kingUsed`. No Volonté cost anywhere.
* `hasConquerorInPlay(state, p)`: `capTraits = flipped ? verso.traits : capDef.traits`;
  `capTraits.contains(conqueror)` → true; else `capDef.traits.contains(conqueror)` → true (so top-level
  traits count on both faces); else any board character whose **printed** `def.traits` contains
  `conqueror`. Fruit/equipment-granted conqueror is ignored.
* `useKingHaki(state, p)`: errors `"Roi Haki not available"`, `"Roi Haki requires a Conquerant unit in
  play"`. `kingUsed = true`; log `"👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !"`;
  snapshot the enemy board characters with **effective DEF ≤ 3** (taken *before* any removal, so
  later victims are KO'd even if their DEF rises); for each still on the board:
  log `"<name> est KO (Haki des Rois) !"` (under its owner) → `grantKOBonus(victimOwner)` →
  `removeFromBoard` → `applyOnKOEffects(victimOwner, p, victimDefId)`. The enemy captain is never
  affected; `kingUsed` is consumed even with zero victims.
* `handleHaki` routing (turn.rs): `observation → useObservationHaki(currentPlayer)`,
  `king → useKingHaki(currentPlayer)`, `armament →` state unchanged.

**Volonté** (`will.rs`)
* `gainVolonte(state)` — current player only: `volonte = min(turnNumber, 10)` (**replace**).
* `spendVolonte(state, p, amount)` — `amount <= 0` is a no-op; else
  `Not enough Volonte: has <v>, needs <n>` if insufficient; else subtract.
* `canAfford(state, p, cost)` = `volonte >= cost` (true for `cost <= 0`).
* `grantKOBonus(state, p)` — `volonte += 2` (**uncapped**) and `allyKOedThisTurn = true`.

### 4.9 The action reducer and valid-action generation

**`executeAction(state, action)`**
1. If `state.winner` is set → return the state unchanged (every action becomes a silent no-op).
2. Dispatch, always using `state.currentPlayer` as the actor where one is needed:
   `deployCharacter/equipObject/deployShip/baseSupportAction/playEvent/flipCaptain/activateShip/
   useHaki/moveCharacter/awakenFruit` → `(state, currentPlayer, …)`;
   `baseAttack/specialAttack/fruitSpecialAttack` → the combat declare functions
   (`targetIsCaptain ?? false`); `playCounter` → routed by `counterEffect.type`
   (`reduceDamage → applyCounterReduce`, `survive → applyCounterSurvive`,
   `cancel | untargetable → applyCounterCancel`; errors `"Counter not found"`, `"Not a counter card"`,
   `"No counter effect"`); `useShield` → `applyShieldBlock`; `passCounter` → `resolveAttack`;
   `captainAttack` → `declareCaptainBaseAttack(currentPlayer, targetInstanceId, targetIsCaptain ?? false)`
   (**`isSpecial` is ignored — always a base attack**); `endTurn` → `startTurn(endTurn(state))`.
3. **No legality validation**: no phase check, no pending-attack check, no cross-check against
   `valid_actions`. Preconditions live inside the delegated functions or only in the generator.

**`getValidActions(state, playerId)`**

*Reaction branch (checked first, before any phase/current-player test).* If `state.pendingAttack` is
set: `defenderId = opponent(currentPlayer)`; if `playerId != defenderId` → `[]`. Otherwise push in
this order: (1) `playCounter` for each id from `getEligibleCounters`; (2) `passCounter` (always);
(3) `useHaki{observation}` if `isHakiAvailable(observation)` and `!pending.cannotBeDodged`;
(4) `useShield` — only if `!pending.ignoreShield`: `targetSlot = pending.targetIsCaptain ?
captain.slot : cards[pending.targetId].slot`; for each adjacent slot holding a card that is not the
target, is untapped and `hasTrait(shield)` → `useShield{blockerInstanceId}`. **No `endTurn` is offered
during a pending attack.**

*Main-phase branch.* `playerId != currentPlayer` → `[]`; `phase != Main` → `[]`. Then, in this exact
order (order is load-bearing for AI tie-breaks):

1. **deployCharacter** — for each hand card with `type == Character` and
   `canAfford(deployCost(...))`, one action per empty slot (`emptySlots` computed once, ALL_SLOTS order).
2. **deployShip** — hand ships with `canAfford(def.cost)` (offered even when a ship is already active).
3. **equipObject** — hand objects with `canAfford(def.cost)` × every own board character
   (no restriction/slot-cap check here).
4. **playEvent** — hand events with `canAfford(def.cost)` (`requiresOwnKO` is **not** pre-checked).
5. **baseSupportAction** — for each own board character that is not tapped, has not used its base
   action, has no summoning sickness and no `freeze|immobilize|sleep|loseAction` status, whose
   `baseAction.isSupport` is true; exactly one sub-branch, in this order:
   `scry` → untargeted; `bluff` → each enemy with effective DEF ≤ 1; `buffAllyAtk` → every ally
   **including self**; `immobilize` → each enemy; description contains `"piege"`/`"Piege"` → each
   enemy; `healAmount` → every ally **excluding self**; otherwise untargeted (global buff).
6. **baseAttack** — own board characters that are untapped, have not used the base action, are not
   summoning-sick, have `getEffectiveAtk > 0` and no blocking status; `getValidTargets(…, false)`;
   `cannotAttackFemale` removes targets tagged `"female"` (not the captain). Captain target is emitted
   as `targetInstanceId = "captain_<opponent>"`, `targetIsCaptain = true`. Support characters with
   ATK > 0 are also offered attacks; `def.baseAction` existence is not checked.
7. **specialAttack** — own board characters with `!usedSpecialAttack` (**`tapped` is not checked**),
   no sickness, a `specialAttack`, not already used if `oncePerGame`, affordable, no blocking status.
   `transform` specials are emitted **self-targeted** and `continue`; `isSupport` specials are skipped
   entirely. Otherwise `getValidTargets(…, true)` with the same female filter and captain encoding.
8. **fruitSpecialAttack** — own board characters without sickness and without `freeze|immobilize`
   (**`sleep`/`loseAction`, `tapped` and `usedSpecialAttack` are not checked**); for each attached
   awakened fruit with an awakening special, not already used (checked on the **character's**
   `usedOnceAbilities` by name) and affordable → `getValidTargets(…, true)` (no female filter).
9. **flipCaptain** — if `canFlipCaptain(state, p)`, one action per empty slot.
10. **captainAttack** — if the captain is flipped, has a slot, is untapped and has no
    `freeze|immobilize`, and (`deployedTurn != turnNumber` or `verso.traits` contains `rush`):
    first the enemy captain (`"captain_<opp>"`, `targetIsCaptain: true`), then **every** enemy board
    character (line/range/stealth rules are **not** applied, `usedBaseAction` is **not** checked,
    `isSpecial` is never set).
11. **moveCharacter** — if `!usedFreeMove`, for each own board character, each adjacent empty slot
    (no sickness/status/tapped gating).
12. **activateShip** — if an `activeShip` exists with a `shipActive` that is either not `oncePerGame`
    or not yet in `usedOnceAbilities`, and affordable.
13. **awakenFruit** — for each attached object of subtype `fruit` with `canAwakenFruit`.
14. **useHaki{king}** — if `isHakiAvailable(king)` and `hasConquerorInPlay` and at least one enemy
    board character has effective DEF ≤ 3. (Armament is never offered.)
15. **endTurn** — always appended last.

### 4.10 Support actions, events, ships, tokens (reducer side)

**`executeSupportAction(p, instanceId, targetInstanceId?)`** — errors `"Card not found"`,
`"Not your card"`, `"Action already used"` (`tapped || usedBaseAction`), `"Not a support action"`.
**No sickness/status re-check.** Sets `tapped = usedBaseAction = usedSpecialAttack = true`, then the
first matching branch:
1. `scry` → take the top `min(scry, deck.len())` deck ids and **sort them ascending by `def.cost`**
   (stable), write them back to indices `0..n`. Log
   `"<name> utilise <action> : réorganise le dessus du deck."`
2. `bluff && target` → status `loseAction{2, 0, sourceInstanceId}`; log
   `"<name> utilise <action> : <target> a peur et perd sa prochaine action !"` (**no DEF ≤ 1 check**).
3. `buffAllyAtk && target` → `{id: support_<defId>_<n>, stat: Atk, amount, source: support_<defId>,
   duration: Turn}`; log `"… : <target> +<n> ATK ce tour."`
4. `immobilize && target` → status `immobilize{2,0}`; log `"… : <target> est immobilise !"`
   (**no `immuneControl` check**).
5. `description` contains `"piege"`/`"Piege"` → requires a target (`"Trap needs a target"`); status
   `trap{-1, 3}`; log `"… : piege pose sur <target> !"`
6. `healAmount && target` → `currentPv = min(cur + heal, def.pv ?? cur + heal)`; log
   `"… : +<n> PV a <target>"` (self-target allowed here, no `noHeal` check).
7. Fallback (including any targeted branch whose target was omitted, except trap which throws):
   every own board card gets `{id: support_<actionName>_<n>, stat: Atk, amount: 1, source:
   support_<defId>, duration: Turn}`; log `"<name> utilise <action> !"`.

**`playEvent(p, instanceId)`** — errors `"Card not found"`, `"Not your card"`, `"Card not in hand"`,
`"Not an event card"`, `Cannot afford <name>`. `spendVolonte(def.cost)`; resolve `eventEffect`
(§3.2) **while the card is still in hand**; then hand → graveyard; then log `"Joue <name>"`.

**`deployToken(p, tokenDefId)`** — first empty slot (ALL_SLOTS order); silently returns unchanged if
the board is full. Creates a full instance (`currentPv = def.pv ?? 1`, `deployedTurn = turnNumber`,
`zone = Board`), pushes log `"Déploie <name>."`, then `recalculatePassiveBuffs(p)`. No cost.

**`activateShipAbility(p, shipInstanceId)`** — errors `"Ship not found"`,
`"Ship has no active ability"`, `"Ship ability already used (1x/game)"`,
`Cannot afford <name> (cost <n>)`. **No check that the ship is the player's `activeShip`.**
Spend (if cost > 0), record `usedOnceAbilities.push(active.name)` when `oncePerGame`, then the first
matching branch:
1. `def.id == "MG-021"` (Gaon Cannon): `dmg = first integer in the description ?? 5`; enemies =
   board characters, prefer the front row if non-empty; target the one with the **lowest effective
   DEF** (first on ties); `currentPv -= dmg`; log `"<ship> : Gaon Cannon — <n> dégâts !"`; `sweepKOs`.
   If there are **no** enemy characters: enemy captain `currentPv -= dmg`, log
   `"<ship> : Gaon Cannon — <n> dégâts au Capitaine !"`, then `checkWinCondition` → `winner`.
2. `def.id == "BW-019"`: `deployToken("TOK-AGENT")` twice; log `"<ship> : deux agents déployés !"`.
3. `def.id == "RH-017"`: `hakiThisTurn = true` and `{id: redforce_<inst>_<n>, stat: Atk, amount: 1,
   source: ship_RH-017, duration: Turn}` on every own board card; log
   `"<ship> : Haki d'Armement et +1 ATK ce tour !"`.
4. lowercased description contains `"deg."` **and** `"avant"`: `dmg` from `/(\d+)\s*deg/` applied to
   the **original-case** description (so a capital "Deg" yields 0); each enemy V1/V2/V3 loses `dmg`.
   **No `sweepKOs`** — cards can stay at ≤ 0 PV. Log `"<ship> active <name> !"`.
5. lowercased description contains `"+"` and (`"atk"` or `"def"`): `atkBuff` from `/\+(\d+)\s*atk/i`
   and `defBuff` from `/\+(\d+)\s*def/i` (both matched against the **lowercased** description; absent
   → 0). Then, for **every** non-null slot of the activating player's board (`Object.values(p.board)`
   order, dangling ids skipped), push — each only when its buff is `> 0`, so a `+0` is never pushed:
   * `{id: ship_<active.name>_atk_<Date.now()>, stat: Atk, amount: atkBuff, source: ship_<def.id>,
     duration: Turn}`
   * `{id: ship_<active.name>_def_<Date.now()>, stat: Def, amount: defBuff, source: ship_<def.id>,
     duration: Turn}`

   Log `"<ship> active <name> !"`.

   **The `source` is load-bearing, and it is `ship_<def.id>` — not `passive_ship_<def.id>`.**
   `recalculatePassiveBuffs` (§4.7) strips modifiers whose `source` starts with `passive_`,
   `captain_` or `synergy_` (`passives.ts:132`), and the *passive* ship aura at `passives.ts:217`
   deliberately uses `source: passive_ship_<shipDef.id>` so that it is stripped and re-derived. This
   **active** buff must survive the next deploy / equip / KO / captain flip, so it uses the bare
   `ship_<def.id>` prefix and is **never** stripped — it expires only through the end-of-turn
   `duration: Turn` sweep. A porter who "normalises" branch 3 (RH-017, also `source: ship_RH-017`) or
   branch 5 onto the `passive_ship_` prefix silently deletes both buffs on the next recalculation.
   The `id`s embed `Date.now()`, so under the §1.4 determinism contract they must come from the
   injected clock/counter, and — because the same millisecond is reused for every slot in the loop —
   the ids of a single activation are **not unique across cards**; nothing reads them, only `source`
   and `duration` are ever queried, so the collision is harmless and must be reproduced as-is.
6. Fallback: log `"<ship> active <name> !"` (cost spent, no effect).

### 4.11 Captain flip and captain attacks

**`canFlipCaptain(state, p)`** (pure): `false` if already flipped. `true` if
(`freeIfAllyKO && allyKOedThisTurn`); else `true` if `autoIfAlliesLte` is `Some` **and
`turnNumber >= 4`** and `boardCharacterCount <= autoIfAlliesLte`; else `canAfford(cost)` if `cost` is
`Some`; else `false`. `freeIfEnemyCursed` / `freeIfAlliesGte` / `freeIfTurnGte` are **never read**.

**`flipCaptain(state, p, slot)`**:
1. `"Captain already flipped"`; `Slot <slot> is occupied` (board slot must be `null`; any of the six
   slots is allowed).
2. `freeFlip = (freeIfAllyKO && allyKOedThisTurn) || (autoIfAlliesLte.is_some() && turnNumber >= 4 &&
   allyCount <= autoIfAlliesLte)`. If not free: `cost = flipCondition.cost ?? 0`; if
   `!canAfford(cost)` → `"Cannot afford captain flip"`. `spendVolonte` only when `cost > 0`
   (so a captain with no cost and no free condition flips for free — see §8).
3. `flipped = true`; `dmgMarked = max(0, recto.pv - currentPv)`; `currentPv = verso.pv - dmgMarked`;
   `slot = slot`; `deployedTurn = turnNumber`. **`board[slot]` is NOT written** — the captain occupies
   a slot only via `captain.slot`. Nothing is reset (tapped, used flags, modifiers, statuses persist).
4. Log `"<capName> s'engage sur le champ de bataille ! (verso, slot <slot>)"`.
5. `checkWinCondition` — if it returns a winner, set `state.winner` and **return without resolving the
   entry effect** (a lethal flip still costs the Volonté).
6. `resolveEntryEffect(verso.entryEffect)` — §3.4.

**`declareCaptainBaseAttack(state, p, targetInstanceId, targetIsCaptain)`**:
`"Captain not flipped (verso required)"`, `"Captain is tapped"`,
`"Captain base action already used"`, and summoning sickness
(`deployedTurn == turnNumber && !verso.traits.contains(rush)` → `"Captain has summoning sickness"`).
`atk = verso.atk + Σ captain.modifiers[Atk].amount` (**`baseAction.atk` is ignored**);
target DEF as in §4.6 (captain face def + captain def modifiers, or `getEffectiveDef`);
**no piercing / ignoreDef step**; `rawDamage = max(0, atk - def)`;
`hasHaki = verso.naturalHaki non-empty || turnNumber >= 7` (`hakiThisTurn` not consulted).
`tapped = usedBaseAction = usedSpecialAttack = true`;
`pendingAttack = { attackerId: "captain_<p>", targetId, targetIsCaptain, isSpecial: false, rawDamage,
attackPower: atk, element: baseAction.element, attackTraits: baseAction.attackTraits ?? [], hasHaki }`
(no `ignoreShield`, no `cannotBeDodged`, no control flags). Log
`"Capitaine <name> attaque avec <actionName> ! (<raw> degats)"`.
Captain **special** attacks, surcharge and `recto.attacks` are not implemented anywhere.

### 4.12 AI

`aiChooseAction(state, p, difficulty = Intermediate)`:
1. `actions = getValidActions(state, p)`; empty → return a fabricated `EndTurn`.
2. `actions.len() == 1` → return it (for every difficulty; no RNG consumed).
3. `beginner → chooseBeginner`, `expert → chooseExpert`, anything else → `chooseByScore(jitter = 0)`.

**`chooseByScore(actions, jitter)`**: `best = actions[0]`, `bestScore = -inf`; for each action,
`score = scoreAction(...) + (jitter > 0 ? rng()*jitter : 0)`; strict `>` replaces (first wins ties).
With `jitter == 0` **no random number is drawn at all**.

**`chooseBeginner`** (draw order matters):
1. If `pendingAttack` is set and a `passCounter` action exists: draw once; `< 0.8` → return it.
2. `simple = actions.filter(type ∈ BASIC)` where
   `BASIC = {deployCharacter, baseAttack, moveCharacter, endTurn, passCounter, playCounter, useShield}`;
   `pool = if simple.is_empty() { actions } else { simple }`.
3. Draw once; `< 0.35` → draw once more and return `pool[floor(r * pool.len())]`.
4. Otherwise `chooseByScore(pool, 7)` (one draw per action).

**`chooseExpert`** (1-ply, no opponent reply): `best = actions[0]`, `bestVal = -inf`; for each action,
`val = evaluateState(executeAction(state, action), p)` (a throw ⇒ `-inf`), then
`val += scoreAction(state, p, action) * 0.02` (computed **outside** the try), then
`if action == EndTurn { val -= 6 }`; strict `>` replaces. Always passes the **original** state.

**`scoreAction`**: `deployCharacter → scoreDeployCharacter`; `deployShip → 15`;
`equipObject → scoreEquipObject`; `baseAttack | specialAttack | captainAttack → scoreAttack`;
`playEvent → scoreEvent`; `playCounter → 50`; `useShield → scoreShield`; `passCounter → 0`;
`flipCaptain → scoreCaptainFlip`; `useHaki → observation 40 / king 60 / else 10`;
`moveCharacter → 2`; `endTurn → -1`; **default (baseSupportAction, activateShip, awakenFruit,
fruitSpecialAttack) → 0**.

* `scoreDeployCharacter`: `10 + (def.atk ?? 0)*2 + (def.pv ?? 0)`; `+10` if `boardCount < 3`;
  `+20` more if `boardCount == 0`; `+3` if `preferredRow == Front && slot ∈ {V1,V2,V3}` or
  `preferredRow == Back && slot ∈ {A1,A2,A3}`. (`cost` and `def` are not read.)
* `scoreEquipObject`: `8 + (objDef.bonusAtk ?? 0)*3` (throws if the instance is missing; the target is
  not considered).
* `scoreAttack`: `5`; `+15` if `targetIsCaptain`; for character targets of base/special:
  `damage = max(0, effectiveAtk(attacker) - effectiveDef(target))`, `+20` if `damage >= target.currentPv`,
  `+ max(0, 5 - target.currentPv)`; `+5` if `type == specialAttack`. `captainAttack` never enters the
  KO block (it has no `attackerInstanceId`) → 5 or 20.
* `scoreEvent`: missing card → 0; no `eventEffect` → 5; `gainWill 18`, `draw 14`, `healAlly 8`,
  `buffAllies 12 + 2*boardCount`, `damageEnemies 15 + 2*amount`, `dodgeAll 3`, everything else 6.
* `scoreShield`: no pending → 0; blocker missing → 0;
  `atkPower = attackPower ?? rawDamage`; `redirected = max(0, atkPower - blockerDef)`;
  `savesKO = targetIsCaptain ? rawDamage >= 6 : (cards[targetId].currentPv ?? 0) <= rawDamage`;
  `blockerSurvives = redirected < blocker.currentPv`;
  → `32` if `savesKO && blockerSurvives`, else `20` if `redirected == 0`,
  else `6` if `redirected < rawDamage && blockerSurvives`, else `0`.
* `scoreCaptainFlip` (first match wins, `slot` ignored): `turnNumber < 4 → -10`;
  `boardCount == 0 && turn >= 5 → 30`; `boardCount <= 1 && turn >= 6 → 25`; `turn >= 7 → 15`;
  `turn >= 5 && boardCount <= 2 → 10`; else `-5`.

**`evaluateState(state, p)`** (f64, from `p`'s viewpoint), terms in this order:
```
v  = (myCaptainPv - oppCaptainPv) * 1.5
v += 1000 if oppCaptainPv <= 0
v -= 1000 if myCaptainPv  <= 0
v += 1000 if winner == p
v -= 1000 if winner == opponent
for c in myBoardCharacters:  v += effAtk + effDef + c.currentPv * 0.5 + 3
for c in oppBoardCharacters: v -= effAtk + effDef + c.currentPv * 0.5 + 3
v += myHand.len() * 1.4 - oppHand.len() * 1.0
v += myVolonte * 0.4
```
Captains are not board characters and contribute only through the PV terms. Ships, equipment (beyond
their stat contribution), statuses, tapped state, deck size and graveyards are ignored.

---

## 5. Card Catalogue

**118 distinct entries**: 28 ST01 Mugiwara + 28 ST02 Marines + 28 ST03 Baroque Works
+ 27 ST04 Red Hair + 3 tokens + 4 captains.

Conventions used below:
* `atk/def/pv` are the printed character stats; `cost` is in Volonté.
* "base" = `baseAction`, "special" = `specialAttack`, "passive" = `passive.effects`.
* Fields not mentioned are absent (`None`).
* **TEXT-ONLY** marks behaviour that exists only as a French string (`equipEffect`, `shipPassive`,
  `shipActive.description`, `passiveDescription`, attack `description`) with no structured field.
  The engine does not implement these unless a card-id special case exists (those are listed in §4).
* Array order within each set file is preserved below and **matters** for instance-id numbering.

### 5.1 ST01 — Mugiwara (`mugiwaraCards`, set `"ST01"`, faction `pirate`) — 28 cards

Array order: MG-001…MG-025, **MG-028**, MG-026, MG-027.

| id | name | type | cost | rarity | stats (ATK/DEF/PV) | traits | tags | row |
|---|---|---|---|---|---|---|---|---|
| MG-001 | Roronoa Zoro | character | 5 | R | 7/3/10 | — | mugiwara, bretteur | front |
| MG-002 | Sanji | character | 4 | R | 6/3/7 | — | mugiwara, cuisinier | front |
| MG-003 | Nami | character | 2 | C | 1/1/4 | range | mugiwara, navigateur, female | back |
| MG-004 | Usopp | character | 2 | C | 1/1/4 | range | mugiwara, tireur | back |
| MG-005 | Tony Tony Chopper | character | 2 | C | 1/2/6 | — | mugiwara, medecin | back |
| MG-006 | Nico Robin | character | 3 | R | 3/2/5 | range | mugiwara, female, archeologue | back |
| MG-007 | Franky | character | 3 | U | 5/4/8 | shield | mugiwara, charpentier | front |
| MG-008 | Brook | character | 3 | U | 5/2/6 | — | mugiwara, musicien, bretteur | front |

* **MG-001 Roronoa Zoro** — no passive (the "Rivalité" is a synergy).
  `synergies: [{partnerId: "MG-002", atkBonus: 2}]`.
  base `Coup de Sabre` atk 7. special `Oni Giri` cost 3, atkBonus 3, attackTraits `[piercing]`.
* **MG-002 Sanji** — passive **Galanterie**: `[cannotAttackFemale]`.
  `synergies: [{partnerId: "MG-001", atkBonus: 2}]`.
  base `Coup de Pied` atk 6. special `Diable Jambe` cost 3, atkBonus 3, element `fire`.
* **MG-003 Nami** — no passive. base `Prévisions` atk 0, `isSupport`, `scry: 2`.
  special `Thunder Tempo` cost 2, atkBonus 3, element `thunder`.
* **MG-004 Usopp** — passive **Vantardise**: `[startTurnBuffAlly{stat: atk, amount: 1}]`.
  base `Bluff` atk 0, `isSupport`, `bluff: true` (TEXT: enemy of DEF ≤ 1 loses its next action — the
  DEF ≤ 1 filter exists only in the valid-action generator).
  special `Kayaku Boshi` cost 2, atkBonus 3, element `fire`.
* **MG-005 Tony Tony Chopper** — passive **Médecin de bord**: `[healAdjacent{amount: 1}]`.
  base `Point de Suture` atk 0, `isSupport`, `healAmount: 2`.
  special `Monster Point` cost 3, atkBonus 0, `oncePerGame`, `isSupport`,
  `transform: {atk: 8, def: 2, pv: 6, turns: 2}` (TEXT promises Rush for 2 turns then automatic KO;
  the engine grants no Rush and ignores `def`/`pv`).
* **MG-006 Nico Robin** — passive **Hana Hana**: `[noDodge]`.
  base `Seis Fleur` atk 3. special `Clutch` cost 2, atkBonus 2, `immobilize`.
* **MG-007 Franky** — no passive. base `Strong Right` atk 5.
  special `Coup de Vent` cost 3, atkBonus 4, element `fire`, attackTraits `[range]`.
* **MG-008 Brook** — passive **Le Barde**: `[buffAlly{stat: def, amount: 1}]` (no filter).
  base `Mélodie de l'Âme` atk 0, `isSupport`, `buffAllyAtk: 2`.
  special `Hanauta Sancho: Yahazu Giri` cost 3, atkBonus 3, element `ice`.

**Weapons (subtype `weapon`)**

| id | name | cost | rarity | bonusAtk | restriction | grantsElement | equipEffect (TEXT-ONLY beyond bonusAtk) |
|---|---|---|---|---|---|---|---|
| MG-009 | Wado Ichimonji | 1 | C | 1 | bretteur | — | +1 ATK. If equipped by Zoro: +1 DEF more. *(the +1 DEF is engine-implemented via the `wield_` table)* |
| MG-010 | Sandai Kitetsu | 1 | C | 2 | bretteur | — | +2 ATK. Curse: at the start of your turn the bearer takes 1 damage. *(implemented by defId in `startTurn`)* |
| MG-011 | Yubashiri | 1 | C | 1 | bretteur | — | +1 ATK. If destroyed: bearer gains +1 permanent ATK. **TEXT-ONLY** |
| MG-012 | Clima-Tact | 1 | C | 1 | Nami | thunder | +1 ATK, attacks become Thunder. Combo (Usopp + Nami in play): costs 0. *(cost-0 combo implemented by defId in `equipObject`)* |
| MG-013 | Kabuto | 1 | C | 1 | tireur | — | +1 ATK. Bearer's attacks are undodgeable. *(implemented by defId in `attackerNoDodge`)* |

**Devil fruits (subtype `fruit`, all `bonusAtk: 0`, base `grantsTraits: [cursed]`, `minTurns: 5`)**

* **MG-014 Gomu Gomu no Mi** — cost 3, SR, restriction `Luffy`.
  equipEffect: "Maudit. Immunité Impact. Portée sur les attaques."
  base passiveDescription (TEXT): "Immunité Impact. Gomu Gomu no Pistol : 1 Vol · ATK +3 · Impact."
  awakening: `porteurLegitime "Luffy"`, `volCost 3`, `atkBonus 3`, `grantsTraits [rush]`,
  passiveDescription "Gear 4 Boundman — +3 ATK, Rush, Impact sur toutes les attaques.",
  specialAttack `Kong Gun` cost 4, atkBonus 6, `oncePerGame`, description
  "Impact · Zone · ignore le Bouclier." (**no structured attackTraits / ignoreShield**).
* **MG-015 Hana Hana no Mi** — cost 2, R, restriction `Robin`.
  equipEffect: "Maudit. Le porteur gagne Portée."
  base passiveDescription: "Le porteur gagne Portée. Doce Fleur : 1 Vol · ATK +2 · repositionne la cible."
  awakening: `porteurLegitime "Robin"`, `volCost 2`, `atkBonus 0`, no grantsTraits,
  passiveDescription "Mil Fleur — Portée ; un ennemi Avant ne peut pas utiliser Bouclier à votre tour.",
  specialAttack `Gigante Fleur` cost 3, atkBonus 0, description "Immobilise toute la Ligne Avant
  adverse 1 tour + 2 dégâts à chacun." (**no structured immobilize**).
* **MG-016 Yomi Yomi no Mi** — cost 2, R, restriction `Brook`.
  equipEffect: "Maudit. Résurrection 1x/partie (revient 3 PV)."
  base passiveDescription: "Revenant : si KO, revient (même slot) avec 3 PV — 1x/partie.
  Aubade Coup Droit : 1 Vol · ATK +2 · Glace." (**no structured `revive`**).
  awakening: `porteurLegitime "Brook"`, `volCost 2`, `atkBonus 0`, no grantsTraits,
  passiveDescription "Soul King — Revenant + les ennemis adjacents au porteur −1 ATK.",
  specialAttack `Nemuriuta Flanc` cost 3, atkBonus 0, description "Un ennemi est endormi (perd ses 2
  prochaines actions)." (**no structured sleep**).

**Accessories (subtype `accessory`, all `bonusAtk: 0`)**

| id | name | cost | rarity | grantsElement | effect |
|---|---|---|---|---|---|
| MG-017 | Baril d'Eau | 1 | C | water | Bearer's attacks become Water (×2 vs Cursed, hits Logia). *(structured via `grantsElement`, applied to base attacks only)* |
| MG-018 | Dial d'Impact | 2 | U | — | 1×/game: absorbs an incoming attack (→ 0) and deals that amount to an enemy next turn (Impact). **TEXT-ONLY** |
| MG-019 | Vivre Card | 1 | C | — | If the bearer is KO'd, tutor a Mugiwara character of cost ≤ 3 to hand. *(implemented by defId in `removeFromBoard`)* |

**Ships**

* **MG-020 Going Merry** — cost 1, U. `shipPassive`: "Vos Mugiwara ont +1 PV au déploiement."
  (parsed by the `déploiement` + `+1 pv` rule). `shipDestroyEffect: {healAll: 2, draw: 1}`.
  No `shipActive`.
* **MG-021 Thousand Sunny** — cost 2, R. `shipPassive`: "Vos Mugiwara ont +1 ATK."
  (parsed by the `+1 atk` regex, mugiwara-tag gated). `shipActive`: `Gaon Cannon`, cost 3,
  description "5 dégâts à un ennemi (Portée).", `oncePerGame` — implemented as a hard-coded branch
  (damage = first integer in the description = 5). No `shipDestroyEffect`.

**Events**

| id | name | cost | rarity | eventEffect |
|---|---|---|---|---|
| MG-022 | Volonté du D. | 1 | C | `gainWill{amount: 2}` |
| MG-023 | Nakama ! | 2 | U | `rally{atk: 1, def: 1, heal: 1}` |
| MG-024 | Flashback : Promesse | 1 | C | `buffSingle{stat: atk, amount: 3, duration: permanent, requiresOwnKO: true}` |
| MG-025 | Tempête | 3 | U | `damageEnemies{amount: 2, target: all, cursedBonus: 3}` |
| MG-028 | Coup de Burst | 3 | R | `rushBuff{atk: 3}` *(listed BEFORE MG-026/027 in the array)* |

**Counters**

| id | name | cost | rarity | counterEffect |
|---|---|---|---|---|
| MG-026 | JE VEUX VIVRE ! | 0 | R | `survive{description: "Un allié Mugiwara survit avec 1 PV. Une fois par personnage."}` — the "Mugiwara only" restriction is TEXT-ONLY; the once-per-character limit **is** enforced via `usedOnceAbilities "survived"` |
| MG-027 | Drapeau Noir | 1 | C | `reduceDamage{amount: 4}` (no `captainBonus`) |

### 5.2 Tokens (`tokenCards`, set `"TOKEN"`, `isToken: true`, cost 0, rarity C) — 3 cards

| id | name | faction | ATK/DEF/PV | tags | row | base action |
|---|---|---|---|---|---|---|
| TOK-MARINE | Jeton Marine | marine | 2/1/3 | marine, soldat | front | `Tir` atk 2 — "Un simple soldat." |
| TOK-AGENT | Agent Baroque Works | pirate | 3/1/3 | baroque | front | `Frappe` atk 3 — "Un agent anonyme." |
| TOK-BANANAWANI | Bananawani | pirate | 4/1/4 | baroque | front | `Morsure` atk 4 — "Un crocodile-banane affamé." |

No passives, no special attacks, no traits, no synergies. Deployed only by effects
(`deployTokens`, `shipDestroyEffect.deployToken`, the `BW-019` ship ability). Token instances are
created directly by `deployToken` with `currentPv = def.pv ?? 1` and `deployedTurn = turnNumber`
(so they are summoning-sick on the turn they appear).

### 5.3 Captains (`allCaptains` — order: Luffy, Akainu, Crocodile, Shanks) — 4 entries

All four have `recto.attacks: []` (a recto captain cannot attack) and **no `surcharge`** on either
side. Recto has no `baseAction`, no `traits`, no `naturalHaki`.

#### CAP-LUFFY — "Monkey D. Luffy" (faction `pirate`, tags `[mugiwara]`, top-level traits `[conqueror]`)
* **recto** — PV 30 / ATK 4 / DEF 2.
  passive **Pavillon au Chapeau de Paille**: `[buffAlly{stat: atk, amount: 1, filter: {faction: pirate}}]`.
* **flipCondition** — `{cost: 2, freeIfAllyKO: true}`.
* **verso** — PV 25 / ATK 6 / DEF 2, traits `[cursed, conqueror]`, no naturalHaki.
  passive **Esprit de Capitaine**:
  `[immuneImpact, selfBuffOnAllyKO{stat: atk, amount: 1, max: 3, filter: {tag: "mugiwara"}}]`.
  entryEffect: `multi[ grantSelfRush, damageEnemies{amount: 3, target: single} ]`.
  base `Gomu Gomu no Pistol` atk 6.
  special `Gomu Gomu no Bazooka` cost 3, atkBonus 4, attackTraits `[impact]`, `pushback: true`.

#### CAP-AKAINU — "Akainu (Sakazuki)" (faction `marine`, tags `[marine, amiral]`, traits `[]`)
* **recto** — PV 35 / ATK 5 / DEF 4.
  passive **Ordre de Marche**: `[buffAlly{stat: atk, amount: 1, filter: {faction: marine}}]`.
* **flipCondition** — `{cost: 3, freeIfEnemyCursed: true}` (**`freeIfEnemyCursed` is never read**).
* **verso** — PV 28 / ATK 8 / DEF 3, traits `[cursed, logia]`.
  passive **Justice Implacable**: `[logiaIntangibility, banishOnKO]`.
  entryEffect: `damageEnemies{amount: 4, target: single, cursedBonus: 6}`.
  base `Coup de Magma` atk 8, element `fire`.
  special `Ryusei Kazan` cost 4, atkBonus 4, element `fire`, attackTraits `[zone]`.

#### CAP-CROCODILE — "Crocodile (Mr. 0)" (faction `pirate`, tags `[baroque]`, traits `[]`)
* **recto** — PV 30 / ATK 5 / DEF 3.
  passive **Maître de Baroque Works**: `[buffAlly{stat: atk, amount: 1, filter: {tag: "baroque"}}]`.
* **flipCondition** — `{cost: 3, freeIfAlliesGte: 3}` (**`freeIfAlliesGte` is never read**).
* **verso** — PV 25 / ATK 7 / DEF 2, traits `[cursed, logia]`.
  passive **Déshydratation**: `[logiaIntangibility, endTurnDesiccation{amount: 1}]`.
  entryEffect: `damageEnemies{amount: 4, target: single, sand: true}`.
  base `Sables` atk 7.
  special `Desert Girasol` cost 4, atkBonus 4, element `sand`, `permanentPvLoss: 2`
  (**`permanentPvLoss` is not implemented by combat**).

#### CAP-SHANKS — "Shanks (Akagami)" (faction `pirate`, tags `[redhair]`, traits `[]`)
* **recto** — PV 35 / ATK 6 / DEF 3.
  passive **Volonté de l'Empereur**: `[buffAlly{stat: atk, amount: 1}]` (**no filter** — all allies).
* **flipCondition** — `{cost: 4, freeIfTurnGte: 7}` (**`freeIfTurnGte` is never read**).
* **verso** — PV 30 / ATK 8 / DEF 4, traits `[conqueror]`, `naturalHaki: [armament]`.
  passive **Présence de l'Empereur**: `[debuffAdjacentEnemies{amount: 2}]`
  (implemented as "enemy front row −2 ATK").
  entryEffect: `haoshoku{immobilizeMaxDef: 2, debuffAtk: 2}`.
  base `Coup de Sabre` atk 8 (TEXT: "touche les Logia" — realised through `naturalHaki`).
  special `Divin Départ` cost 4, atkBonus 4, `ignoreDef: 99` (sentinel for "ignore all DEF"),
  `ignoreShield: true`.

### 5.4 ST02 — Marines : Justice Absolue (`marinesCards`, set `"ST02"`, faction `marine`) — 28 cards

Array order: MR-001…MR-024, **MR-027, MR-028**, MR-025, MR-026.

| id | name | type | cost | rarity | ATK/DEF/PV | traits | tags | row |
|---|---|---|---|---|---|---|---|---|
| MR-001 | Coby | character | 2 | C | 2/1/4 | — | marine, soldat | front |
| MR-002 | Helmeppo | character | 2 | C | 3/2/4 | — | marine, soldat, bretteur | front |
| MR-003 | Tashigi | character | 3 | U | 4/2/5 | — | marine, capitaine_marine, bretteur | front |
| MR-004 | Smoker | character | 4 | R | 5/3/7 | — | marine, capitaine_marine | front |
| MR-005 | Sentomaru | character | 3 | U | 4/4/7 | — | marine, garde | front |
| MR-006 | Momonga | character | 3 | U | 5/3/6 | — | marine, vice_amiral, bretteur | front |
| MR-007 | Garp | character | 5 | SR | 7/3/10 | — | marine, vice_amiral | front |
| MR-008 | Kizaru (Borsalino) | character | 5 | SR | 6/2/8 | cursed, logia | marine, amiral | front |
| MR-009 | Aokiji (Kuzan) | character | 5 | SR | 6/3/9 | cursed, logia | marine, amiral | front |
| MR-010 | Sengoku | character | 5 | SR | 6/4/9 | — | marine, amiral_en_chef | front |

* **MR-001 Coby** — `synergies: [{partnerId: "MR-002", atkBonus: 1}]`. base `Coup de Poing` atk 2.
  special `Poing de la Justice` cost 2, atkBonus 3, attackTraits `[piercing]`.
* **MR-002 Helmeppo** — `synergies: [{partnerId: "MR-001", atkBonus: 1}]`. base `Coup de Lame` atk 3.
  special `Frappe Jumelle` cost 2, atkBonus 2, `twoTargets: true` (⇒ engine adds AttackTrait `zone`).
* **MR-003 Tashigi** — passive **Chasseuse de Meito**: `[twoWeaponSlots]`. base `Coup de Sabre` atk 4.
  special `Tenpô` cost 2, atkBonus 3, attackTraits `[piercing]`.
* **MR-004 Smoker** — passive **White Snake**: `[stripStealthOnAttack]`. base `Jitte` atk 5.
  special `White Blow` cost 2, atkBonus 3, `immobilize`.
* **MR-005 Sentomaru** — `naturalHaki: [armament]`; passive **Garde du Corps**:
  `[naturalHaki{hakiType: armament}]`. base `Coup de Hache` atk 4.
  special `Ashigara Dokkoi` cost 3, atkBonus 3, `ignoreShield`.
* **MR-006 Momonga** — passive **Volonté de Fer**: `[immuneControl]`. base `Coup de Sabre` atk 5.
  special `Frappe Disciplinée` cost 2, atkBonus 3, attackTraits `[piercing]`.
* **MR-007 Garp** — `naturalHaki: [armament]`; passive **Poing de Garp**:
  `[naturalHaki{hakiType: armament}]`. base `Poing de l'Amour` atk 7, attackTraits `[impact]`
  (**BaseAction has no pushback field, so the base attack never pushes back**).
  special `Boulet de Canon` cost 3, atkBonus 3, attackTraits `[range, impact]`, `pushback: true`.
* **MR-008 Kizaru (Borsalino)** — passive **Vitesse de la Lumière**: `[logiaIntangibility, noDodge]`.
  base `Coup de Pied Lumineux` atk 6.
  special `Yasakani no Magatama` cost 4, atkBonus 4, attackTraits `[zone, range]`.
* **MR-009 Aokiji (Kuzan)** — passive **Souffle Glacial**: `[logiaIntangibility]`.
  base `Ice Time` atk 6, element `ice` (TEXT claims the target loses its next action; **no structured
  `immobilize`** — only the ice → `freeze` status applies).
  special `Ice Age` cost 4, atkBonus 3, element `ice`, attackTraits `[zone]`.
* **MR-010 Sengoku** — passive **Stratège Suprême**:
  `[buffAlly{stat: def, amount: 1, filter: {faction: marine}}]`. base `Onde de Choc` atk 6.
  special `Daibutsu : Paume de l'Illumination` cost 4, atkBonus 4, attackTraits `[zone, impact]`.

**Devil fruits (subtype `fruit`, `bonusAtk: 0`, `minTurns: 5`)**

* **MR-011 Magu Magu no Mi** — cost 3, SR, restriction `Akainu`.
  equipEffect "Applique Logia + Maudit. Attaques gagnent Feu."
  base `grantsTraits: [cursed, logia]`; passiveDescription "Logia. Meigo : 2 Vol · ATK +3 · Feu."
  awakening: `porteurLegitime "Akainu"`, `volCost 3`, `atkBonus 4`,
  passiveDescription "Terre Brûlée — les ennemis adjacents subissent 1 dégât/tour (Feu).",
  specialAttack `Inugami Guren` cost 4, atkBonus 6, `oncePerGame`, element `fire`,
  attackTraits `[zone, piercing]`.
* **MR-012 Moku Moku no Mi** — cost 2, R, restriction `Smoker`.
  equipEffect "Applique Logia + Maudit."
  base `grantsTraits: [cursed, logia]`; passiveDescription "Logia. White Out : 1 Vol · ATK +2 · la
  cible ne peut pas se déplacer."
  awakening: `porteurLegitime "Smoker"`, `volCost 2`, `atkBonus 3`,
  passiveDescription "White Monster — les ennemis ne peuvent pas quitter un slot adjacent au porteur.",
  specialAttack `White Launcher` cost 3, atkBonus 3, attackTraits `[range]`, `immobilize: true`
  (**no `oncePerGame`**).

**Weapons / accessories**

| id | name | type | cost | rarity | subtype | bonusAtk | restriction | grantsElement | effect |
|---|---|---|---|---|---|---|---|---|---|
| MR-013 | Shigure | object | 1 | C | weapon | 1 | bretteur | — | +1 ATK. If equipped by Tashigi: +1 ATK more. *(the Tashigi bonus is engine-implemented via the `wield_` table)* |
| MR-014 | Jitte Granit Marin | object | 1 | U | weapon | 1 | — | water | +1 ATK. Granit Marin: ×2 damage to Cursed and hits Logia. *(realised through `grantsElement: water`)* |
| MR-015 | Menottes Granit Marin | object | 2 | R | accessory | 0 | — | — | 1×/game: a Cursed enemy loses its traits and its next action. **TEXT-ONLY** |
| MR-016 | Canon Marine | object | 1 | C | accessory | 0 | — | — | Grants the bearer an attack: "Tir de Canon — 1 Vol · 3 dégâts (Portée)". **TEXT-ONLY** |
| MR-017 | Boulet Granit Marin | object | 1 | C | accessory | 0 | — | — | 1×/game: 2 damage to an enemy; if Cursed, 4 damage and it loses its traits this turn. **TEXT-ONLY** |

**Ships**

* **MR-018 Navire de Guerre** — cost 2, U. `shipPassive` "Vos Marine gagnent +1 ATK."
  `shipActive`: `Salve de Canons`, cost 2, description "3 dégâts à un ennemi (Portée).",
  `oncePerGame` — falls through the id branches; the description contains no `"avant"`, so it hits the
  **fallback** (log only, no damage).
* **MR-019 Navire de Justice** — cost 1, C. `shipPassive` "Vos Marine gagnent +1 PV."
  `shipDestroyEffect: {deployToken: "TOK-MARINE"}`. No `shipActive`.

**Events**

| id | name | cost | rarity | eventEffect |
|---|---|---|---|---|
| MR-020 | Buster Call | 5 | SR | `damageEnemies{amount: 4, target: all, destroyShips: true}` |
| MR-021 | Promotion | 1 | C | `buffSingle{stat: atk, amount: 1, duration: permanent}` |
| MR-022 | Justice Absolue | 2 | U | `buffAllies{stat: atk, amount: 2, filter: {faction: marine}, duration: turn}` (**filter ignored by the implementation**) |
| MR-023 | Renforts | 1 | C | `deployTokens{tokenId: "TOK-MARINE", count: 2}` |
| MR-024 | Ordre de Tir | 2 | U | `custom{id: "coordinatedFire", description: "Chaque Marine inflige 1 dégât à un ennemi."}` |
| MR-027 | Embargo | 2 | U | `custom{id: "embargo", description: "L'adversaire ne peut ni équiper ni jouer de Navire à son prochain tour."}` — **log only** |
| MR-028 | Exécution Publique | 3 | R | `custom{id: "execute4", description: "Détruisez un ennemi de PV actuels ≤ 4."}` |

**Counters**

| id | name | cost | rarity | counterEffect |
|---|---|---|---|---|
| MR-025 | Manteau de Justice | 0 | R | `cancel{description: "Annule l'attaque. 1x par partie.", once: true}` (**`once` is never enforced**) |
| MR-026 | Mur d'Acier | 1 | C | `reduceDamage{amount: 5}` |

### 5.5 ST03 — Baroque Works : Utopia (`baroqueCards`, set `"ST03"`, faction `pirate`) — 28 cards

Array order: BW-001…BW-010, **BW-013, BW-014, BW-011, BW-012**, BW-015…BW-024,
**BW-027, BW-028**, BW-025, BW-026.

| id | name | type | cost | rarity | ATK/DEF/PV | traits | tags | row |
|---|---|---|---|---|---|---|---|---|
| BW-001 | Mr. 1 (Daz Bonez) | character | 4 | R | 6/4/7 | piercing | baroque | front |
| BW-002 | Miss Doublefinger | character | 3 | U | 4/2/5 | — | baroque, female | front |
| BW-003 | Mr. 2 (Bon Clay) | character | 3 | U | 4/2/6 | — | baroque | front |
| BW-004 | Mr. 3 (Galdino) | character | 2 | C | 2/3/5 | shield | baroque | front |
| BW-005 | Miss Goldenweek | character | 2 | U | 1/1/4 | — | baroque, female | back |
| BW-006 | Mr. 5 | character | 2 | C | 3/1/4 | — | baroque | front |
| BW-007 | Miss Valentine | character | 2 | C | 3/1/4 | rush | baroque, female | front |
| BW-008 | Mr. 4 | character | 3 | U | 5/3/7 | — | baroque | front |
| BW-009 | Miss Merry Christmas | character | 2 | C | 3/2/5 | — | baroque, female | front |
| BW-010 | Miss All Sunday (Robin) | character | 3 | R | 3/2/5 | range | baroque, female | back |

* **BW-001 Mr. 1** — no passive. base `Lame du Corps` atk 6.
  special `Atomic Spurt` cost 3, atkBonus 3, `ignoreShield`.
  (Its `piercing` **trait** halves target DEF on base and special attacks.)
* **BW-002 Miss Doublefinger** — passive **Épines**: `[meleeRecoil{amount: 2}]`.
  base `Stinger` atk 4. special `Tsubaki` cost 2, atkBonus 3, attackTraits `[piercing]`.
* **BW-003 Mr. 2 (Bon Clay)** — passive **Mane Mane**: `[copyAtkOnDeploy]`.
  base `Coup de Ballet` atk 4. special `Okama Kenpo` cost 2, atkBonus 2, `twoTargets: true`.
* **BW-004 Mr. 3 (Galdino)** — no passive. base `Coup de Cire` atk 2.
  special `Candle Lock` cost 2, atkBonus 2, `immobilize`.
* **BW-005 Miss Goldenweek** — passive **Colors Trap**: `[debuffOneEnemy{amount: 2}]`.
  base `Peinture du Rire` atk 0, `isSupport`, `immobilize: true` (a support action that immobilizes).
  special `Peinture de la Colère` cost 1, atkBonus 0, `isSupport`, description "Un ennemi doit cibler
  Miss Goldenweek à son prochain tour." (**taunt is TEXT-ONLY**; support specials are never offered
  by the valid-action generator).
* **BW-006 Mr. 5** — passive **Corps Explosif**: `[explodeOnKO{amount: 2}]`.
  base `Coup Explosif` atk 3. special `Nose Fancy Cannon` cost 2, atkBonus 2, attackTraits `[zone]`.
* **BW-007 Miss Valentine** — no passive. base `Coup Léger` atk 3.
  special `Tonne Drop` cost 2, atkBonus 3, attackTraits `[impact]`, `pushback: true`.
* **BW-008 Mr. 4** — passive **Quatre Tonnes**: `[immuneImpact]`. base `Coup de Batte` atk 5.
  special `Home Run` cost 3, atkBonus 4, attackTraits `[impact]`, `pushback: true`.
* **BW-009 Miss Merry Christmas** — no passive. base `Coup de Griffe` atk 3.
  special `Mole Attack` cost 2, atkBonus 2, attackTraits `[range]`.
* **BW-010 Miss All Sunday (Robin)** — passive **Vice-Présidente**: `[entryDiscardRandom]`.
  base `Seis Fleur` atk 3. special `Spider Net` cost 2, atkBonus 2, `immobilize`.

**Weapons (declared before the fruits in the array)**

| id | name | cost | rarity | subtype | bonusAtk | restriction | grantsElement | effect |
|---|---|---|---|---|---|---|---|---|
| BW-013 | Crochet Empoisonné | 1 | U | weapon | 1 | — | poison | +1 ATK; bearer's attacks become Poison (1 dmg/turn, permanent). |
| BW-014 | Lassoo | 1 | C | weapon | 1 | Mr. 4 | fire | +1 ATK; attacks become Fire. If destroyed: deploy a token. **(token part TEXT-ONLY)** |

**Devil fruits (subtype `fruit`, `bonusAtk: 0`, `minTurns: 5`)**

* **BW-011 Suna Suna no Mi** — cost 3, SR, restriction `Crocodile`.
  equipEffect "Applique Logia + Maudit."
  base `grantsTraits: [cursed, logia]`; passiveDescription "Logia. Sables : 2 Vol · ATK +3 · Sable
  (-1 PV permanent)."
  awakening: `porteurLegitime "Crocodile"`, `volCost 3`, `atkBonus 5`,
  passiveDescription "Fin de votre tour : tous les ennemis blessés perdent 1 PV permanent.",
  specialAttack `Ground Death` cost 4, atkBonus 5, `oncePerGame`, element `sand`, description
  "La cible perd 3 PV permanent et ne peut plus être soignée." (**no structured permanentPvLoss /
  noHeal — the fruit special shape has no such fields**).
* **BW-012 Supa Supa no Mi** — cost 2, R, restriction `Mr. 1`.
  equipEffect "Applique Perçant + Maudit."
  base `grantsTraits: [cursed, piercing]`; passiveDescription "Perçant. Spartan : 1 Vol · ATK +2 ·
  Perçant."
  awakening: `porteurLegitime "Mr. 1"`, `volCost 2`, `atkBonus 0`,
  passiveDescription "Les attaques ignorent le Bouclier et la DEF.",
  specialAttack `Atomic Spurt` cost 3, atkBonus 3, attackTraits `[total]`, `ignoreShield: true`,
  `ignoreDef: 99` (**no `oncePerGame`**; the name collides with BW-001's special).

**Accessories (`bonusAtk: 0`)**

| id | name | cost | rarity | effect |
|---|---|---|---|---|
| BW-015 | Den Den Mushi Secret | 1 | C | On entry, look at the opponent's hand; bearer's attacks ignore Stealth. **TEXT-ONLY** |
| BW-016 | Bananawani | 2 | U | On the bearer's entry, deploy a Bananawani token (ATK 4 / DEF 1 / PV 4) adjacent. **TEXT-ONLY** (the token def is `TOK-BANANAWANI`, but nothing references it) |
| BW-017 | Poudre Explosive | 1 | C | 1×/game: 3 damage to an enemy (Zone). **TEXT-ONLY** |

**Ships**

* **BW-018 Rain Dinners** — cost 1, U. `shipPassive` "Vos Baroque Works gagnent +1 ATK."
  (`baroque` is a **tag**, and the ship-passive regex gate only special-cases `mugiwara`/`marine`, so
  the `!contains(mugiwara) && !contains(marine)` branch applies → **every** own board character gets
  +1 ATK.) `shipActive`: `Casino`, cost 1, description "Piochez 1 carte puis défaussez 1 carte."
  — **no `oncePerGame`** (repeatable) and it hits the fallback branch (log only).
* **BW-019 Navire Baroque Works** — cost 2, U. `shipPassive` "Vos Baroque Works gagnent +1 PV."
  `shipActive`: `Agents`, cost 2, description "Déployez deux jetons agents.", `oncePerGame` —
  hard-coded branch: `deployToken("TOK-AGENT")` twice.

**Events**

| id | name | cost | rarity | eventEffect |
|---|---|---|---|---|
| BW-020 | Operation Utopia | 3 | R | `damageEnemies{amount: 3, target: all}` |
| BW-021 | Embuscade | 2 | U | `rushBuff{atk: 2}` |
| BW-022 | Contrat d'Assassinat | 1 | C | `custom{id: "execute3", description: "Détruisez un ennemi de PV actuels ≤ 3."}` |
| BW-023 | Tempête de Sable | 3 | U | `damageEnemies{amount: 2, target: all, sand: true}` |
| BW-024 | Trahison | 2 | U | `custom{id: "betrayal", description: "Prenez le contrôle d'un ennemi de coût ≤ 2 ce tour."}` — **log only** |
| BW-027 | Infiltration | 1 | C | `tutor{filterTag: "baroque", maxCost: 3}` |
| BW-028 | Pluie Artificielle | 2 | U | `custom{id: "noHeal2", description: "Persistant (2 tours) : les ennemis ne peuvent pas être soignés."}` — **log only** |

**Counters**

| id | name | cost | rarity | counterEffect |
|---|---|---|---|---|
| BW-025 | « Faible » | 0 | C | `cancel{description: "Annulez une attaque d'un ennemi d'ATK ≤ 4.", maxAttackerAtk: 4}` |
| BW-026 | Mirage du Désert | 1 | C | `untargetable{description: "La cible devient Inciblable jusqu'à la fin du tour."}` — routed to the same handler as `cancel` |

### 5.6 ST04 — Red Hair : L'Empereur (`redhairCards`, set `"ST04"`, faction `pirate`) — 27 cards

Array order: RH-001…RH-023, **RH-026**, RH-024, RH-025, RH-027.

| id | name | type | cost | rarity | ATK/DEF/PV | traits | tags | row |
|---|---|---|---|---|---|---|---|---|
| RH-001 | Ben Beckman | character | 5 | SR | 6/4/8 | — | redhair | front |
| RH-002 | Lucky Roux | character | 4 | R | 6/3/8 | — | redhair | front |
| RH-003 | Yasopp | character | 5 | R | 5/2/6 | range | redhair, tireur | back |
| RH-004 | Rockstar | character | 2 | C | 3/2/4 | — | redhair | front |
| RH-005 | Limejuice | character | 3 | U | 5/3/6 | — | redhair | front |
| RH-006 | Bonk Punch | character | 3 | U | 4/4/7 | shield | redhair | front |
| RH-007 | Howling Gab | character | 3 | C | 3/2/5 | — | redhair | front |
| RH-008 | Building Snake | character | 3 | U | 5/3/6 | — | redhair, bretteur | front |
| RH-009 | Hongo | character | 2 | C | 1/2/4 | — | redhair, medecin | back |

* **RH-001 Ben Beckman** — passive **Observation de Maître**: `[grantObservationAll]`
  (**DATA-ONLY: `haki.ts` never consults it**). base `Tir de Précision` atk 6.
  special `Tir de Maître` cost 3, atkBonus 3, attackTraits `[range]`, `immobilize: true`.
* **RH-002 Lucky Roux** — passive **Quick Draw**: `[noDodge]`. base `Tir` atk 6.
  special `Tir à Bout Portant` cost 2, atkBonus 3, attackTraits `[range]`.
* **RH-003 Yasopp** — passive **Précision Absolue**: `[noDodge, attacksIgnoreShield]`
  (**`attacksIgnoreShield` is DATA-ONLY — combat only reads `PendingAttack.ignoreShield`**).
  base `Tir Précis` atk 5. special `Tir Mortel` cost 3, atkBonus 3, attackTraits `[piercing]`.
* **RH-004 Rockstar** — `synergies: [{partnerId: "CAP-SHANKS", atkBonus: 1}]`
  (**never triggers: `recalculatePassiveBuffs` only scans board `cards`, and captains are not cards**).
  base `Coup Insolent` atk 3. special `Provocation` cost 1, atkBonus 0, `isSupport`, description
  "Un ennemi doit cibler Rockstar à son prochain tour." (**TEXT-ONLY**).
* **RH-005 Limejuice** — `naturalHaki: [armament]`; passive **Haki d'Équipage**:
  `[naturalHaki{hakiType: armament}]`. base `Frappe` atk 5.
  special `Frappe Aguerrie` cost 2, atkBonus 3, attackTraits `[piercing]`.
* **RH-006 Bonk Punch** — no passive. base `Coup de Poing` atk 4.
  special `Charge` cost 2, atkBonus 3, attackTraits `[impact]` — **no `pushback` flag**, so no knockback.
* **RH-007 Howling Gab** — passive **Hurlement**: `[debuffOneEnemy{amount: 1}]`.
  base `Coup Sonore` atk 3. special `Onde de Choc` cost 2, atkBonus 2, attackTraits `[impact]`
  — **no `pushback` flag**.
* **RH-008 Building Snake** — no passive. base `Coup de Canne` atk 5.
  special `Coup Fourbe` cost 2, atkBonus 3, attackTraits `[piercing]`, `ignoreShield: true`.
* **RH-009 Hongo** — passive **Médecin de l'Équipage**: `[healAdjacent{amount: 1}]`.
  base `Soins` atk 0, `isSupport`, `healAmount: 2`.
  special `Stimulant` cost 2, atkBonus 0, `isSupport`, description "Un allié gagne +2 ATK et perd
  gelé/immobilisé." (**TEXT-ONLY**).

**Weapons / accessories**

| id | name | cost | rarity | subtype | bonusAtk | bonusDef | restriction | grantsTraits | effect |
|---|---|---|---|---|---|---|---|---|---|
| RH-010 | Gryphon | 2 | R | weapon | 2 | — | bretteur | — | +2 ATK. Attacks gain Armament Haki (hit Logia) and ignore Shield. If equipped by Shanks: +1 ATK. **(all but bonusAtk TEXT-ONLY)** |
| RH-011 | Fusil de Beckman | 1 | C | weapon | 1 | — | tireur | `[range]` | +1 ATK, attacks gain Range. If equipped by Ben Beckman: +1 ATK. *(the Beckman bonus IS implemented via the `wield_` table; note `getValidTargets` ignores equipment-granted range)* |
| RH-012 | Pistolet de Lucky Roux | 1 | C | weapon | 1 | — | tireur | `[range]` | +1 ATK, attacks gain Range. If equipped by Lucky Roux: attacks are undodgeable. **(second clause TEXT-ONLY)** |
| RH-013 | Fusil de Yasopp | 1 | C | weapon | 1 | — | tireur | `[range]` | +1 ATK, attacks gain Range. If equipped by Yasopp: +1 ATK and Piercing. *(the +1 ATK IS implemented via the `wield_` table; the Piercing clause is TEXT-ONLY)* |
| RH-014 | Chapeau de Paille | 1 | R | accessory | 1 | — | — | — | +1 ATK. The first time the bearer would be KO'd, it survives at 1 PV. *(implemented by defId in `applyCharacterDamage`, tag `"strawhat"`; does NOT protect against element/thunder/spread/recoil KOs)* |
| RH-015 | Sake de la Fête | 1 | U | accessory | 0 | — | — | — | 1×/game: all your allies heal 2 PV and gain +1 ATK this turn. **TEXT-ONLY** |
| RH-016 | Cape de l'Empereur | 2 | R | accessory | 0 | 2 | — | — | +2 DEF. Enemies adjacent to the bearer have −1 ATK. **(the aura is TEXT-ONLY)** |

**Ships**

* **RH-017 Red Force** — cost 2, R. `shipPassive` "Vos personnages gagnent +1 ATK."
  (no faction word ⇒ applies to every own board character). `shipActive`: `Volonté d'Acier`, cost 3,
  description "Ce tour, vos personnages gagnent le Haki Armement et +1 ATK.", `oncePerGame` —
  hard-coded branch: `hakiThisTurn = true` and +1 ATK (turn) to every own board card.
* **RH-018 Navire du Nouveau Monde** — cost 1, C. `shipPassive` "Vos personnages gagnent +1 PV."
  `shipDestroyEffect: {draw: 1}`. No `shipActive`.

**Events**

| id | name | cost | rarity | eventEffect |
|---|---|---|---|---|
| RH-019 | Haki du Roi | 3 | R | `debuffAllEnemies{atk: 2, immobilizeMaxDef: 3}` |
| RH-020 | Festin | 2 | U | `healAllBuff{heal: 3, atk: 1}` |
| RH-021 | Intimidation | 2 | U | `debuffAllEnemies{atk: 2}` |
| RH-022 | Promesse | 1 | C | `buffSingle{stat: atk, amount: 2, duration: permanent}` |
| RH-023 | L'Ère des Rêves | 1 | C | `grantHakiAll{}` (no `atk`) |
| RH-026 | Alliance | 2 | U | `tutor{}` (no `filterTag`, no `maxCost`) *(listed before RH-024/025)* |

**Counters**

| id | name | cost | rarity | counterEffect |
|---|---|---|---|---|
| RH-024 | Courant du Nouveau Monde | 0 | C | `reduceDamage{amount: 3}` |
| RH-025 | Imposer le Respect | 1 | U | `reduceDamage{amount: 3}` (mechanically identical to RH-024, different cost/rarity) |
| RH-027 | Sacrifice du Bras | 3 | R | `cancel{description: "Annulez l'attaque ; votre Capitaine subit 5 dégâts.", selfCaptainDamage: 5}` |

### 5.7 Cross-references the catalogue relies on

* Token ids referenced from card data: `TOK-MARINE` (MR-019 `deployToken`, MR-023 `deployTokens`),
  `TOK-AGENT` (BW-019 ship ability, engine-hard-coded). `TOK-BANANAWANI` is defined but never
  referenced by any structured field.
* Captain id referenced from card data: `CAP-SHANKS` (RH-004 synergy — inert, see above).
* Card ids hard-coded inside the engine: `MG-003`+`MG-004` (Clima-Tact combo), `MG-009`, `MG-010`,
  `MG-012`, `MG-013`, `MG-019`, `MG-021`, `MR-013`, `RH-011`, `RH-013`, `RH-014`, `RH-017`, `BW-019`.
* Tags used by filters: `mugiwara`, `marine`, `baroque`, `redhair`, `female`, `bretteur`, `tireur`,
  `medecin`, `soldat`, `garde`, `capitaine_marine`, `vice_amiral`, `amiral`, `amiral_en_chef`,
  `cuisinier`, `navigateur`, `archeologue`, `charpentier`, `musicien`.

---

## 6. Deck Lists

Four `DeckDef`s in `src/data/decks.ts`; each sums to exactly 50 cards. `verifyDeck` runs at module
load and `console.warn`s `Deck "<name>" has <n> cards (expected 50)` when a deck does not sum to 50 —
it never fires with the shipped data. **The entry order below is significant** because
`createPlayerState` instantiates entries in array order and the instance-id counter follows it.

### 6.1 `mugiwaraDeck` — "Mugiwara - Pre-Ellipse", captain `CAP-LUFFY`

| # | cardId | count | # | cardId | count |
|---|---|---|---|---|---|
| 1 | MG-001 | 3 | 15 | MG-015 | 1 |
| 2 | MG-002 | 3 | 16 | MG-016 | 1 |
| 3 | MG-003 | 3 | 17 | MG-017 | 1 |
| 4 | MG-004 | 3 | 18 | MG-018 | 1 |
| 5 | MG-005 | 3 | 19 | MG-019 | 1 |
| 6 | MG-006 | 2 | 20 | MG-020 | 2 |
| 7 | MG-007 | 2 | 21 | MG-021 | 2 |
| 8 | MG-008 | 2 | 22 | MG-022 | 2 |
| 9 | MG-009 | 1 | 23 | MG-023 | 2 |
| 10 | MG-010 | 1 | 24 | MG-024 | 2 |
| 11 | MG-011 | 1 | 25 | MG-025 | 2 |
| 12 | MG-012 | 1 | 26 | **MG-028** | 2 |
| 13 | MG-013 | 1 | 27 | MG-026 | 2 |
| 14 | MG-014 | 1 | 28 | MG-027 | 2 |

Total 50 (21 characters, 5 weapons, 3 fruits, 3 accessories, 4 ships, 10 events, 4 counters).

### 6.2 `marinesDeck` — "Marines - Justice Absolue", captain `CAP-AKAINU`

| # | cardId | count | # | cardId | count |
|---|---|---|---|---|---|
| 1 | MR-001 | 3 | 15 | MR-015 | 1 |
| 2 | MR-002 | 2 | 16 | MR-016 | 1 |
| 3 | MR-003 | 3 | 17 | MR-017 | 1 |
| 4 | MR-004 | 2 | 18 | MR-018 | 2 |
| 5 | MR-005 | 2 | 19 | MR-019 | 2 |
| 6 | MR-006 | 2 | 20 | MR-020 | 2 |
| 7 | MR-007 | 2 | 21 | MR-021 | 2 |
| 8 | MR-008 | 2 | 22 | MR-022 | 2 |
| 9 | MR-009 | 2 | 23 | MR-023 | 2 |
| 10 | MR-010 | 1 | 24 | MR-024 | 2 |
| 11 | MR-011 | 1 | 25 | **MR-027** | 2 |
| 12 | MR-012 | 1 | 26 | **MR-028** | 2 |
| 13 | MR-013 | 1 | 27 | MR-025 | 2 |
| 14 | MR-014 | 1 | 28 | MR-026 | 2 |

Total 50 (21 characters, 2 fruits, 2 weapons, 3 accessories, 4 ships, 14 events, 4 counters).

### 6.3 `baroqueDeck` — "Baroque Works - Utopia", captain `CAP-CROCODILE`

| # | cardId | count | # | cardId | count |
|---|---|---|---|---|---|
| 1 | BW-001 | 3 | 15 | BW-015 | 1 |
| 2 | BW-002 | 2 | 16 | BW-016 | 1 |
| 3 | BW-003 | 2 | 17 | BW-017 | 1 |
| 4 | BW-004 | 2 | 18 | BW-018 | 2 |
| 5 | BW-005 | 2 | 19 | BW-019 | 2 |
| 6 | BW-006 | 2 | 20 | BW-020 | 2 |
| 7 | BW-007 | 2 | 21 | BW-021 | 2 |
| 8 | BW-008 | 2 | 22 | BW-022 | 2 |
| 9 | BW-009 | 2 | 23 | BW-023 | 2 |
| 10 | BW-010 | 2 | 24 | BW-024 | 2 |
| 11 | **BW-013** | 2 | 25 | **BW-027** | 2 |
| 12 | **BW-014** | 1 | 26 | **BW-028** | 1 |
| 13 | BW-011 | 1 | 27 | BW-025 | 2 |
| 14 | BW-012 | 1 | 28 | BW-026 | 2 |

Total 50 (3 + 2×9 = 21 characters; then 2+1 = 24; +1+1 = 26; +1+1+1 = 29; +2+2 = 33; +2×5 = 43;
+2 = 45; +1 = 46; +2+2 = 50).

### 6.4 `redhairDeck` — "Red Hair - L'Empereur", captain `CAP-SHANKS`

| # | cardId | count | # | cardId | count |
|---|---|---|---|---|---|
| 1 | RH-001 | 2 | 15 | RH-015 | 1 |
| 2 | RH-002 | 3 | 16 | RH-016 | 1 |
| 3 | RH-003 | 2 | 17 | RH-017 | 2 |
| 4 | RH-004 | 3 | 18 | RH-018 | 2 |
| 5 | RH-005 | 2 | 19 | RH-019 | 2 |
| 6 | RH-006 | 3 | 20 | RH-020 | 2 |
| 7 | RH-007 | 2 | 21 | RH-021 | 2 |
| 8 | RH-008 | 2 | 22 | RH-022 | 3 |
| 9 | RH-009 | 2 | 23 | RH-023 | 2 |
| 10 | RH-010 | 1 | 24 | **RH-026** | 2 |
| 11 | RH-011 | 1 | 25 | RH-024 | 2 |
| 12 | RH-012 | 1 | 26 | RH-025 | 2 |
| 13 | RH-013 | 1 | 27 | RH-027 | 1 |
| 14 | RH-014 | 1 | | | |

Total 50 (21 characters, 4 + 3 single-copy objects, 4 ships, 13 events, 5 counters).

---

## 7. Engine ↔ UI Contract

The reference UI is a React hook (`src/hooks/useGameEngine.ts`) plus a presentational component
(`src/components/Game.tsx`) and an announcement builder (`src/lib/announce.ts`). The Rust port must
expose enough to reproduce all of it; a WASM shim serialising `GameState`/`GameAction` as JSON is
sufficient.

### 7.1 Hook state and exposed values

```
useGameEngine(humanDeck, aiDeck, humanPlayer = "player1", difficulty = "intermediate")
  -> { state, validActions, dispatch, isAiTurn, humanPlayer, announcements, dismissAnnouncement }
```

* `state` — created once with `createGame(humanDeck, aiDeck)` (human deck is always player 1).
* `validActions` —
  * `[]` if `state.winner`;
  * `getValidActions(state, humanPlayer)` if `pendingAttack && currentPlayer == aiPlayer`
    (human is defending);
  * `getValidActions(state, humanPlayer)` if `!pendingAttack && currentPlayer == humanPlayer`;
  * `[]` otherwise (AI main phase, or the human's own attack awaiting the AI's answer).
* `isAiTurn = currentPlayer == aiPlayer && !pendingAttack`.
* `dispatch(action)` — **announce first, then apply**: `announce(action, currentState)` then
  `state = executeAction(state, action)` inside a try/catch; on error the state is left unchanged and
  the error is logged as `"Action failed:"` + error. **No validity check against `validActions`.**

### 7.2 State fields the UI reads

| Path | Use |
|---|---|
| `players[p].captain.{defId, flipped, slot, currentPv, tapped}` | command card, verso token, PV bar (`currentPv / side.pv`), tapped styling |
| `players[p].captain.statusEffects` | status pills on the command card (`CaptainCard.tsx:86-88`) and in the captain menu (`CaptainMenu.tsx:89`); first two rungs of the captain-attack disable chain below (`CaptainMenu.tsx:54-56`) |
| `players[p].captain.tapped` | **third** rung of the captain-attack disable chain → `"Incliné"` (as well as the tapped styling noted in the row above) |
| `players[p].captain.deployedTurn` | **fourth** rung of the captain-attack disable chain → `"Vient d'être engagé"` when `deployedTurn == turnNumber` (`CaptainMenu.tsx:56`) |

**Captain-attack disable reason — the exact 4-branch chain** (`CaptainMenu.tsx:56`, first match wins;
`null` = the attack button is enabled):

```ts
const attackReason = isFrozen ? "Gelé !" : isImmob ? "Immobilisé !" : captain.tapped ? "Incliné"
  : captain.deployedTurn === state.turnNumber ? "Vient d'être engagé" : null;
```

with `isFrozen = captain.statusEffects.some(e => e.type === "freeze")` (`CaptainMenu.tsx:54`) and
`isImmob = captain.statusEffects.some(e => e.type === "immobilize")` (`:55`). The `tapped` branch sits
**between** the status branches and `deployedTurn`: omitting it both loses a reason and mislabels a
tapped-but-unfrozen captain (it would fall through to `"Vient d'être engagé"` or to no reason at all).
| `players[p].board[slot]` | board slots (front V1–V3 then back A1–A3; the foe's half is rendered command → back → front) |
| `players[p].activeShip` | ship chip + ship menu |
| `players[p].volonte` | Volonté counter, `min(volonte, 10)` pips |
| `players[p].hand` / `.deck.length` | hand cards and deck count (opponent: counts only) |
| `cards[id].defId` | card art / definition lookup |
| `cards[id].usedOnceAbilities` | ship "already used" indicator (matches `shipActive.name`) — see the **ship-menu activation reason** below for the exact two-branch precedence and the two strings; also the special-attack disable reason `"Déjà utilisé (1x/partie)"` when `def.specialAttack.oncePerGame && usedOnceAbilities.includes(def.specialAttack.name)` |
| `cards[id].currentPv` | board-token HP bar, shown **only when damaged** (`currentPv < def.pv`), width `max(0, currentPv/def.pv*100)%`; full-card PV line; VFX damage diffing |
| `cards[id].tapped` | board-token desaturated/dimmed styling + the `"Incliné"` veil; ActionMenu base-action disable reason `"Incliné"` |
| `cards[id].statusEffects` | status pills on the board token (compact) and in `CardDetail`; `some(type == freeze)` → `"Gelé !"`, `some(type == immobilize)` → `"Immobilisé !"` as ActionMenu base/special disable reasons |
| `cards[id].attachedObjects` | equipment count chip `⚔<len>` on the board token; the equipment chip list in `ActionMenu`/`CardDetail` (each id resolved through `cards[]` + `getCardDef`) |
| `cards[id].usedBaseAction` / `.usedSpecialAttack` | ActionMenu disable reason `"Déjà utilisé ce tour"` (base / special respectively) |
| `cards[id].deployedTurn` | summoning sickness: `deployedTurn == turnNumber && !def.traits.includes("rush")` → disable reason `"Mal de terre"` |
| `cards[id].isAwakened` | `FullCard` uses the **verso** art (`CARD_ART_VERSO[def.id]`) when set and a verso art exists, otherwise `CARD_ART[def.id]` |
| `cards[id].zone` | VFX snapshot: only instances with `zone == "board"` are tracked for PV-loss / KO effects (`useCombatVfx.ts:56-67`) |
| `cards[id].slot` | not read directly by any component (board layout is driven by `players[p].board`), but it is part of the instance payload and must round-trip through serialisation |
| `pendingAttack.{attackerId, targetId, targetIsCaptain, rawDamage, element, hasHaki, isSpecial, cannotBeDodged, ignoreShield}` | counter window text and available reactions; `isSpecial` / `attackerId.startsWith("captain_")` also drive AI pacing |
| `turnNumber`, `winner`, `log` | header, game-over screen, last 12 log lines reversed (`T<turn>`, `►` for the human, `◄` for the foe) |

`phase` and `firstPlayer` are never read by the UI.

**ActionMenu disable-reason chains — the exact two chains** (`ActionMenu.tsx:43-62`). Like the
captain chain above, each is a first-match-wins ladder returning `null` when the button is enabled;
unlike it, the two ladders have **different rung counts and different rungs**, and the port must not
merge them. The locals they read (`ActionMenu.tsx:33-41`):

```ts
const isTapped      = instance.tapped;
const usedBase      = instance.usedBaseAction;
const usedSpecial   = instance.usedSpecialAttack;
const hasSickness   = instance.deployedTurn === state.turnNumber && !(def.traits?.includes("rush"));
const isFrozen      = instance.statusEffects.some((e) => e.type === "freeze");
const isImmobilized = instance.statusEffects.some((e) => e.type === "immobilize");
const playerVol     = state.players[instance.owner].volonte;   // NOT from validActions — see §7.3
const base          = def.baseAction;
```

*Base / support action — **7 rungs**, in this order:*

```ts
const baseReason = (() => {
  if (isFrozen) return "Gelé !";
  if (isImmobilized) return "Immobilisé !";
  if (hasSickness) return "Mal de terre";
  if (isTapped) return "Incliné";
  if (usedBase) return "Déjà utilisé ce tour";
  if (effectiveAtk <= 0 && !base?.isSupport) return "ATK 0";
  if (!canBaseAttack && !canSupport) return "Pas de cible";
  return null;
})();
```

*Special attack — **7 rungs**, and note there is **no `isTapped` rung**:*

```ts
const specReason = (() => {
  if (isFrozen) return "Gelé !";
  if (isImmobilized) return "Immobilisé !";
  if (hasSickness) return "Mal de terre";
  if (usedSpecial) return "Déjà utilisé ce tour";
  if (def.specialAttack?.oncePerGame && instance.usedOnceAbilities.includes(def.specialAttack.name))
    return "Déjà utilisé (1x/partie)";
  if (def.specialAttack && playerVol < def.specialAttack.cost)
    return `Volonté insuffisante (${playerVol}/${def.specialAttack.cost})`;
  if (!canSpecialAttack) return "Pas de cible";
  return null;
})();
```

Three facts a port must not lose:

1. **Order is frozen.** Base: frozen → immobilized → sickness → **tapped** → usedBase → ATK 0 →
   no-target. Special: frozen → immobilized → sickness → usedSpecial → once-per-game → **Volonté** →
   no-target. The special ladder deliberately omits `tapped` (a tapped-but-not-`usedSpecialAttack`
   character still reports the *later* reason), mirroring `declareSpecialAttack`, which does not
   check `tapped` either (§4.6).
2. **`"Pas de cible"` terminates both chains** and is the only reason derived from `validActions`
   (`canBaseAttack` / `canSupport` / `canSpecialAttack`, matched by `attackerInstanceId` /
   `instanceId` against this instance). It is the fallback that fires when the engine simply produced
   no action for this attacker.
3. **The `"ATK 0"` rung is suppressed for support base actions** — the guard is
   `effectiveAtk <= 0 && !base?.isSupport`, using `getEffectiveAtk(state, instance.instanceId)`.
   A 0-ATK support character (e.g. a healer) must show `"Pas de cible"` or be enabled, **never**
   `"ATK 0"`.

The two reasons are handed to `FullCard` as `actions.base.reason` / `actions.special.reason`, with
`disabled: !(canBaseAttack || canSupport)` for base and `disabled: !canSpecialAttack` for special —
so `disabled` and `reason` are computed from **different** inputs and can disagree (a reason string
may be non-null on an enabled button, and `null` on a disabled one). `actions.base` is `undefined`
entirely when `def.baseAction` is absent, and `actions.special` when `def.specialAttack` is absent.
The base button dispatches the **support** handler when `isSupport && canSupport`, otherwise the
base-attack handler.

**Ship-menu activation reason** (`Game.tsx:755-756`, rendered by `ShipMenu.tsx:39`) — a
two-branch precedence, once-per-game **before** affordability:

```ts
const canActivate = uiMode.isYou && validActions.some(
  (a) => a.type === "activateShip" && a.shipInstanceId === uiMode.instanceId);
const used   = def.shipActive?.oncePerGame && inst.usedOnceAbilities.includes(def.shipActive.name);
const reason = used ? "déjà utilisé" : !canActivate ? "Volonté insuffisante" : null;
```

Both strings are lowercase-initial and are **appended into the button label in parentheses**, not
shown separately:

```tsx
⚓ Activer — {active.name}{!canActivate && activateReason ? ` (${activateReason})` : ""}
```

so a spent once-per-game ability renders e.g. `⚓ Activer — Gaon Cannon (déjà utilisé)` and an
unaffordable one `⚓ Activer — Gaon Cannon (Volonté insuffisante)`. Note the parenthetical is gated on
`!canActivate`, so a ship that is `used` but somehow still activatable would show no suffix. The
button exists at all only when `isYou && def.shipActive`, and is `disabled={!canActivate}`.
`"Volonté insuffisante"` here is a **guess**, not a derivation: it is the catch-all for *any* reason
`activateShip` is missing from `validActions` (including "this is not your `activeShip`" or a missing
active ability), so the port must reproduce the mislabel rather than compute a truer reason.

**Serialisation contract.** Every field above must be present in the JSON the WASM shim hands the UI: a
`CardInstance` payload is the full struct of §2.3 — `{instanceId, defId, owner, zone, slot?, tapped,
currentPv, attachedObjects, modifiers, statusEffects, deployedTurn?, usedBaseAction,
usedSpecialAttack, logiaUsedThisTurn?, usedOnceAbilities, isAwakened?}` — and a `CaptainInstance`
payload is `{defId, owner, flipped, currentPv, slot?, tapped, modifiers, statusEffects, deployedTurn?,
usedBaseAction, usedSpecialAttack, usedOnceAbilities}`. The VFX layer diffs *whole state
snapshots* (`{pv per board card, captain pv per player, board id set, pendingAttack signature}`), so a
shim that ships a trimmed projection of the state breaks the animation layer, not just a label.

**Status badge presentation map** (`StatusBadges.tsx:9-21`) — icon + French label per
`StatusEffect.type`; the colour comes from `STATUS_COLOR[type]` (`src/lib/theme.ts:125-137`) with
fallback `#aaa`. All eleven colours, verbatim:

| `type` | icon | label | `STATUS_COLOR` |
|---|---|---|---|
| `burn` | 🔥 | Brûlé | `#E0653C` |
| `poison` | ☠ | Empoisonné | `#8FB84A` |
| `freeze` | ❄ | Gelé | `#5CC6E0` |
| `desiccation` | 🏜 | Dessèchement | `#C2925A` |
| `trap` | 💣 | Piégé | `#E0463F` |
| `immobilize` | 🌸 | Immobilisé | `#E879B0` |
| `sleep` | 💤 | Endormi | `#9B8CE0` |
| `loseAction` | ⏸ | Action perdue | `#C9A82E` |
| `selfKO` | ⌛ | Sursis | `#E0463F` |
| `noStealth` | 👁 | Repéré | `#7FB0E8` |
| `noHeal` | 🚫 | Soins bloqués | `#FF8A80` |

Note `trap` and `selfKO` share the exact same red `#E0463F`; the map is a flat
`Record<string, string>` keyed by the raw type string, with no ordering significance.

Unknown type → `{icon: "•", label: <the raw type string>, color: "#aaa"}`. Each pill renders
`icon`, then the `label` (**omitted in `compact` mode**, which is what board tokens and the command
card use), then the turns counter `turnsRemaining < 0 ? "∞" : String(turnsRemaining)`; the `title`
tooltip is `"<label> · <turnsRemaining> tour(s)"` when `turnsRemaining >= 0` and
`"<label> · permanent"` otherwise. `StatusLegend` reuses the same map (icon + label only, no turns).

### 7.3 Actions the UI dispatches

15 of the 18 variants: `deployCharacter`, `equipObject`, `deployShip`, `baseAttack`, `specialAttack`,
`baseSupportAction`, `playEvent` (**never with `targets`**), `playCounter`, `useShield`, `passCounter`,
`flipCaptain`, `captainAttack` (**never with `isSpecial`**), `useHaki` (only `king` from the captain
menu and `observation` copied verbatim from `validActions`), `activateShip`, `endTurn`.
Never dispatched: `moveCharacter`, `awakenFruit`, `fruitSpecialAttack`, `useHaki{armament}`.

**Confirm-then-dispatch: `playEvent` and `deployShip` are two-step.** Clicking a hand card routes on
`def.type` (`Game.tsx:170-183`): `character` → `selectingSlot`, `object` → `selectingEquipTarget`,
`event` → **`confirmEvent`**, `ship` → **`confirmShip`**. The last two open the shared `EventConfirm`
modal (`Game.tsx:789-807`) with `playerVol = player.volonte`; only `onConfirm` dispatches —
`{type: "playEvent", instanceId}` from `confirmEvent`, `{type: "deployShip", instanceId}` from
`confirmShip` — and both then `resetUI()`. `onCancel` (the backdrop click included) returns to `idle`
with **no** dispatch. There is no equivalent confirmation step for any other action: deploys, equips,
attacks, supports, counters and `endTurn` dispatch directly. The modal's header/verb switches on
`def.type === "ship"`: `⚓ Deployer navire` vs `✦ Jouer evenement`.

Affordance rules — **mostly** derived from `validActions`, with **two documented exceptions where the
UI recomputes affordability itself**. A port that drives everything off `valid_actions` will produce a
different enabled/disabled surface than the reference UI and will never render the two affordability
strings:

* **Exception 1 — `EventConfirm` (`EventConfirm.tsx:40`, `:111`).** The confirm button is gated on a
  locally recomputed `const canAfford = playerVol >= def.cost;` — **`validActions` is not consulted at
  all** (the component never receives it). `onClick={canAfford ? onConfirm : undefined}` and
  `disabled={!canAfford}` (`:111-112`), the button class flips `btn-gold` → `btn-ghost`, and its label
  is `canAfford ? "Confirmer" : "Volonte insuffisante"` (**unaccented `Volonte`** — this is *not* the
  accented `"Volonté insuffisante"` of the ship menu and ActionMenu). The cost chip beside it reads
  `<def.cost> Vol.` with `({playerVol} disponible)` coloured `text-green-400/80` when `canAfford`,
  `text-red-400/80` otherwise. Consequence: this modal can enable a confirm the engine would reject
  (it only compares raw `def.cost`, so it ignores `deployCost` reductions, hand membership and every
  other `playEvent`/`deployShip` precondition of §4.10), and the resulting throw is swallowed by
  `dispatch`'s try/catch as `"Action failed:"`.
* **Exception 2 — `ActionMenu` (`ActionMenu.tsx:39`, `:60`).** The special-attack cost check is
  recomputed from state, not from `validActions`:
  `const playerVol = state.players[instance.owner].volonte;` then, as the 6th rung of the special
  chain of §7.2, ``if (def.specialAttack && playerVol < def.specialAttack.cost) return `Volonté
  insuffisante (${playerVol}/${def.specialAttack.cost})`;`` — an **accented** string that embeds the
  live/required pair, e.g. `Volonté insuffisante (3/5)`. Note it is read from
  `state.players[instance.owner]`, i.e. the **card owner's** Volonté, not `humanPlayer`'s. The same
  `playerVol` is also printed in the menu header as `<playerVol> Vol.`. This rung only affects the
  *reason string*; the button's `disabled` still comes from `validActions` (`!canSpecialAttack`).

Everything else is derived from `validActions`, never recomputed by the UI:
* hand card playable ⇔ some valid action has `instanceId == id` or `objectInstanceId == id`;
* deploy slots ⇔ `deployCharacter` actions for that card; captain-flip slots ⇔ `flipCaptain` actions;
* equip targets ⇔ `equipObject` actions with that `objectInstanceId`;
* support targets ⇔ `baseSupportAction` actions with that `instanceId` **and** a `targetInstanceId`
  (if none exist, the untargeted variant is dispatched immediately);
* attack targets ⇔ the matching attack actions for that attacker; a captain target is encoded as
  `targetInstanceId = "captain_<opponent>"` with `targetIsCaptain = true`;
* Zone highlight ⇔ the chosen attack's `attackTraits` contains `zone` (whole enemy front row);
* counter window is shown ⇔ `pendingAttack != null` **and** `validActions` contains at least one of
  `playCounter | passCounter | useShield | useHaki{observation}`. Buttons are rendered in
  `validActions` order and dispatch the action object verbatim.

### 7.4 Announcements (`PlayAnnouncement`) and pacing

```
PlayAnnouncement { id: u64, side: You|Foe, def_id: Option<String>, instance_id: Option<String>,
                   kind: String /* = action type */, dest_id: Option<String>,
                   caption: String, big: bool, toast: bool }
```
`dest_id` is a DOM anchor key: an `instanceId`, or `captain_<playerId>`.
`side` is decided by `actorOf`: during a pending attack, `playCounter | useShield | passCounter` and
`useHaki{observation}` are attributed to `opponent(currentPlayer)`; everything else (including
`useHaki{armament|king}`) to `currentPlayer`.

`buildAnnouncement(action, PRE-execution state, humanPlayer)` — a card lookup failure yields `None`
(the id counter still advances). Captions (French, exact):

| kind | defId | dest | big | toast | caption |
|---|---|---|---|---|---|
| deployCharacter | attacker/card | own instanceId | ✓ | | `Déploie <name> en <slot>` |
| deployShip | card | — | ✓ | | `Déploie le navire <name>` |
| equipObject | object | target instanceId | ✓ | | `Équipe <name>` |
| playEvent | card | — | ✓ | | `describeEvent(eventEffect)` or the card name |
| playCounter | card | — | ✓ | | `describeCounter(counterEffect)` or the card name |
| useShield | blocker | blocker | ✓ | | `Bloque avec <name> (Bouclier)` |
| activateShip | ship | — | ✓ | | `Active <shipActive.name ?? name>` |
| awakenFruit | fruit | — | ✓ | | `Éveille <name> !` |
| baseAttack | attacker | attacker | | | `<name> attaque` |
| specialAttack | attacker | attacker | | | `Spéciale : <specialAttack.name ?? name>` |
| fruitSpecialAttack | fruit | attacker | | | `Éveil : <awakening.specialAttack.name ?? name>` |
| moveCharacter | card | own instanceId | | | `Déplace <name>` |
| flipCaptain | — | `captain_<actor>` | ✓ | ✓ | `Retourne son Capitaine !` |
| captainAttack | — | `captain_<actor>` | | ✓ | `Le Capitaine attaque` |
| useHaki | — | — | ✓ | ✓ | king `👑 Haki des Rois !` / observation `Esquive (Haki Observation)` / else `Haki` |
| passCounter | — | — | | ✓ | foe `L'adversaire encaisse` / you `Vous encaissez` |
| endTurn | — | — | | ✓ | `Fin de tour` |
| baseSupportAction | — | — | — | — | **`None` — no announcement at all** |

`describeEvent` (exact strings): `gainWill` → `Gagne +<n> Volonté.`; `draw` →
`Pioche <n>[, défausse <d>].`; `healAlly` → `Tous les alliés +<n> PV.` / `1 allié +<n> PV.`;
`buffAllies` → `Alliés +<n> <STAT>.`; `rally` → `Ralliement : +ATK/+DEF et soin.`;
`buffSingle` → `Un allié +<n> <STAT>.`; `rushBuff` → `Un allié gagne Rush +<n> ATK.`;
`damageEnemies` → `<n> dégâts (<target>).`; `tutor` → `Cherche un personnage.`;
`deployTokens` → `Déploie <n> jeton(s).`; `healAllBuff` → `Soigne <h> et +<a> ATK.`;
`debuffAllEnemies` → `Ennemis −<n> ATK.` (U+2212); `grantHakiAll` → `Vos attaques gagnent le Haki.`;
`dodgeAll` → `Esquive totale ce tour.`; default → the `custom` description.
`describeCounter`: `reduceDamage` → `Réduit les dégâts de <n>.`; `survive` → `Survit avec 1 PV.`;
`cancel` → `Annule l'attaque.`; `untargetable` → `Devient inciblable.`

**A second, independent event-description table: `EFFECT_DESCRIPTIONS`** (`EventConfirm.tsx:12-35`).
The play-confirmation modal of §7.3 does **not** call `describeEvent` — it has its own map, with
different strings (**unaccented**), a `targets` sub-map, only **7** handlers, and a
`JSON.stringify` fallback:

```ts
const description = effect
  ? (EFFECT_DESCRIPTIONS[effect.type]?.(effect) ?? JSON.stringify(effect))   // :43
  : "Effet inconnu";
```

| `eventEffect.type` | `EFFECT_DESCRIPTIONS` string (exact) |
|---|---|
| `gainWill` | `Gagne +<amount> Volonte ce tour.` |
| `draw` | `Pioche <amount> carte(s)` + (`, defausse <discard>` when `discard` is truthy) + `.` |
| `healAlly` | `allAllies` → `Tous les allies +<amount> PV.` ; else `1 allie +<amount> PV.` |
| `buffAllies` | `Tous les allies +<amount> <STAT> (<D>).` — `<STAT>` = `stat.toUpperCase()`; `<D>` = `ce tour` when `duration === "turn"`, else `permanent` |
| `damageEnemies` | `<amount> degats a <T>.` with `T` from the local `targets` map: `allFront` → `toute la Ligne Avant ennemie`, `allCursed` → `tous les Maudits ennemis`, `single` → `1 ennemi`; unknown key falls back to the raw `eff.target` string |
| `dodgeAll` | `Tous vos personnages esquivent toutes les attaques ce tour.` (ignores the effect payload) |
| `custom` | `(e as {description: string}).description` — the raw `custom` description, verbatim |

**The other 8 of the 15 `EventEffect` variants have no handler here** — `rally`, `buffSingle`,
`rushBuff`, `tutor`, `deployTokens`, `healAllBuff`, `debuffAllEnemies`, `grantHakiAll` all fall
through `?.` to `JSON.stringify(effect)` and are rendered to the player as **raw JSON** (e.g.
`{"type":"tutor","filter":{"faction":"pirate"}}`). This is a reference-UI wart, not a gap in this
spec: a zero-drift port must render the same JSON blob. `JSON.stringify` here means TS/JS key order
(insertion order of the object literal in `src/data/cards/*.ts`), no whitespace, and `undefined`
fields omitted.

Two further differences from `describeEvent`: an event card with **no** `eventEffect` sets
`description = "Effet inconnu"` (`:44`) — though that string is never actually shown, because the
whole effect block is gated on `def.eventEffect` (`:70`) — and none of these strings carries accents,
unlike every string in the `describeEvent` table above. The same modal also renders, when present: the counter block —
`reduceDamage` → `Reduit les degats de <amount>.`, otherwise `counterEffect.description` verbatim —
and, for ships, `def.shipPassive` verbatim plus
`<shipActive.name> (<shipActive.cost>V) — <shipActive.description>`.

**Reveal durations** (`announceDuration`, ms; checked in this order):

| condition | ms |
|---|---|
| kind `specialAttack` or `fruitSpecialAttack` | 1700 |
| kind `captainAttack` | 1600 |
| `toast` && kind `endTurn` | 550 |
| `toast` (flipCaptain, useHaki, passCounter) | 750 |
| `!big` (baseAttack, moveCharacter) | 700 |
| otherwise (all big card reveals) | 1150 |

`announce()` appends the announcement and sets
`busyUntil = max(busyUntil, now) + announceDuration(ann)` so reveals queue back-to-back.
Announcements are removed only via `dismissAnnouncement(id)` (called by the reveal layer).

### 7.5 AI loop timing

A derived mode drives a single timer:

| mode | condition |
|---|---|
| `none` | `winner` set; or the human is defending and has a real counter option; or the human's main phase |
| `ai_defend` | `pendingAttack && currentPlayer == humanPlayer` (the AI must answer) |
| `auto_pass` | `pendingAttack && currentPlayer == aiPlayer` and the human's valid actions contain nothing but `passCounter` |
| `ai_turn` | `currentPlayer == aiPlayer && !pendingAttack` |

```
attackPause = if pendingAttack { if isSpecial || attackerId.starts_with("captain_") {1000} else {750} } else {0}
baseDelay   = if mode == ai_turn {650} else {attackPause}      // the TS `|| 700` fallback is unreachable
wait        = max(baseDelay, busyUntil - now)
```
On fire: re-read the latest state; abort if `winner`; choose the action
(`auto_pass` → `passCounter`; `ai_defend` → `aiChooseAction(state, aiPlayer, difficulty)` with
`passCounter` as the catch-all; `ai_turn` → `aiChooseAction(...)` with `endTurn` as the catch-all);
`announce(action, state)`; then apply with a fallback of `passCounter` when a pending attack exists,
else `endTurn`; if both throw, the state is returned unchanged (and the loop stalls — see §8).
So AI actions land at ≥ 650 ms intervals stretched by outstanding reveals, and reactions at
≥ 750/1000 ms.

### 7.6 Errors surfaced to the UI

All engine errors are plain exceptions in TS; the UI catches them at `dispatch` and logs
`"Action failed:"`. A Rust port should return `Result` and let the driver decide; the exact message
strings are listed in §4 and should be preserved for parity testing.

---

## 8. Open Questions / Decisions for the Port

Each item is a place where the TS engine is inconsistent, buggy, or under-specified. The default for
a zero-drift port is **"reproduce as-is"**; the alternative is noted where a fix is obviously intended.

### 8.1 Confirmed bugs (reproduce or fix — decide once, document the choice)

1. **Synergy rage is dead.** `applyOnKOEffects` pushes `synergy_rage_<defId>` modifiers and then calls
   `recalculatePassiveBuffs`, which strips every `source.startsWith("synergy_")`. `onPartnerKO` never
   has any effect.
2. **Vantardise is fragile.** `startTurnBuffAlly` uses `source: passive_<id>`, which the same strip
   removes on the next deploy/equip/KO/flip/awaken of that turn.
3. **Fruit base stat bonuses are gated on `grantsTraits`.** `applyFruitBaseEffects` nests the
   `atkBonus`/`defBonus` pushes inside `if (base.grantsTraits)`. A fruit with bonuses but no granted
   traits gets nothing. (All shipped fruits have `grantsTraits`, so this is currently latent.)
4. **`selfBuffOnAllyKO` can overshoot `max`.** The check is `current < max` *before* adding `amount`.
5. **`deployShip` `healAll` can lower PV.** `min(cur + healAll, printed pv)` reduces a unit whose
   `currentPv` exceeds its printed PV (possible after a `shipdep_pv` bonus). Same pattern in
   `healAdjacent`, `rally`, `healAllBuff`, support `healAmount`.
6. **`canFlipCaptain` vs `flipCaptain` disagree.** With no `cost` and no free condition the predicate
   says *no* but `flipCaptain` flips for free (`cost ?? 0`, `canAfford(0)` is true).
7. **`RH-004`'s `CAP-SHANKS` synergy never fires** (captains are not board `cards`).
8. **`entryEffect damageEnemies single` grants the KO bonus to the wrong side**
   (`grantKOBonus(opponentId)` — the victim's owner — rather than the flipping player's opponent
   convention used elsewhere; note combat also gives the +2 to the *loser*, so this may be intended).
9. **`getValidTargets` range is inconsistent with stealth.** Range is read from `CardDef.traits` /
   `attackTraits` directly (so fruit- and equipment-granted range is ignored), while stealth uses
   `hasTrait` (so fruit-granted stealth counts). `RH-011/012/013` grant `range` via
   `CardDef.grantsTraits`, which `hasTrait` also ignores — equipment-granted range is entirely inert.
10. **Ship-passive faction gating is inconsistent**: `deployCost` matches `def.faction == marine`,
    `recalculatePassiveBuffs` matches the `marine` **tag**. `BW-018`/`BW-019` say "Baroque Works" but
    neither word is special-cased, so their buff applies to *all* own characters.
11. **`activateShipAbility` branch 4** tests the lowercased description for `"deg."` but extracts the
    number from the original-case string (`/(\d+)\s*deg/`), so a capitalised "Deg" yields 0 damage;
    that branch also never runs `sweepKOs`, leaving 0-PV cards on the board.
12. **Trap KO at declaration** removes the attacker with **no KO bonus and no on-KO effects**, unlike
    every other KO path. In `declareSpecialAttack` the trap fires before the spec/afford checks, so a
    later throw silently discards the trap consumption.
13. **`applyShieldBlock` validates almost nothing** — not adjacency, not ownership, not that the
    blocker is on the board or is not itself the target; it recomputes damage from `attackPower`,
    discarding earlier `reduceDamage` reductions and the special's `ignoreDef`, and it never applies
    the attacker's `def.traits` piercing.
14. **`applyCounterReduce`'s `captainBonus` replaces rather than adds** (despite the name);
    `captainBonus == 0` is falsy and falls back to `amount`. Same replace-not-add pattern for
    `cursedBonus` in `damageEnemies` and captain entry effects.
15. **`applyCounterSurvive` tags "survived" immediately**, before the attack resolves; if the attack is
    then cancelled or shield-blocked the tag persists and `survivePlayed` protects the *blocker*.
16. **`applyCaptainDamage`'s `survivePlayed` branch is unreachable** (survive throws on captain targets;
    shield block clears `targetIsCaptain`).
17. **`endTurn`'s recto/verso passive selection is dead code** (the desiccation branch requires
    `flipped`).
18. **`applyElementEffects` returns early when the primary target left the board**, so thunder never
    propagates off a killing blow. Thunder also uses the *reduced* `rawDamage` while Zone/Total spread
    uses the *unreduced* `attackPower`.
19. **AI: `captainAttack` can never score the KO bonus** (it has no `attackerInstanceId`), and
    `fruitSpecialAttack` falls to the default score 0 (below `moveCharacter`'s 2).
20. **AI: `scoreEquipObject` dereferences the instance without a null check** and can throw; in
    `chooseExpert` that call is outside the try/catch, so it aborts the AI turn.
21. **AI loop stall**: if the primary action *and* the fallback both throw, the state is unchanged, the
    version does not bump and the timer is never re-armed.

### 8.2 Under-specified semantics that must be decided

22. `Modifier.turnsRemaining` and `duration == "nextTurn"` are never decremented or expired anywhere.
    Either implement them or drop them from the model.
23. `Modifier.stat == "haki"` is in the type union but no code emits or reads it.
24. `StatusEffect.turnsRemaining == 0` still deals its damage once and is then dropped; is 0 a legal
    input? Is `-1` legal for statuses other than `poison`/`trap`?
25. The poison floor (`currentPv = 1`) is applied per effect, so a `burn` later in the same list can
    still kill through it.
26. `deployCost` floors at 1, so a cost-0 character costs 1 Volonté.
27. `deployCost` dereferences `cards[id]` for board ids without a null check (throws on a dangling id)
    while `getBoardCharacters` tolerates it. Pick one policy.
28. `equipObject` never enforces `CardDef.restriction`, never checks that the target is a character
    (an active ship instance passes the `zone == board` test), and its `wield_` modifier is permanent
    with no unequip path.
29. `moveCharacter` does not update attached objects' `slot`, does not block movement while
    frozen/immobilised/tapped, does not recalculate adjacency-based passives and does not log.
30. `removeFromBoard` on an active ship graveyards it without clearing `player.activeShip` (the **only** code path that does clear it is `damageEnemies{destroyShips}` — MR-020 Buster Call, §3.2); it also
    leaves `modifiers`, `statusEffects`, `currentPv` and used-flags on the graveyard instance, and
    `deployCharacter` does not reset them — a bounce/revive path would resurrect stale state.
31. Captain slot occupancy is expressed only via `captain.slot`; `board[slot]` stays `null`, so nothing
    stops a later deploy into the captain's slot (`flipCaptain` only refuses an already-occupied slot).
32. `flipCaptain` spends the Volonté before the lethal-flip check and skips the entry effect on a
    lethal flip.
33. `flipCondition.freeIfEnemyCursed` / `freeIfAlliesGte` / `freeIfTurnGte` are declared and populated
    (Akainu / Crocodile / Shanks) but **never read**: those captains only ever flip by paying.
34. Captain **special** attacks, `surcharge` and `recto.attacks` are entirely unimplemented;
    `GameAction::CaptainAttack.is_special` is ignored, and `recto.attacks` is `[]` in all data.
35. `declareCaptainBaseAttack` ignores `baseAction.atk` (uses `verso.atk` + captain ATK modifiers),
    never checks captain status effects, applies no piercing/ignoreDef and sets no control flags.
36. `EventEffect::DamageEnemies` with `target: Single` is implemented identically to `All`;
    `HealAlly` without `allAllies` is a no-op; `DodgeAll` is a no-op; `BuffAllies.filter` is ignored;
    `sand` is approximated as +1 damage rather than a permanent max-PV loss.
37. `custom` event ids `embargo`, `betrayal`, `noHeal2` are log-only; the set of legal `custom` ids is
    implicit (`execute3`, `execute4`, `coordinatedFire` + the three unimplemented ones).
38. `SpecialAttack.permanentPvLoss`, `noHeal`, `isSupport`, `healAmount` and `pushbackSlots` are never
    honoured by combat; `AttackTrait::Impact` is never read (pushback comes from
    `PendingAttack.pushback` only); `BaseAction.cannotBeDodged` is never **read**.
    Symmetrically — and this is the omission the rest of the UNREACHABLE audit would otherwise miss —
    `SpecialAttack.cannotBeDodged` is never **populated**: it *is* read (`combat.ts:322`,
    `cannotBeDodged: spec.cannotBeDodged || attackerNoDodge(state, attackerInstanceId)`) but
    `grep -c cannotBeDodged src/data/cards/*.ts` is **0** for all seven card files, so with the
    shipped catalogue `PendingAttack.cannotBeDodged` on a special always equals `attackerNoDodge(...)`
    exactly, and the branch carries zero parity signal in the §1.2 differential harness. See the
    UNREACHABLE note at the end of `declareSpecialAttack` in §4.6. Between the two, **no `cannotBeDodged`
    field on any *action* definition affects the shipped game**: the base one is dead on the read side,
    the special one dead on the write side.
39. `SpecialAttack.transform` ignores `def`/`pv` and grants no Rush despite the type comment; the ATK
    modifier can only raise ATK (`max(0, t.atk - current)`).
40. Element coverage on captains is limited to `fire` and `ice` ("simplified for MVP"); sand/water/
    thunder/poison do nothing to a captain. Captain Logia is unlimited (no once-per-turn), and only
    `verso.traits` is consulted (top-level `CaptainDef.traits` is ignored for Logia).
41. `Trait::Conqueror` is documented as "T10+" only in a comment; the numeric gate lives in
    `HAKI_THRESHOLDS.king = 10`. `hasConquerorInPlay` ignores fruit/equipment-granted conqueror and
    treats top-level captain traits as active on both faces while `verso.traits` only counts when flipped.
42. Haki costs no Volonté anywhere, yet `hakiCostReduction` exists. `armamentUsed` is never read or
    written. `naturalHaki` exists in two forms (`CardDef.naturalHaki[]` and the `naturalHaki` passive);
    only the array form is read by combat.
43. `useObservationHaki` does not verify that the caller is the defender, and the UI hook always passes
    `currentPlayer` (the attacker) to `handleHaki` — verify against the real defender in the port.
44. `checkWinCondition` is only called from `resolveAttack`, `applyCounterCancel`, `flipCaptain` and the
    Gaon-Cannon captain branch. Deck-out sets `winner` inside `drawCard` and nothing re-checks it;
    burn/poison damage to a captain during `startTurn` is not win-checked.
45. `GameState.firstPlayer` is documented as alternating but is always `player1`; `turnNumber` is a
    round counter (increments after player 2) although the type comment describes a per-turn counter.
    Every turn threshold in the game uses the round counter.
46. `Slot → Row` mapping (`V*` = front, `A*` = back) is not declared in the types; it is only implied by
    `FRONT_SLOTS`/`BACK_SLOTS`. Adjacency is orthogonal with no diagonals.
47. `CardDef.grantsTraits` mixes `Trait` and `AttackTrait` literals (they overlap on `range`/`piercing`)
    and is never read by the engine. Either merge into one 11-value enum or keep the wrapper, but note
    the data is currently inert.
48. `fruitEffects.awakening.specialAttack` is a *different* shape from `SpecialAttack` (required
    `description`, missing nine fields). Keep it as a separate struct.
49. `PassiveEffect::Custom` carries only `id` (no description) and no id is handled anywhere.
50. `usedOnceAbilities` is a `Vec<String>` whose keys are ad-hoc: `SpecialAttack.name`,
    `ShipActive.name`, `"survived"`, `"strawhat"`. Consider a typed key in the port, but keep the
    string values if any UI matches on them (the ship menu does: it compares `shipActive.name`).
51. Optional booleans (`logiaUsedThisTurn`, `isAwakened`, `allyKOedThisTurn`, `charKOedThisGame`,
    `hakiThisTurn`, every `targetIsCaptain?`/`isSpecial?`) are `undefined | true | false` in TS; the
    engine never distinguishes `undefined` from `false`, so `bool` with default `false` is safe —
    but serialisation of an absent field must not become `false` if the UI ever tests presence.
52. `PlayerState` has no banished pile; banished cards exist only as `cards[id].zone == Banished`.
53. `src/data/cards/index.ts` builds a *second* `cardRegistry`/`captainRegistry` at import time that the
    engine never reads. The port should have exactly one registry.
54. Free-text parsing (`shipPassive`, `shipActive.description`, `baseAction.description` containing
    `"piege"`) is string-fragile. Either reproduce the exact lowercase/substring/regex logic or migrate
    the card data to structured fields **in lockstep** with the engine — never one without the other.
55. `Math.floor(def / 2)` on a negative DEF rounds toward −∞ and increases damage; DEF is never clamped
    at 0 by the piercing step (only `getEffectiveDef` clamps its own result at 0, but captain DEF —
    face DEF plus modifiers — is not clamped).
56. AI tie-breaking is "first action in `getValidActions` order wins" (strict `>` everywhere) — any
    reordering of the generator changes AI play even with identical scores.
57. The AI is offered actions whose execution can throw (`playEvent` of `MG-024` without a prior own KO;
    equipment whose slot cap is full; ships that are not the active ship). The expert lookahead scores
    those `-Infinity`; the intermediate AI can pick one and lose its turn to the catch.
58. `announce` runs before `executeAction`, so a rejected action still produces a reveal and still
    extends the AI's pacing window.
59. `baseSupportAction` produces no announcement, so support actions never pace the AI loop.
60. Log strings mix accented and unaccented spellings deliberately (`dégâts` vs `degats`, `allié` vs
    `allie`, `éveille` vs `eveille`) and contain emoji/typographic characters (`🛡`, `⚠`, `⭐`, `👑`,
    `≤`, `—`, `−`). Preserve them byte-for-byte if logs are compared in tests.
