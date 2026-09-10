//! Core type definitions — a 1:1 port of `src/types/index.ts`.
//!
//! Naming conventions (kept identical across the whole crate so later modules
//! can be diffed against the TypeScript source):
//! - every TS `interface` is a `struct` with `snake_case` fields and
//!   `#[serde(rename_all = "camelCase")]` so the JSON shape is byte-compatible
//!   with the TS engine;
//! - every TS string-literal union is an `enum` whose variants carry an
//!   explicit `#[serde(rename = "<exact TS literal>")]`;
//! - every TS discriminated union (`{ type: "..." }`) is an enum with
//!   `#[serde(tag = "type")]`;
//! - TS optional fields (`foo?: T`) are `Option<T>` and are omitted from the
//!   JSON when `None` (`skip_serializing_if`), exactly like an absent TS key.
//!
//! Numbers: TS `number` is mapped to `i32` for stats / costs / amounts (they
//! can go negative through modifiers and damage) and to `u32` for turn
//! numbers and card counts. The one exception is `deployedTurn`
//! (`CardInstance` / `CaptainInstance`), which TS overwrites with the sentinel
//! `-1` to clear summoning sickness: it is an `Option<i64>` so the sentinel
//! round-trips through JSON exactly as the TS engine writes it.

use serde::{Deserialize, Serialize};

// ============================================================
// --- Enums & Literal Types ---
// ============================================================

/// TS `PlayerId = "player1" | "player2"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PlayerId {
    #[serde(rename = "player1")]
    Player1,
    #[serde(rename = "player2")]
    Player2,
}

impl PlayerId {
    /// TS `getOpponent(playerId)` (gameState.ts).
    pub fn opponent(self) -> PlayerId {
        match self {
            PlayerId::Player1 => PlayerId::Player2,
            PlayerId::Player2 => PlayerId::Player1,
        }
    }

    /// The exact TS literal (`"player1"` / `"player2"`).
    pub fn as_str(self) -> &'static str {
        match self {
            PlayerId::Player1 => "player1",
            PlayerId::Player2 => "player2",
        }
    }
}

/// TS `CardType = "character" | "object" | "ship" | "event" | "counter"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CardType {
    #[serde(rename = "character")]
    Character,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "ship")]
    Ship,
    #[serde(rename = "event")]
    Event,
    #[serde(rename = "counter")]
    Counter,
}

/// TS `ObjectSubtype = "weapon" | "fruit" | "accessory"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectSubtype {
    #[serde(rename = "weapon")]
    Weapon,
    #[serde(rename = "fruit")]
    Fruit,
    #[serde(rename = "accessory")]
    Accessory,
}

/// TS `Element = "fire" | "water" | "thunder" | "ice" | "sand" | "poison"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Element {
    #[serde(rename = "fire")]
    Fire,
    #[serde(rename = "water")]
    Water,
    #[serde(rename = "thunder")]
    Thunder,
    #[serde(rename = "ice")]
    Ice,
    #[serde(rename = "sand")]
    Sand,
    #[serde(rename = "poison")]
    Poison,
}

/// TS `Trait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Trait {
    /// S'incline pour bloquer pour 1 adjacent
    #[serde(rename = "shield")]
    Shield,
    /// Attaque depuis l'Arriere, cible libre
    #[serde(rename = "range")]
    Range,
    /// Inciblable tant qu'un allie non-Furtif existe
    #[serde(rename = "stealth")]
    Stealth,
    /// Ignore le mal de terre (attaques seulement)
    #[serde(rename = "rush")]
    Rush,
    /// Porteur de Fruit du Demon, vulnerable Eau/Granit
    #[serde(rename = "cursed")]
    Cursed,
    /// Intangibilite: ignore degats sans Haki/Eau/Granit
    #[serde(rename = "logia")]
    Logia,
    /// Toutes les attaques divisent DEF par 2
    #[serde(rename = "piercing")]
    Piercing,
    /// Acces au Haki du Roi (T10+)
    #[serde(rename = "conqueror")]
    Conqueror,
}

/// TS `AttackTrait = "range" | "piercing" | "zone" | "total" | "impact"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttackTrait {
    #[serde(rename = "range")]
    Range,
    #[serde(rename = "piercing")]
    Piercing,
    #[serde(rename = "zone")]
    Zone,
    #[serde(rename = "total")]
    Total,
    #[serde(rename = "impact")]
    Impact,
}

/// TS `(Trait | AttackTrait)` — element type of `CardDef.grantsTraits`.
///
/// Serialised untagged, so `"range"` / `"piercing"` (present in both unions)
/// deserialise as the `Trait` arm, exactly like TS narrowing would.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GrantedTrait {
    Trait(Trait),
    AttackTrait(AttackTrait),
}

/// TS `HakiType = "observation" | "armament" | "king"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HakiType {
    #[serde(rename = "observation")]
    Observation,
    #[serde(rename = "armament")]
    Armament,
    #[serde(rename = "king")]
    King,
}

/// TS `Slot = "V1" | "V2" | "V3" | "A1" | "A2" | "A3"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Slot {
    #[serde(rename = "V1")]
    V1,
    #[serde(rename = "V2")]
    V2,
    #[serde(rename = "V3")]
    V3,
    #[serde(rename = "A1")]
    A1,
    #[serde(rename = "A2")]
    A2,
    #[serde(rename = "A3")]
    A3,
}

impl Slot {
    /// TS `ALL_SLOTS` (utils.ts) — same order.
    pub const ALL: [Slot; 6] = [Slot::V1, Slot::V2, Slot::V3, Slot::A1, Slot::A2, Slot::A3];
    /// TS `FRONT_SLOTS` (utils.ts).
    pub const FRONT: [Slot; 3] = [Slot::V1, Slot::V2, Slot::V3];
    /// TS `BACK_SLOTS` (utils.ts).
    pub const BACK: [Slot; 3] = [Slot::A1, Slot::A2, Slot::A3];

    /// TS `ADJACENCY[slot]` (utils.ts) — same order as the TS arrays.
    pub fn adjacency(self) -> &'static [Slot] {
        match self {
            Slot::V1 => &[Slot::V2, Slot::A1],
            Slot::V2 => &[Slot::V1, Slot::V3, Slot::A2],
            Slot::V3 => &[Slot::V2, Slot::A3],
            Slot::A1 => &[Slot::A2, Slot::V1],
            Slot::A2 => &[Slot::A1, Slot::A3, Slot::V2],
            Slot::A3 => &[Slot::A2, Slot::V3],
        }
    }

    /// `true` when `other` is listed in `ADJACENCY[self]`.
    pub fn is_adjacent_to(self, other: Slot) -> bool {
        self.adjacency().contains(&other)
    }

    /// Which row (`Row`) this slot belongs to (`V*` = front, `A*` = back).
    pub fn row(self) -> Row {
        match self {
            Slot::V1 | Slot::V2 | Slot::V3 => Row::Front,
            Slot::A1 | Slot::A2 | Slot::A3 => Row::Back,
        }
    }

    /// The exact TS literal (`"V1"`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::V1 => "V1",
            Slot::V2 => "V2",
            Slot::V3 => "V3",
            Slot::A1 => "A1",
            Slot::A2 => "A2",
            Slot::A3 => "A3",
        }
    }
}

/// TS `Row = "front" | "back"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Row {
    #[serde(rename = "front")]
    Front,
    #[serde(rename = "back")]
    Back,
}

/// TS `Phase = "untap" | "draw" | "willGain" | "main" | "end"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    #[serde(rename = "untap")]
    Untap,
    #[serde(rename = "draw")]
    Draw,
    #[serde(rename = "willGain")]
    WillGain,
    #[serde(rename = "main")]
    Main,
    #[serde(rename = "end")]
    End,
}

/// TS `Faction = "pirate" | "marine" | "revolutionary" | "independent"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Faction {
    #[serde(rename = "pirate")]
    Pirate,
    #[serde(rename = "marine")]
    Marine,
    #[serde(rename = "revolutionary")]
    Revolutionary,
    #[serde(rename = "independent")]
    Independent,
}

/// TS `CardDef.rarity: "C" | "U" | "R" | "SR" | "L" | "CAP"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    #[serde(rename = "C")]
    C,
    #[serde(rename = "U")]
    U,
    #[serde(rename = "R")]
    R,
    #[serde(rename = "SR")]
    Sr,
    #[serde(rename = "L")]
    L,
    #[serde(rename = "CAP")]
    Cap,
}

// ------------------------------------------------------------
// Small literal unions used inside effect payloads
// ------------------------------------------------------------

/// TS `"atk" | "def" | "pv"` (PassiveEffect.buffAlly `stat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuffStat {
    #[serde(rename = "atk")]
    Atk,
    #[serde(rename = "def")]
    Def,
    #[serde(rename = "pv")]
    Pv,
}

/// TS `"atk" | "def"` (startTurnBuffAlly / EventEffect.buffAllies / buffSingle / EntryEffect.buffAllies `stat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AtkDefStat {
    #[serde(rename = "atk")]
    Atk,
    #[serde(rename = "def")]
    Def,
}

/// TS `"atk"` (PassiveEffect.selfBuffOnAllyKO `stat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AtkStat {
    #[serde(rename = "atk")]
    Atk,
}

/// TS `"bonusWill"` (PassiveEffect.onAllyKO `effect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OnAllyKoEffectKind {
    #[serde(rename = "bonusWill")]
    BonusWill,
}

/// TS `"turn" | "permanent"` (EventEffect.buffAllies / buffSingle `duration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuffDuration {
    #[serde(rename = "turn")]
    Turn,
    #[serde(rename = "permanent")]
    Permanent,
}

/// TS `"turn"` (EntryEffect.buffAllies `duration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TurnDuration {
    #[serde(rename = "turn")]
    Turn,
}

/// TS `"allFront" | "allCursed" | "all" | "single"` (EventEffect.damageEnemies `target`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageTarget {
    #[serde(rename = "allFront")]
    AllFront,
    #[serde(rename = "allCursed")]
    AllCursed,
    #[serde(rename = "all")]
    All,
    #[serde(rename = "single")]
    Single,
}

/// TS `"allFront" | "single"` (EntryEffect.damageEnemies `target`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryDamageTarget {
    #[serde(rename = "allFront")]
    AllFront,
    #[serde(rename = "single")]
    Single,
}

/// TS `Modifier.stat: "atk" | "def" | "pv" | "haki"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModifierStat {
    #[serde(rename = "atk")]
    Atk,
    #[serde(rename = "def")]
    Def,
    #[serde(rename = "pv")]
    Pv,
    #[serde(rename = "haki")]
    Haki,
}

/// TS `Modifier.duration: "permanent" | "turn" | "nextTurn"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModifierDuration {
    #[serde(rename = "permanent")]
    Permanent,
    #[serde(rename = "turn")]
    Turn,
    #[serde(rename = "nextTurn")]
    NextTurn,
}

/// TS `StatusEffect.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusEffectType {
    #[serde(rename = "burn")]
    Burn,
    #[serde(rename = "poison")]
    Poison,
    #[serde(rename = "freeze")]
    Freeze,
    #[serde(rename = "desiccation")]
    Desiccation,
    #[serde(rename = "trap")]
    Trap,
    #[serde(rename = "immobilize")]
    Immobilize,
    #[serde(rename = "sleep")]
    Sleep,
    #[serde(rename = "loseAction")]
    LoseAction,
    #[serde(rename = "selfKO")]
    SelfKo,
    #[serde(rename = "noStealth")]
    NoStealth,
    #[serde(rename = "noHeal")]
    NoHeal,
}

/// TS `CardInstance.zone: "deck" | "hand" | "board" | "graveyard" | "banished"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Zone {
    #[serde(rename = "deck")]
    Deck,
    #[serde(rename = "hand")]
    Hand,
    #[serde(rename = "board")]
    Board,
    #[serde(rename = "graveyard")]
    Graveyard,
    #[serde(rename = "banished")]
    Banished,
}

// ============================================================
// --- Card Definitions (static data, from card catalogue) ---
// ============================================================

/// TS `BaseAction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BaseAction {
    pub name: String,
    /// ATK value for attacks, 0 for support actions
    pub atk: i32,
    /// Trait keywords on this specific attack
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_traits: Option<Vec<AttackTrait>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<Element>,
    /// Is this a support action (heal, buff, etc.) rather than an attack?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_support: Option<bool>,
    /// Heal amount if support
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heal_amount: Option<i32>,
    /// Control effect: immobilize target
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immobilize: Option<bool>,
    /// This attack cannot be dodged (Observation Haki)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cannot_be_dodged: Option<bool>,
    /// The attack strips Furtif/Stealth from the target this turn
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_stealth: Option<bool>,
    /// Support: look at the top N of your deck and reorder them (Nami Prévisions)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scry: Option<i32>,
    /// Support: an enemy of DEF <= 1 loses its next action (Usopp Bluff)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bluff: Option<bool>,
    /// Support: give one ally +N ATK this turn (Brook Mélodie)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_ally_atk: Option<i32>,
    /// Description text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// TS `SpecialAttack.conditionalBonus`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalBonus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vs_trait: Option<Trait>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vs_faction: Option<Faction>,
    pub amount: i32,
}

/// TS `SpecialAttack.transform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub atk: i32,
    pub def: i32,
    pub pv: i32,
    pub turns: i32,
}

/// TS `SpecialAttack`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpecialAttack {
    pub name: String,
    /// Volonte cost
    pub cost: i32,
    /// ATK bonus added to base ATK. Total = baseATK + atkBonus
    pub atk_bonus: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_traits: Option<Vec<AttackTrait>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<Element>,
    /// 1x per game
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once_per_game: Option<bool>,
    /// Ignore N points of DEF
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_def: Option<i32>,
    /// Is support (heal, buff) rather than damage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_support: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heal_amount: Option<i32>,
    /// Control: immobilize the target for 1 turn (Robin Clutch)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immobilize: Option<bool>,
    /// Control: put the target to sleep (loses its next 2 actions)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep: Option<bool>,
    /// This attack cannot be dodged (Observation Haki)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cannot_be_dodged: Option<bool>,
    /// This attack bypasses the Bouclier/Shield block
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_shield: Option<bool>,
    /// Impact: knock the target back one slot (negated by immuneImpact)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushback: Option<bool>,
    /// Impact variant: knock the target back N slots
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushback_slots: Option<i32>,
    /// The attack strips Furtif/Stealth from the target this turn
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_stealth: Option<bool>,
    /// Conditional ATK bonus when the target matches a trait/faction
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditional_bonus: Option<ConditionalBonus>,
    /// Sand: the target loses N permanent PV (in addition to the damage)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permanent_pv_loss: Option<i32>,
    /// The attack also hits a second target (approximated as a small Zone)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub two_targets: Option<bool>,
    /// The target can no longer be healed (desiccation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_heal: Option<bool>,
    /// Self-transformation special (Chopper Monster Point): set stats + Rush for N turns, then self-KO
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// TS `SynergyDef`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynergyDef {
    /// Card ID of the synergy partner
    pub partner_id: String,
    /// Bonus ATK when partner is in play
    pub atk_bonus: i32,
    /// Bonus ATK when partner is KO (rage) — this turn only (TS `onPartnerKO`)
    #[serde(
        rename = "onPartnerKO",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub on_partner_ko: Option<i32>,
}

/// TS `PassiveDef`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PassiveDef {
    pub name: String,
    pub description: String,
    /// Encoded effects — kept as structured data for the engine
    pub effects: Vec<PassiveEffect>,
}

/// TS `AllyFilter`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AllyFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<Faction>,
    /// e.g. "mugiwara", "marine", "bretteur"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// TS `trait` (a Rust keyword, hence the trailing underscore).
    #[serde(rename = "trait", default, skip_serializing_if = "Option::is_none")]
    pub trait_: Option<Trait>,
}

/// TS `PassiveEffect` (discriminated on `type`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum PassiveEffect {
    #[serde(rename = "buffAlly")]
    BuffAlly {
        stat: BuffStat,
        amount: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<AllyFilter>,
    },
    #[serde(rename = "healAdjacent")]
    HealAdjacent { amount: i32 },
    #[serde(rename = "onAllyKO")]
    OnAllyKo {
        effect: OnAllyKoEffectKind,
        amount: i32,
    },
    #[serde(rename = "naturalHaki")]
    NaturalHaki { haki_type: HakiType },
    #[serde(rename = "threeWeaponSlots")]
    ThreeWeaponSlots,
    #[serde(rename = "twoAccessorySlots")]
    TwoAccessorySlots,
    #[serde(rename = "cannotAttackFemale")]
    CannotAttackFemale,
    #[serde(rename = "revive")]
    Revive { pv: i32 },
    #[serde(rename = "logiaIntangibility")]
    LogiaIntangibility,
    #[serde(rename = "immuneImpact")]
    ImmuneImpact,
    #[serde(rename = "startTurnBuffAlly")]
    StartTurnBuffAlly { stat: AtkDefStat, amount: i32 },
    #[serde(rename = "noDodge")]
    NoDodge,
    #[serde(rename = "blockDamageReduction")]
    BlockDamageReduction { amount: i32 },
    #[serde(rename = "stripStealthOnAttack")]
    StripStealthOnAttack,
    #[serde(rename = "debuffAdjacentEnemies")]
    DebuffAdjacentEnemies { amount: i32 },
    #[serde(rename = "debuffOneEnemy")]
    DebuffOneEnemy { amount: i32 },
    #[serde(rename = "banishOnKO")]
    BanishOnKo,
    #[serde(rename = "explodeOnKO")]
    ExplodeOnKo { amount: i32 },
    #[serde(rename = "copyAtkOnDeploy")]
    CopyAtkOnDeploy,
    #[serde(rename = "entryDiscardRandom")]
    EntryDiscardRandom,
    #[serde(rename = "immuneControl")]
    ImmuneControl,
    #[serde(rename = "endTurnDesiccation")]
    EndTurnDesiccation { amount: i32 },
    #[serde(rename = "twoWeaponSlots")]
    TwoWeaponSlots,
    #[serde(rename = "attacksIgnoreShield")]
    AttacksIgnoreShield,
    #[serde(rename = "grantObservationAll")]
    GrantObservationAll,
    #[serde(rename = "selfBuffOnAllyKO")]
    SelfBuffOnAllyKo {
        stat: AtkStat,
        amount: i32,
        max: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<AllyFilter>,
    },
    #[serde(rename = "meleeRecoil")]
    MeleeRecoil { amount: i32 },
    #[serde(rename = "costReduction")]
    CostReduction {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<AllyFilter>,
        amount: i32,
    },
    #[serde(rename = "hakiCostReduction")]
    HakiCostReduction { amount: i32 },
    #[serde(rename = "ignoreEnemyStealth")]
    IgnoreEnemyStealth,
    #[serde(rename = "custom")]
    Custom { id: String },
}

/// TS `CardDef.fruitEffects.base`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FruitBaseEffects {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants_traits: Option<Vec<Trait>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passive_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atk_bonus: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def_bonus: Option<i32>,
}

/// TS `CardDef.fruitEffects.awakening.specialAttack`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FruitAwakeningSpecialAttack {
    pub name: String,
    pub cost: i32,
    pub atk_bonus: i32,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once_per_game: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_traits: Option<Vec<AttackTrait>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_def: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immobilize: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushback: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_shield: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_stealth: Option<bool>,
}

/// TS `CardDef.fruitEffects.awakening`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FruitAwakening {
    /// character name that can awaken
    pub porteur_legitime: String,
    pub min_turns: u32,
    pub vol_cost: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants_traits: Option<Vec<Trait>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atk_bonus: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def_bonus: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passive_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub special_attack: Option<FruitAwakeningSpecialAttack>,
}

/// TS `CardDef.fruitEffects`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FruitEffects {
    pub base: FruitBaseEffects,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awakening: Option<FruitAwakening>,
}

/// TS `CardDef.shipActive`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipActive {
    pub name: String,
    pub cost: i32,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once_per_game: Option<bool>,
}

/// TS `CardDef.shipDestroyEffect`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShipDestroyEffect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heal_all: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_atk: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_def: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_token: Option<String>,
}

/// TS `CardDef`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub card_type: CardType,
    pub cost: i32,
    pub faction: Faction,
    pub rarity: Rarity,
    pub set: String,
    /// Token bodies (jeton Marine, agent, Bananawani…) deployed by effects, not in the deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_token: Option<bool>,

    // --- Character fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atk: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traits: Option<Vec<Trait>>,
    /// Preferred line: front or back (for AI placement)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_row: Option<Row>,
    /// Tags for synergy/filter matching
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Does this character have natural Haki?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_haki: Option<Vec<HakiType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_action: Option<BaseAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub special_attack: Option<SpecialAttack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passive: Option<PassiveDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synergies: Option<Vec<SynergyDef>>,

    // --- Object fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<ObjectSubtype>,
    /// ATK bonus when equipped
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonus_atk: Option<i32>,
    /// DEF bonus when equipped
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bonus_def: Option<i32>,
    /// Restricted to a specific character name or tag
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restriction: Option<String>,
    /// Equipment grants traits
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants_traits: Option<Vec<GrantedTrait>>,
    /// Equipment grants an element to attacks
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants_element: Option<Element>,
    /// Special equipment effect description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equip_effect: Option<String>,
    /// Devil Fruit structured effects
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fruit_effects: Option<FruitEffects>,

    // --- Ship fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship_passive: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship_active: Option<ShipActive>,
    /// Effect when this ship is destroyed/replaced (e.g. Going Merry's funeral).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship_destroy_effect: Option<ShipDestroyEffect>,

    // --- Event fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_effect: Option<EventEffect>,

    // --- Counter fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_effect: Option<CounterEffect>,
}

impl CardDef {
    /// Convenience constructor: a `CardDef` with every optional field unset.
    /// Card-data modules fill the relevant optional fields afterwards.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        card_type: CardType,
        cost: i32,
        faction: Faction,
        rarity: Rarity,
        set: impl Into<String>,
    ) -> Self {
        CardDef {
            id: id.into(),
            name: name.into(),
            card_type,
            cost,
            faction,
            rarity,
            set: set.into(),
            is_token: None,
            atk: None,
            def: None,
            pv: None,
            traits: None,
            preferred_row: None,
            tags: None,
            natural_haki: None,
            base_action: None,
            special_attack: None,
            passive: None,
            synergies: None,
            subtype: None,
            bonus_atk: None,
            bonus_def: None,
            restriction: None,
            grants_traits: None,
            grants_element: None,
            equip_effect: None,
            fruit_effects: None,
            ship_passive: None,
            ship_active: None,
            ship_destroy_effect: None,
            event_effect: None,
            counter_effect: None,
        }
    }

    /// TS `def.traits?.includes(t)`.
    pub fn has_trait(&self, t: Trait) -> bool {
        self.traits.as_ref().is_some_and(|ts| ts.contains(&t))
    }

    /// TS `def.tags?.includes(tag)`.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags
            .as_ref()
            .is_some_and(|ts| ts.iter().any(|x| x == tag))
    }
}

// ============================================================
// --- Event Effects ---
// ============================================================

/// TS `EventEffect` (discriminated on `type`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum EventEffect {
    #[serde(rename = "gainWill")]
    GainWill { amount: i32 },
    #[serde(rename = "draw")]
    Draw {
        amount: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discard: Option<i32>,
    },
    #[serde(rename = "healAlly")]
    HealAlly {
        amount: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        all_allies: Option<bool>,
    },
    #[serde(rename = "buffAllies")]
    BuffAllies {
        stat: AtkDefStat,
        amount: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<AllyFilter>,
        duration: BuffDuration,
    },
    #[serde(rename = "damageEnemies")]
    DamageEnemies {
        amount: i32,
        target: DamageTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursed_bonus: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sand: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        destroy_ships: Option<bool>,
    },
    #[serde(rename = "dodgeAll")]
    DodgeAll,
    #[serde(rename = "rally")]
    Rally { atk: i32, def: i32, heal: i32 },
    #[serde(rename = "buffSingle")]
    BuffSingle {
        stat: AtkDefStat,
        amount: i32,
        duration: BuffDuration,
        /// TS `requiresOwnKO`
        #[serde(
            rename = "requiresOwnKO",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        requires_own_ko: Option<bool>,
    },
    #[serde(rename = "rushBuff")]
    RushBuff { atk: i32 },
    #[serde(rename = "tutor")]
    Tutor {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter_tag: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_cost: Option<i32>,
    },
    #[serde(rename = "deployTokens")]
    DeployTokens { token_id: String, count: u32 },
    #[serde(rename = "healAllBuff")]
    HealAllBuff { heal: i32, atk: i32 },
    #[serde(rename = "debuffAllEnemies")]
    DebuffAllEnemies {
        atk: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        immobilize_max_def: Option<i32>,
    },
    #[serde(rename = "grantHakiAll")]
    GrantHakiAll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        atk: Option<i32>,
    },
    #[serde(rename = "custom")]
    Custom { id: String, description: String },
}

/// TS `CounterEffect` (discriminated on `type`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum CounterEffect {
    #[serde(rename = "survive")]
    Survive { description: String },
    #[serde(rename = "reduceDamage")]
    ReduceDamage {
        amount: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captain_bonus: Option<i32>,
    },
    #[serde(rename = "cancel")]
    Cancel {
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_attacker_atk: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        self_captain_damage: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        once: Option<bool>,
    },
    #[serde(rename = "untargetable")]
    Untargetable { description: String },
}

// ============================================================
// --- Captain Definition ---
// ============================================================

/// TS `CaptainDef.recto`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptainRecto {
    pub pv: i32,
    pub atk: i32,
    pub def: i32,
    pub passive: PassiveDef,
    /// Captain attacks (cost Vol., no free base action)
    pub attacks: Vec<SpecialAttack>,
    /// Surcharge ability
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surcharge: Option<SpecialAttack>,
}

/// TS `CaptainDef.flipCondition` — conditions to flip recto -> verso.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlipCondition {
    /// Volonte cost to flip
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<i32>,
    /// Auto-flip if allies count <= this
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_if_allies_lte: Option<i32>,
    /// Free flip if one of your allies was KO'd this turn (Luffy) (TS `freeIfAllyKO`)
    #[serde(
        rename = "freeIfAllyKO",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub free_if_ally_ko: Option<bool>,
    /// Free flip if an enemy Cursed unit is in play (Akainu)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_if_enemy_cursed: Option<bool>,
    /// Free flip if you control >= N characters (Crocodile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_if_allies_gte: Option<i32>,
    /// Free flip on turn >= N (Shanks)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_if_turn_gte: Option<u32>,
}

/// TS `CaptainDef.verso`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptainVerso {
    pub pv: i32,
    pub atk: i32,
    pub def: i32,
    pub passive: PassiveDef,
    pub entry_effect: EntryEffect,
    pub base_action: BaseAction,
    pub special_attack: SpecialAttack,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surcharge: Option<SpecialAttack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traits: Option<Vec<Trait>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural_haki: Option<Vec<HakiType>>,
}

/// TS `CaptainDef`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptainDef {
    pub id: String,
    pub name: String,
    pub faction: Faction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traits: Option<Vec<Trait>>,
    pub recto: CaptainRecto,
    /// Conditions to flip recto -> verso
    pub flip_condition: FlipCondition,
    pub verso: CaptainVerso,
}

/// TS `EntryEffect` (discriminated on `type`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum EntryEffect {
    #[serde(rename = "buffAllies")]
    BuffAllies {
        stat: AtkDefStat,
        amount: i32,
        duration: TurnDuration,
    },
    #[serde(rename = "draw")]
    Draw { amount: i32 },
    #[serde(rename = "damageEnemies")]
    DamageEnemies {
        amount: i32,
        target: EntryDamageTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursed_bonus: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sand: Option<bool>,
    },
    #[serde(rename = "grantSelfRush")]
    GrantSelfRush,
    #[serde(rename = "haoshoku")]
    Haoshoku {
        immobilize_max_def: i32,
        debuff_atk: i32,
    },
    #[serde(rename = "debuffAllEnemies")]
    DebuffAllEnemies { atk: i32 },
    #[serde(rename = "discardOpponentRandom")]
    DiscardOpponentRandom { amount: i32 },
    #[serde(rename = "multi")]
    Multi { effects: Vec<EntryEffect> },
    #[serde(rename = "custom")]
    Custom { id: String, description: String },
}

// ============================================================
// --- Deck Definition ---
// ============================================================

/// TS `DeckEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckEntry {
    pub card_id: String,
    pub count: u32,
}

/// TS `DeckDef`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckDef {
    pub name: String,
    pub captain_id: String,
    pub cards: Vec<DeckEntry>,
}

// ============================================================
// --- Runtime Game State (value types) ---
// ============================================================
// `CardInstance`, `CaptainInstance`, `PlayerState`, `PendingAttack`,
// `LogEntry` and `GameState` live in `state.rs` next to their constructors.

/// TS `Modifier`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Modifier {
    pub id: String,
    pub stat: ModifierStat,
    pub amount: i32,
    pub source: String,
    pub duration: ModifierDuration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns_remaining: Option<i32>,
}

/// TS `StatusEffect`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEffect {
    #[serde(rename = "type")]
    pub effect_type: StatusEffectType,
    /// -1 = permanent (poison)
    pub turns_remaining: i32,
    pub damage_per_turn: i32,
    pub source: String,
}

// ============================================================
// --- Game Actions (discriminated union) ---
// ============================================================

/// TS `GameAction` — `#[serde(tag = "type")]`, discriminators and field names
/// are byte-identical to the TS literals.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum GameAction {
    #[serde(rename = "deployCharacter")]
    DeployCharacter { instance_id: String, slot: Slot },
    #[serde(rename = "equipObject")]
    EquipObject {
        object_instance_id: String,
        target_instance_id: String,
    },
    #[serde(rename = "deployShip")]
    DeployShip { instance_id: String },
    #[serde(rename = "baseAttack")]
    BaseAttack {
        attacker_instance_id: String,
        target_instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_is_captain: Option<bool>,
    },
    #[serde(rename = "specialAttack")]
    SpecialAttack {
        attacker_instance_id: String,
        target_instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_is_captain: Option<bool>,
    },
    #[serde(rename = "baseSupportAction")]
    BaseSupportAction {
        instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_instance_id: Option<String>,
    },
    #[serde(rename = "playEvent")]
    PlayEvent {
        instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        targets: Option<Vec<String>>,
    },
    #[serde(rename = "playCounter")]
    PlayCounter { instance_id: String },
    #[serde(rename = "useShield")]
    UseShield { blocker_instance_id: String },
    #[serde(rename = "passCounter")]
    PassCounter,
    #[serde(rename = "flipCaptain")]
    FlipCaptain { slot: Slot },
    #[serde(rename = "captainAttack")]
    CaptainAttack {
        target_instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_is_captain: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_special: Option<bool>,
    },
    #[serde(rename = "useHaki")]
    UseHaki {
        haki_type: HakiType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_instance_id: Option<String>,
    },
    #[serde(rename = "moveCharacter")]
    MoveCharacter {
        instance_id: String,
        target_slot: Slot,
    },
    #[serde(rename = "activateShip")]
    ActivateShip { ship_instance_id: String },
    #[serde(rename = "awakenFruit")]
    AwakenFruit { fruit_instance_id: String },
    #[serde(rename = "fruitSpecialAttack")]
    FruitSpecialAttack {
        attacker_instance_id: String,
        fruit_instance_id: String,
        target_instance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_is_captain: Option<bool>,
    },
    #[serde(rename = "endTurn")]
    EndTurn,
}

impl GameAction {
    /// The exact TS discriminator string (`action.type`).
    pub fn type_name(&self) -> &'static str {
        match self {
            GameAction::DeployCharacter { .. } => "deployCharacter",
            GameAction::EquipObject { .. } => "equipObject",
            GameAction::DeployShip { .. } => "deployShip",
            GameAction::BaseAttack { .. } => "baseAttack",
            GameAction::SpecialAttack { .. } => "specialAttack",
            GameAction::BaseSupportAction { .. } => "baseSupportAction",
            GameAction::PlayEvent { .. } => "playEvent",
            GameAction::PlayCounter { .. } => "playCounter",
            GameAction::UseShield { .. } => "useShield",
            GameAction::PassCounter => "passCounter",
            GameAction::FlipCaptain { .. } => "flipCaptain",
            GameAction::CaptainAttack { .. } => "captainAttack",
            GameAction::UseHaki { .. } => "useHaki",
            GameAction::MoveCharacter { .. } => "moveCharacter",
            GameAction::ActivateShip { .. } => "activateShip",
            GameAction::AwakenFruit { .. } => "awakenFruit",
            GameAction::FruitSpecialAttack { .. } => "fruitSpecialAttack",
            GameAction::EndTurn => "endTurn",
        }
    }
}
