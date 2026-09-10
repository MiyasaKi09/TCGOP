//! The data half of [`panels`](crate::panels) — pure view models.
//!
//! Nothing here touches the ECS: every function is
//! `(&GameState, &CardRegistry, &[GameAction], …) -> Option<View>`, so the
//! whole "what does this modal say and what does each button do" layer is
//! unit-testable without a window.
//!
//! Map of the TypeScript sources:
//!
//! | web | here |
//! |---|---|
//! | `ActionMenu.tsx` (`baseReason` / `specReason` / `CardActions`) | [`action_menu_view`] |
//! | `CaptainMenu.tsx` | [`captain_menu_view`] |
//! | `ShipMenu.tsx` + the `shipMenu` branch of `Game.tsx` | [`ship_menu_view`] |
//! | `CardDetail.tsx` | [`card_detail_view`] |
//! | `EventConfirm.tsx` (+ `FullCard.describeEvent`) | [`confirm_view`] |
//! | `Game.tsx::renderCounterWindow` | [`counter_view`] |
//!
//! Four rows have no TSX counterpart — they are the engine's post-port
//! actions, each carrying its cost and its French disabled reason like every
//! other gated control here: the captain's `surcharge`
//! ([`CaptainMenuView::surcharge`], §8.34(b)), an awakened fruit's own special
//! ([`ActionMenuView::fruit_specials`], §8.28 follow-up × §8.48), *Éveiller*
//! ([`ActionMenuView::awakenings`], §8.47) and the free move
//! ([`ActionMenuView::free_move`], §8.29). A fifth split an existing row in
//! two: a support character's effect and its attack are both legal in the same
//! turn, so [`ActionMenuView::base`] and [`ActionMenuView::support`] are
//! separate buttons (§8.38).
//!
//! Every button carries the [`UiCommand`] it produces, so the renderer only has
//! to hand it to
//! [`apply_ui_command`](crate::board::interaction::apply_ui_command).

use tcgop_engine::board::{get_effective_atk, get_effective_def, has_summoning_sickness};
use tcgop_engine::captain::FreeFlipReason;
use tcgop_engine::registry::CardRegistry;
use tcgop_engine::state::GameState;
use tcgop_engine::types::{
    AtkDefStat, AttackTrait, BuffDuration, CardType, CounterEffect, DamageTarget, Element,
    EventEffect, Faction, GameAction, HakiType, ObjectSubtype, PlayerId, Rarity, StatusEffect,
    StatusEffectType, Trait,
};

use crate::hand::card_face::{describe_counter, describe_event};
use crate::selection::{
    AttackKind, UiCommand, UiMode, awakenable_fruits, captain_key, fruit_awakening_special,
    fruit_special_ids, move_slots, support_needs_target,
};

// ============================================================
// Shared little models
// ============================================================

/// One status effect as a pill — TS `STATUS_META` (`StatusBadges.tsx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusChip {
    pub effect: StatusEffectType,
    pub label: &'static str,
    /// `-1` means permanent (TS renders `∞`).
    pub turns: i32,
}

/// TS `STATUS_TEXT[type].label` (`StatusBadges.tsx`) — the shared vocabulary of
/// the board tokens, the captain menu and the card detail.
pub fn status_label(effect: StatusEffectType) -> &'static str {
    match effect {
        StatusEffectType::Burn => "Brûlé",
        StatusEffectType::Poison => "Empoisonné",
        StatusEffectType::Freeze => "Gelé",
        StatusEffectType::Desiccation => "Dessèchement",
        StatusEffectType::Trap => "Piégé",
        StatusEffectType::Immobilize => "Immobilisé",
        StatusEffectType::Sleep => "Endormi",
        StatusEffectType::LoseAction => "Action perdue",
        StatusEffectType::SelfKo => "Sursis",
        StatusEffectType::NoStealth => "Repéré",
        StatusEffectType::NoHeal => "Soins bloqués",
        StatusEffectType::Taunt => "Provoqué",
    }
}

/// TS `turnsLabel(n)` — `∞` for a permanent effect.
pub fn turns_label(turns: i32) -> String {
    if turns < 0 {
        "\u{221E}".to_string()
    } else {
        turns.to_string()
    }
}

/// TS `<StatusBadges effects={…} />`.
pub fn status_chips(effects: &[StatusEffect]) -> Vec<StatusChip> {
    effects
        .iter()
        .map(|e| StatusChip {
            effect: e.effect_type,
            label: status_label(e.effect_type),
            turns: e.turns_remaining,
        })
        .collect()
}

/// TS `CardDetail`'s "Effets actifs" line:
/// `{label}{" (n t)" | " (permanent)"}{" — d/tour"}`.
pub fn status_detail_line(effect: &StatusEffect) -> String {
    let mut line = status_label(effect.effect_type).to_string();
    if effect.turns_remaining > 0 {
        line.push_str(&format!(" ({} t)", effect.turns_remaining));
    } else {
        line.push_str(" (permanent)");
    }
    if effect.damage_per_turn != 0 {
        line.push_str(&format!(" \u{2014} {}/tour", effect.damage_per_turn));
    }
    line
}

/// One attached object — TS `instance.attachedObjects.map(…)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipmentLine {
    pub name: String,
    pub bonus_atk: i32,
    pub bonus_def: i32,
}

impl EquipmentLine {
    /// TS `` `${objDef.name}${bonusAtk ? ` +${bonusAtk} ATK` : ""}…` ``.
    pub fn label(&self) -> String {
        let mut out = self.name.clone();
        if self.bonus_atk != 0 {
            out.push_str(&format!(" +{} ATK", self.bonus_atk));
        }
        if self.bonus_def != 0 {
            out.push_str(&format!(" +{} DEF", self.bonus_def));
        }
        out
    }
}

fn equipment_lines(
    state: &GameState,
    registry: &CardRegistry,
    attached: &[String],
) -> Vec<EquipmentLine> {
    attached
        .iter()
        .filter_map(|id| {
            let obj = state.card(id)?;
            let def = registry.card_def(&obj.def_id)?;
            Some(EquipmentLine {
                name: def.name.clone(),
                bonus_atk: def.bonus_atk.unwrap_or(0),
                bonus_def: def.bonus_def.unwrap_or(0),
            })
        })
        .collect()
}

// ============================================================
// Action menu (ActionMenu.tsx)
// ============================================================

/// One clickable rules row — TS `CardActions["base"] / ["special"]` fused with
/// what `FullCard.renderAction` draws for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackOption {
    pub name: String,
    pub description: Option<String>,
    pub element: Option<Element>,
    pub attack_traits: Vec<AttackTrait>,
    /// `0 Vol.` for a base action, `SpecialAttack::cost` for the special.
    pub cost: i32,
    /// `None` when the action heals (TS hides the ATK column then).
    pub atk: Option<i32>,
    pub is_special: bool,
    pub disabled: bool,
    /// Why it is greyed out, in French — the exact `ActionMenu.tsx` strings.
    pub reason: Option<String>,
    /// What a click does (ignored while [`AttackOption::disabled`]).
    pub command: UiCommand,
}

/// TS `ActionMenu`'s whole payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMenuView {
    pub instance_id: String,
    pub def_id: String,
    pub name: String,
    /// The owner's Volonté, shown top-right.
    pub volonte: i32,
    pub atk: i32,
    pub def: i32,
    /// TS's four state pills, in the same order.
    pub flags: Vec<&'static str>,
    /// The printed base **attack** (`baseAttack`). A support character keeps
    /// this row whenever the engine offers it the attack too — the two are not
    /// exclusive (`actions.rs`: "even support chars … do their effect *and*
    /// attack").
    pub base: Option<AttackOption>,
    /// The printed base **support** action (`baseSupportAction`), when the def
    /// carries `isSupport`.
    pub support: Option<AttackOption>,
    pub special: Option<AttackOption>,
    /// One row per awakened Devil Fruit this unit wears whose
    /// `fruitEffects.awakening.specialAttack` it may declare — the
    /// `fruitSpecialAttack` action (§8.28 follow-up × §8.48). Without it the
    /// awakening is a dead end: the fruit flips its art and nothing can spend
    /// it.
    pub fruit_specials: Vec<AttackOption>,
    /// *Éveiller* — one per Devil Fruit the engine is offering
    /// [`GameAction::AwakenFruit`] for, plus the fruits it refuses with the
    /// reason why.
    pub awakenings: Vec<AbilityButton>,
    /// *Déplacer* — the once-per-turn free repositioning (§8.29). `None` when
    /// the unit carries no move at all (never deployed, off the board).
    pub free_move: Option<AbilityButton>,
    pub equipment: Vec<EquipmentLine>,
    /// The permanent maximum-PV loss this unit has taken (§8.36/§8.38/§8.40
    /// "perd N PV permanent (Sable)"), `0` when it has taken none. The menu
    /// prints it because a heal can never climb back over it.
    pub pv_max_loss: i32,
    /// The unit carries a `noHeal` status — its PV cannot go back up at all.
    pub no_heal: bool,
    /// §8.38 — "Provoqué : doit cibler {taunter}" while a Taunt binds this
    /// unit; `None` otherwise.
    pub taunt: Option<String>,
    /// *Détails* → `cardDetail`.
    pub detail_command: UiCommand,
}

/// TS `ActionMenu.onSupportAction` — pick a target on the board when the
/// support action has any, otherwise fire it straight away.
fn support_command(valid: &[GameAction], instance_id: &str) -> UiCommand {
    if support_needs_target(valid, instance_id) {
        UiCommand::SetMode(UiMode::SelectingSupportTarget {
            instance_id: instance_id.to_string(),
        })
    } else {
        UiCommand::Dispatch(GameAction::BaseSupportAction {
            instance_id: instance_id.to_string(),
            target_instance_id: None,
        })
    }
}

fn aim(attacker_id: &str, kind: AttackKind) -> UiCommand {
    UiCommand::SetMode(UiMode::SelectingTarget {
        attacker_id: attacker_id.to_string(),
        kind,
    })
}

/// One extra ability button of a menu — the rows that are not the base action
/// and not the printed special: the free move (§8.29), *Éveiller* (`awakenFruit`),
/// an awakened fruit's special (§8.28 follow-up × §8.48) and the captain's
/// surcharge (§8.34(b)).
///
/// It carries the same three things every gated control in this client carries:
/// what it costs, whether the engine is offering it, and — when it is not —
/// the French reason it is greyed out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbilityButton {
    /// What the button says, without the cost suffix the renderer appends.
    pub label: String,
    /// Volonté the action spends (`0` when it is free).
    pub cost: i32,
    pub disabled: bool,
    pub reason: Option<String>,
    pub command: UiCommand,
}

/// TS `ActionMenu` — the unit's stats, its two rules rows with their disabled
/// reasons, its equipment and the *Détails* button.
///
/// One deliberate refinement over the TSX: summoning sickness is asked of the
/// engine ([`has_summoning_sickness`]) instead of being re-derived from
/// `def.traits`, so a Rush granted by an equipped Devil Fruit is honoured. The
/// wording ("Mal de terre") and the priority order are unchanged.
pub fn action_menu_view(
    state: &GameState,
    registry: &CardRegistry,
    valid: &[GameAction],
    instance_id: &str,
) -> Option<ActionMenuView> {
    let instance = state.card(instance_id)?;
    let def = registry.card_def(&instance.def_id)?;

    let atk = get_effective_atk(state, registry, instance_id).unwrap_or(0);
    let def_value = get_effective_def(state, registry, instance_id).unwrap_or(0);
    let volonte = state.player(instance.owner).volonte;

    let can_base_attack = valid.iter().any(|a| {
        matches!(a, GameAction::BaseAttack { attacker_instance_id, .. } if attacker_instance_id == instance_id)
    });
    let can_special_attack = valid.iter().any(|a| {
        matches!(a, GameAction::SpecialAttack { attacker_instance_id, .. } if attacker_instance_id == instance_id)
    });
    let can_support = valid.iter().any(
        |a| matches!(a, GameAction::BaseSupportAction { instance_id: id, .. } if id == instance_id),
    );

    let tapped = instance.tapped;
    let sickness = has_summoning_sickness(state, registry, instance_id).unwrap_or(false);
    let frozen = instance.has_status(StatusEffectType::Freeze);
    let immobilised = instance.has_status(StatusEffectType::Immobilize);

    let base_def = def.base_action.as_ref();
    let is_support = base_def.and_then(|b| b.is_support).unwrap_or(false);

    // TS `baseReason`, same clauses in the same order — but split in two, one
    // per row, because the two base actions are gated independently: a support
    // unit whose effect is live may still have nothing in range to hit, and a
    // row that is dead must say which of the two reasons killed it.
    // §8.38 — a Taunt binds its bearer to its source: while the taunter is a
    // legal target it is the *only* one, and `combat::enforce_taunt` refuses
    // every other declaration. It is never a *reason a row is dead* (a taunter
    // that dies, goes Furtif or drifts out of range releases the attacker, so
    // "bound" and "no target" cannot both hold); it is a live row whose target
    // list has silently shrunk to one, which is exactly the kind of thing a
    // player reads as a bug unless the menu names the unit it has to hit.
    let taunt: Option<String> = instance.status(StatusEffectType::Taunt).map(|effect| {
        let taunter = state
            .card(&effect.source)
            .and_then(|c| registry.card_def(&c.def_id))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| effect.source.clone());
        format!("Provoqué : doit cibler {taunter}")
    });
    let blocked: Option<String> = first_reason(&[
        (frozen, "Gelé !"),
        (immobilised, "Immobilisé !"),
        (sickness, "Mal de terre"),
        (tapped, "Incliné"),
        (instance.used_base_action, "Déjà utilisé ce tour"),
    ]);
    let attack_reason: Option<String> = if can_base_attack {
        None
    } else {
        blocked
            .clone()
            .or_else(|| Some(if atk <= 0 { "ATK 0" } else { "Pas de cible" }.to_string()))
    };
    let support_reason: Option<String> = if can_support {
        None
    } else {
        blocked.clone().or_else(|| Some("Pas de cible".to_string()))
    };

    let special_def = def.special_attack.as_ref();
    // TS `specReason`, same order.
    let special_reason: Option<String> = if frozen {
        Some("Gelé !".into())
    } else if immobilised {
        Some("Immobilisé !".into())
    } else if sickness {
        Some("Mal de terre".into())
    } else if instance.used_special_attack {
        Some("Déjà utilisé ce tour".into())
    } else if special_def
        .is_some_and(|s| s.once_per_game.unwrap_or(false) && instance.used_once(&s.name))
    {
        Some("Déjà utilisé (1x/partie)".into())
    } else if special_def.is_some_and(|s| volonte < s.cost) {
        let cost = special_def.map(|s| s.cost).unwrap_or(0);
        Some(format!("Volonté insuffisante ({volonte}/{cost})"))
    } else if !can_special_attack {
        Some("Pas de cible".into())
    } else {
        None
    };

    // The base row is the **attack**, and a support unit gets a second row of
    // its own.
    //
    // The TS fused the two into one button because `baseAttack` and
    // `baseSupportAction` looked mutually exclusive there. They are not: the
    // Rust enumerator offers `baseAttack` to "ALL characters with ATK > 0
    // (even support chars like Chopper ATK 1 — they do their effect *and*
    // attack)" while the support loop offers `baseSupportAction` in the same
    // breath, so a support character with ATK could only ever reach one of the
    // two legal actions through a single button — the support one, since it
    // won the `if`. Splitting the row is the only way the menu can offer both.
    let base = base_def
        // A pure support card with no ATK at all has no attack to show; the
        // engine never offers it one (`get_effective_atk > 0`).
        .filter(|_| !is_support || atk > 0)
        .map(|b| AttackOption {
            name: if is_support {
                "Attaque".to_string()
            } else {
                b.name.clone()
            },
            description: (!is_support).then(|| b.description.clone()).flatten(),
            element: b.element,
            attack_traits: b.attack_traits.clone().unwrap_or_default(),
            cost: 0,
            atk: Some(atk),
            is_special: false,
            disabled: !can_base_attack,
            reason: attack_reason,
            command: aim(instance_id, AttackKind::Base),
        });

    let support = base_def.filter(|_| is_support).map(|b| {
        let heals = b.heal_amount.is_some();
        AttackOption {
            name: b.name.clone(),
            description: b.description.clone(),
            element: b.element,
            attack_traits: b.attack_traits.clone().unwrap_or_default(),
            cost: 0,
            atk: (!heals).then_some(atk),
            is_special: false,
            disabled: !can_support,
            reason: support_reason,
            command: if can_support {
                support_command(valid, instance_id)
            } else {
                UiCommand::Ignore
            },
        }
    });

    let special = special_def.map(|s| {
        let heals = s.is_support.unwrap_or(false) && s.heal_amount.is_some();
        AttackOption {
            name: s.name.clone(),
            description: s.description.clone(),
            element: s.element,
            attack_traits: s.attack_traits.clone().unwrap_or_default(),
            cost: s.cost,
            atk: (!heals).then_some(atk + s.atk_bonus),
            is_special: true,
            disabled: !can_special_attack,
            reason: special_reason,
            command: aim(instance_id, AttackKind::Special),
        }
    });

    let mut flags = Vec::new();
    if tapped {
        flags.push("Incliné");
    }
    if sickness {
        flags.push("Mal de terre");
    }
    if frozen {
        flags.push("Gelé");
    }
    if immobilised {
        flags.push("Immobilisé");
    }
    if instance.has_status(StatusEffectType::NoHeal) {
        flags.push("Soins bloqués");
    }
    if instance.has_status(StatusEffectType::Taunt) {
        flags.push("Provoqué");
    }
    // §8.42 — natural Haki in **either** printed form (the `naturalHaki` array
    // or a `PassiveEffect::NaturalHaki`) is what lets this unit's blows land on
    // a Logia. The engine reads both; the menu says so, since nothing else on
    // screen explains why one attacker goes through an intangible target and
    // its neighbour does not.
    if tcgop_engine::haki::def_has_natural_haki(def) {
        flags.push("Haki naturel");
    }

    // --- the awakened fruits this unit may fire (§8.28 follow-up × §8.48) ---
    let fruit_specials = fruit_special_rows(
        state,
        registry,
        valid,
        instance_id,
        &instance.attached_objects,
        atk,
        volonte,
        // The engine gates a character's fruit special on the same three
        // "cannot act" flags as its own special, plus summoning sickness.
        first_reason(&[
            (frozen, "Gelé !"),
            (immobilised, "Immobilisé !"),
            (sickness, "Mal de terre"),
        ]),
    );

    // --- *Éveiller* (§8.47/§8.48: awakening is what unlocks the above) ---
    let awakenings = awakening_rows(state, registry, valid, &instance.attached_objects, volonte);

    // --- the free move (§8.29) ---
    let move_targets = move_slots(valid, instance_id);
    let free_move = def.pv.map(|_| {
        let reason = first_reason(&[
            (frozen, "Gelé !"),
            (immobilised, "Immobilisé !"),
            (tapped, "Incliné"),
            (
                state.player(instance.controller()).used_free_move,
                "Déplacement déjà utilisé",
            ),
            (move_targets.is_empty(), "Aucune case adjacente"),
        ]);
        AbilityButton {
            label: "Déplacer".to_string(),
            cost: 0,
            disabled: move_targets.is_empty(),
            reason,
            command: UiCommand::SetMode(UiMode::SelectingMoveSlot {
                instance_id: instance_id.to_string(),
            }),
        }
    });

    Some(ActionMenuView {
        instance_id: instance_id.to_string(),
        def_id: instance.def_id.clone(),
        name: def.name.clone(),
        volonte,
        atk,
        def: def_value,
        flags,
        base,
        support,
        special,
        fruit_specials,
        awakenings,
        free_move,
        equipment: equipment_lines(state, registry, &instance.attached_objects),
        pv_max_loss: instance.pv_max_loss.unwrap_or(0).max(0),
        no_heal: instance.has_status(StatusEffectType::NoHeal),
        taunt,
        detail_command: UiCommand::SetMode(UiMode::CardDetail {
            def_id: instance.def_id.clone(),
            instance_id: Some(instance_id.to_string()),
        }),
    })
}

/// The first clause that holds, in declaration order — the shape every
/// `…Reason` block in `ActionMenu.tsx` / `CaptainMenu.tsx` has.
fn first_reason(clauses: &[(bool, &'static str)]) -> Option<String> {
    clauses
        .iter()
        .find(|(holds, _)| *holds)
        .map(|(_, text)| (*text).to_string())
}

/// One [`AttackOption`] per awakened Devil Fruit in `attached` whose special
/// `attacker_id` could fire — the `fruitSpecialAttack` rows of a unit menu or
/// of the captain menu.
///
/// `blocked` is the bearer-level reason (frozen / tapped / …) that outranks the
/// per-fruit ones, exactly like `specReason` outranks the cost check.
#[allow(clippy::too_many_arguments)]
fn fruit_special_rows(
    state: &GameState,
    registry: &CardRegistry,
    valid: &[GameAction],
    attacker_id: &str,
    attached: &[String],
    atk: i32,
    volonte: i32,
    blocked: Option<String>,
) -> Vec<AttackOption> {
    let offered = fruit_special_ids(valid, attacker_id);
    let mut rows = Vec::new();
    for fruit_id in attached {
        let Some(fruit) = state.card(fruit_id) else {
            continue;
        };
        // A fruit that is not awakened yet has no special to show: the menu
        // offers *Éveiller* for it instead.
        if !fruit.is_awakened.unwrap_or(false) {
            continue;
        }
        let Some(spec) = fruit_awakening_special(state, registry, fruit_id) else {
            continue;
        };
        let can = offered.contains(fruit_id);
        let bearer_used_once = state
            .card(attacker_id)
            .map(|c| c.used_once(&spec.name))
            .unwrap_or_else(|| {
                crate::selection::captain_key_owner(attacker_id)
                    .is_some_and(|p| state.player(p).captain.used_once(&spec.name))
            });
        let reason = if can {
            None
        } else if blocked.is_some() {
            blocked.clone()
        } else {
            first_reason(&[
                (
                    spec.once_per_game.unwrap_or(false) && bearer_used_once,
                    "Déjà utilisé (1x/partie)",
                ),
                (volonte < spec.cost, "Volonté insuffisante"),
            ])
            .or_else(|| Some("Pas de cible".to_string()))
        };
        rows.push(AttackOption {
            name: spec.name.clone(),
            description: Some(spec.description.clone()),
            element: spec.element,
            attack_traits: spec.attack_traits.clone().unwrap_or_default(),
            cost: spec.cost,
            atk: Some(atk + spec.atk_bonus),
            is_special: true,
            disabled: !can,
            reason,
            command: aim(
                attacker_id,
                AttackKind::Fruit {
                    fruit_instance_id: fruit_id.clone(),
                },
            ),
        });
    }
    rows
}

/// One *Éveiller* button per Devil Fruit in `attached` that is not awakened
/// yet — offered when the engine lists [`GameAction::AwakenFruit`] for it,
/// greyed out with a reason otherwise.
fn awakening_rows(
    state: &GameState,
    registry: &CardRegistry,
    valid: &[GameAction],
    attached: &[String],
    volonte: i32,
) -> Vec<AbilityButton> {
    let offered = awakenable_fruits(valid, attached);
    let mut rows = Vec::new();
    for fruit_id in attached {
        let Some(fruit) = state.card(fruit_id) else {
            continue;
        };
        if fruit.is_awakened.unwrap_or(false) {
            continue;
        }
        let Some(def) = registry.card_def(&fruit.def_id) else {
            continue;
        };
        if def.subtype != Some(ObjectSubtype::Fruit) {
            continue;
        }
        let cost = def
            .fruit_effects
            .as_ref()
            .and_then(|fx| fx.awakening.as_ref())
            .map(|aw| aw.vol_cost)
            .unwrap_or(0);
        let can = offered.contains(fruit_id);
        rows.push(AbilityButton {
            label: format!("Éveiller {}", def.name),
            cost,
            disabled: !can,
            reason: (!can).then(|| {
                first_reason(&[(volonte < cost, "Volonté insuffisante")])
                    .unwrap_or_else(|| "Conditions non réunies".to_string())
            }),
            command: UiCommand::Dispatch(GameAction::AwakenFruit {
                fruit_instance_id: fruit_id.clone(),
            }),
        });
    }
    rows
}

// ============================================================
// Captain menu (CaptainMenu.tsx)
// ============================================================

/// Which glyph / accent an [`AbilityRow`] wears — TS `<AbilityRow kind>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityKind {
    /// `⚔`, deploy green — the verso's base action.
    Base,
    /// `★`, atk red — a recto attack or the verso special.
    Attack,
    /// `⚡`, gold — a surcharge.
    Surcharge,
}

/// TS `<AbilityRow a={…} />`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbilityRow {
    pub kind: AbilityKind,
    pub name: String,
    pub element: Option<Element>,
    pub cost: i32,
    pub description: Option<String>,
    /// Why this printed power cannot be played from the face that is showing.
    ///
    /// §8.34(c): `recto.attacks` and `recto.surcharge` are never enumerated —
    /// "Le Capitaine recto ne peut pas attaquer" (Rulebook v3.1 §2.1, quoted
    /// verbatim in `captains.ts`) — so the row is drawn (the card prints it)
    /// but it carries the rule that forbids it instead of pretending to be
    /// clickable.
    pub reason: Option<&'static str>,
}

/// TS "En Verso (engagé)" preview, shown on the recto only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersoPreview {
    pub atk: i32,
    pub def: i32,
    pub pv: i32,
    pub passive_name: String,
}

/// TS `CaptainMenu`'s whole payload.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptainMenuView {
    pub player: PlayerId,
    pub is_you: bool,
    pub def_id: String,
    pub name: String,
    pub faction: Faction,
    pub flipped: bool,
    pub pv: i32,
    pub max_pv: i32,
    /// `max(0, pv / max_pv)`, clamped to 1.
    pub ratio: f32,
    pub atk: i32,
    pub def: i32,
    pub statuses: Vec<StatusChip>,
    pub passive_name: String,
    pub passive_description: String,
    /// The powers of the side that is face-up.
    pub abilities: Vec<AbilityRow>,
    /// Verso only.
    pub natural_haki: Vec<HakiType>,
    /// Verso only.
    pub traits: Vec<Trait>,
    /// Recto only.
    pub verso_preview: Option<VersoPreview>,
    pub can_flip: bool,
    /// The *Attaquer* button only exists once the captain is engaged.
    pub show_attack: bool,
    pub can_attack: bool,
    pub attack_reason: Option<&'static str>,
    /// The ★ special attack — engine §8.2 item 34(a), `captainAttack` with
    /// `isSpecial: true`.
    ///
    /// The menu already *prices* this ability in its `AbilityRow` list, so
    /// without a button of its own the player is shown a Volonté cost the UI
    /// has no path to spend while the engine keeps offering the action in
    /// `Session::valid`.
    pub show_special_attack: bool,
    pub can_special_attack: bool,
    pub special_attack_name: String,
    pub special_attack_cost: i32,
    pub special_attack_reason: Option<&'static str>,
    /// *Surcharge* — §8.34(b)'s `useSurcharge`. `None` while the active face
    /// prints no `surcharge` (which is every face of the shipped catalogue),
    /// so the button only ever appears for data that defines one.
    pub surcharge: Option<AbilityButton>,
    /// The awakened fruits the captain wears and may fire (§8.28 follow-up ×
    /// §8.48 — `MG-014` Kong Gun, `BW-011` Ground Death, `MR-011` Inugami
    /// Guren).
    pub fruit_specials: Vec<AttackOption>,
    /// *Éveiller* for a fruit the captain wears but has not awakened yet.
    pub awakenings: Vec<AbilityButton>,
    /// The equipment the captain wears (§8.28 follow-up).
    pub equipment: Vec<EquipmentLine>,
    pub can_king_haki: bool,
    /// What *Engager* costs — `flipCondition.cost ?? 0` (§8.6: a condition
    /// with no cost is a cost of zero, and the flip is then always affordable).
    pub flip_cost: i32,
    /// Why the flip is **free** right now, in French — §8.33's five clauses,
    /// read straight off the engine's own [`free_flip_reason`]. `None` means
    /// the player pays [`CaptainMenuView::flip_cost`].
    pub free_flip_reason: Option<&'static str>,
    /// *Engager* → pick the slot the verso lands on.
    pub flip_command: UiCommand,
    /// *Attaquer* → aim the human captain.
    pub attack_command: UiCommand,
    /// *Spéciale ★* → aim the human captain's special attack.
    pub special_attack_command: UiCommand,
    /// *Haki des Rois* → dispatch straight away.
    pub king_haki_command: UiCommand,
}

/// §8.33 — the printed clause that makes the flip free, in French.
pub fn free_flip_label(reason: FreeFlipReason) -> &'static str {
    match reason {
        FreeFlipReason::AllyKo => "Gratuit : un allié est KO ce tour",
        FreeFlipReason::AutoIfAlliesLte => "Gratuit : trop peu d'alliés",
        FreeFlipReason::EnemyCursed => "Gratuit : un ennemi Maudit est en jeu",
        FreeFlipReason::AlliesGte => "Gratuit : assez d'alliés en jeu",
        FreeFlipReason::TurnGte => "Gratuit : le tour est venu",
    }
}

/// TS `HakiType` labels (the TSX prints the raw ids).
pub fn haki_label(haki: HakiType) -> &'static str {
    match haki {
        HakiType::Observation => "Observation",
        HakiType::Armament => "Armement",
        HakiType::King => "Rois",
    }
}

/// TS `CaptainMenu` — stats, PV, passive, the powers of the current side, the
/// verso preview and the three action buttons.
pub fn captain_menu_view(
    state: &GameState,
    registry: &CardRegistry,
    valid: &[GameAction],
    player: PlayerId,
    human: PlayerId,
) -> Option<CaptainMenuView> {
    let captain = &state.player(player).captain;
    let def = registry.captain_def(&captain.def_id)?;
    let is_you = player == human;

    let (max_pv, atk, def_value, passive) = if captain.flipped {
        (
            def.verso.pv,
            def.verso.atk,
            def.verso.def,
            &def.verso.passive,
        )
    } else {
        (
            def.recto.pv,
            def.recto.atk,
            def.recto.def,
            &def.recto.passive,
        )
    };
    let ratio = if max_pv > 0 {
        (captain.current_pv as f32 / max_pv as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let can_flip = is_you
        && !captain.flipped
        && valid
            .iter()
            .any(|a| matches!(a, GameAction::FlipCaptain { .. }));
    let can_attack = is_you
        && captain.flipped
        && valid.iter().any(|a| {
            matches!(
                a,
                GameAction::CaptainAttack { is_special, .. } if !is_special.unwrap_or(false)
            )
        });
    let can_special_attack = is_you
        && captain.flipped
        && valid.iter().any(|a| {
            matches!(
                a,
                GameAction::CaptainAttack {
                    is_special: Some(true),
                    ..
                }
            )
        });
    let can_king_haki = is_you
        && valid.iter().any(|a| {
            matches!(
                a,
                GameAction::UseHaki {
                    haki_type: HakiType::King,
                    ..
                }
            )
        });
    // §8.34(b) — `useSurcharge` is its own variant, so it is asked for by name.
    let can_surcharge = is_you
        && captain.flipped
        && valid
            .iter()
            .any(|a| matches!(a, GameAction::UseSurcharge { .. }));

    // TS `attackReason`.
    let attack_reason = if captain.has_status(StatusEffectType::Freeze) {
        Some("Gelé !")
    } else if captain.has_status(StatusEffectType::Immobilize) {
        Some("Immobilisé !")
    } else if captain.tapped {
        Some("Incliné")
    } else if captain.deployed_turn == Some(state.turn_number as i64) {
        Some("Vient d'être engagé")
    } else {
        None
    };

    // Why the ★ button is dead, in the engine's own order of tests
    // (`actions.rs`: already used, once-per-game spent, then affordability).
    let special = &def.verso.special_attack;
    let special_attack_reason = if can_special_attack {
        None
    } else if attack_reason.is_some() {
        attack_reason
    } else if captain.used_special_attack {
        Some("Déjà utilisée ce tour")
    } else if special.once_per_game.unwrap_or(false) && captain.used_once(&special.name) {
        Some("Une fois par partie")
    } else if !state.can_afford(player, special.cost) {
        Some("Volonté insuffisante")
    } else {
        None
    };

    // §8.34(c): the recto captain has no attack and no surcharge — the rows are
    // still drawn (the card prints them) but they carry the rule instead.
    const RECTO_RULE: &str = "Le Capitaine recto ne peut pas attaquer";

    let mut abilities = Vec::new();
    if captain.flipped {
        abilities.push(AbilityRow {
            kind: AbilityKind::Base,
            name: def.verso.base_action.name.clone(),
            element: def.verso.base_action.element,
            cost: 0,
            description: def.verso.base_action.description.clone(),
            reason: None,
        });
        abilities.push(AbilityRow {
            kind: AbilityKind::Attack,
            name: def.verso.special_attack.name.clone(),
            element: def.verso.special_attack.element,
            cost: def.verso.special_attack.cost,
            description: def.verso.special_attack.description.clone(),
            reason: None,
        });
        if let Some(surcharge) = &def.verso.surcharge {
            abilities.push(AbilityRow {
                kind: AbilityKind::Surcharge,
                name: surcharge.name.clone(),
                element: surcharge.element,
                cost: surcharge.cost,
                description: surcharge.description.clone(),
                reason: None,
            });
        }
    } else {
        for attack in &def.recto.attacks {
            abilities.push(AbilityRow {
                kind: AbilityKind::Attack,
                name: attack.name.clone(),
                element: attack.element,
                cost: attack.cost,
                description: attack.description.clone(),
                reason: Some(RECTO_RULE),
            });
        }
        if let Some(surcharge) = &def.recto.surcharge {
            abilities.push(AbilityRow {
                kind: AbilityKind::Surcharge,
                name: surcharge.name.clone(),
                element: surcharge.element,
                cost: surcharge.cost,
                description: surcharge.description.clone(),
                reason: Some(RECTO_RULE),
            });
        }
    }

    // --- the surcharge button (§8.34(b)) ---
    let surcharge = def.verso.surcharge.as_ref().map(|s| {
        let once_used =
            s.once_per_game.unwrap_or(false) && captain.used_once(&format!("surcharge_{}", s.name));
        let reason = if can_surcharge {
            None
        } else {
            first_reason(&[
                (!captain.flipped, RECTO_RULE),
                (attack_reason.is_some(), attack_reason.unwrap_or("")),
                (captain.used_special_attack, "Déjà utilisée ce tour"),
                (once_used, "Une fois par partie"),
                (!state.can_afford(player, s.cost), "Volonté insuffisante"),
            ])
            .or_else(|| Some("Pas de cible".to_string()))
        };
        AbilityButton {
            label: s.name.clone(),
            cost: s.cost,
            disabled: !can_surcharge,
            reason,
            command: aim(&captain_key(player), AttackKind::Surcharge),
        }
    });

    // --- the fruits the captain wears (§8.28 follow-up × §8.48) ---
    let captain_id = captain_key(player);
    let fruit_specials = if is_you {
        fruit_special_rows(
            state,
            registry,
            valid,
            &captain_id,
            &captain.attached_objects,
            atk,
            state.player(player).volonte,
            first_reason(&[
                (!captain.flipped, RECTO_RULE),
                (attack_reason.is_some(), attack_reason.unwrap_or("")),
                (captain.used_special_attack, "Déjà utilisée ce tour"),
            ]),
        )
    } else {
        Vec::new()
    };
    let awakenings = if is_you {
        awakening_rows(
            state,
            registry,
            valid,
            &captain.attached_objects,
            state.player(player).volonte,
        )
    } else {
        Vec::new()
    };

    // --- what engaging costs, and when it is free (§8.6 / §8.33) ---
    let flip_cost = def.flip_condition.cost.unwrap_or(0);
    let free_flip_reason = (!captain.flipped)
        .then(|| tcgop_engine::captain::free_flip_reason(state, registry, player).ok())
        .flatten()
        .flatten()
        .map(free_flip_label);

    Some(CaptainMenuView {
        player,
        is_you,
        def_id: captain.def_id.clone(),
        name: def.name.clone(),
        faction: def.faction,
        flipped: captain.flipped,
        pv: captain.current_pv,
        max_pv,
        ratio,
        atk,
        def: def_value,
        statuses: status_chips(&captain.status_effects),
        passive_name: passive.name.clone(),
        passive_description: passive.description.clone(),
        abilities,
        natural_haki: if captain.flipped {
            def.verso.natural_haki.clone().unwrap_or_default()
        } else {
            Vec::new()
        },
        traits: if captain.flipped {
            def.verso.traits.clone().unwrap_or_default()
        } else {
            Vec::new()
        },
        verso_preview: (!captain.flipped).then(|| VersoPreview {
            atk: def.verso.atk,
            def: def.verso.def,
            pv: def.verso.pv,
            passive_name: def.verso.passive.name.clone(),
        }),
        can_flip,
        show_attack: captain.flipped,
        can_attack,
        attack_reason,
        show_special_attack: captain.flipped,
        can_special_attack,
        special_attack_name: def.verso.special_attack.name.clone(),
        special_attack_cost: def.verso.special_attack.cost,
        special_attack_reason,
        surcharge,
        fruit_specials,
        awakenings,
        equipment: equipment_lines(state, registry, &captain.attached_objects),
        can_king_haki,
        flip_cost,
        free_flip_reason,
        flip_command: UiCommand::SetMode(UiMode::SelectingCaptainSlot),
        attack_command: aim(&captain_key(human), AttackKind::Base),
        special_attack_command: aim(&captain_key(human), AttackKind::Special),
        king_haki_command: UiCommand::Dispatch(GameAction::UseHaki {
            haki_type: HakiType::King,
            target_instance_id: None,
        }),
    })
}

// ============================================================
// Ship menu (ShipMenu.tsx + the `shipMenu` branch of Game.tsx)
// ============================================================

/// TS `def.shipActive`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipActiveView {
    pub name: String,
    pub cost: i32,
    pub description: String,
}

/// TS `ShipMenu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipMenuView {
    pub instance_id: String,
    pub def_id: String,
    pub name: String,
    pub is_you: bool,
    pub passive: Option<String>,
    pub active: Option<ShipActiveView>,
    pub can_activate: bool,
    /// TS: `used ? "déjà utilisé" : !canActivate ? "Volonté insuffisante" : null`.
    pub reason: Option<&'static str>,
    /// *Activer* → `activateShip`.
    pub activate_command: UiCommand,
}

/// TS `Game.tsx`'s `shipMenu` branch fused with `ShipMenu.tsx`.
pub fn ship_menu_view(
    state: &GameState,
    registry: &CardRegistry,
    valid: &[GameAction],
    instance_id: &str,
    is_you: bool,
) -> Option<ShipMenuView> {
    let instance = state.card(instance_id)?;
    let def = registry.card_def(&instance.def_id)?;

    let can_activate = is_you
        && valid.iter().any(|a| {
            matches!(a, GameAction::ActivateShip { ship_instance_id } if ship_instance_id == instance_id)
        });
    let used = def.ship_active.as_ref().is_some_and(|active| {
        active.once_per_game.unwrap_or(false) && instance.used_once(&active.name)
    });
    let reason = if used {
        Some("déjà utilisé")
    } else if !can_activate {
        Some("Volonté insuffisante")
    } else {
        None
    };

    Some(ShipMenuView {
        instance_id: instance_id.to_string(),
        def_id: instance.def_id.clone(),
        name: def.name.clone(),
        is_you,
        passive: def.ship_passive.clone(),
        active: def.ship_active.as_ref().map(|a| ShipActiveView {
            name: a.name.clone(),
            cost: a.cost,
            description: a.description.clone(),
        }),
        can_activate,
        reason,
        activate_command: UiCommand::Dispatch(GameAction::ActivateShip {
            ship_instance_id: instance_id.to_string(),
        }),
    })
}

// ============================================================
// Card detail (CardDetail.tsx)
// ============================================================

/// TS `def.type` / `def.subtype` / `def.rarity`, in French.
pub fn card_type_label(card_type: CardType) -> &'static str {
    match card_type {
        CardType::Character => "Personnage",
        CardType::Object => "Objet",
        CardType::Ship => "Navire",
        CardType::Event => "Événement",
        CardType::Counter => "Contre",
    }
}

/// TS `ObjectSubtype`, in French.
pub fn subtype_label(subtype: ObjectSubtype) -> &'static str {
    match subtype {
        ObjectSubtype::Weapon => "Arme",
        ObjectSubtype::Fruit => "Fruit du Démon",
        ObjectSubtype::Accessory => "Accessoire",
    }
}

/// The rarity code printed on the type line.
pub fn rarity_label(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::C => "C",
        Rarity::U => "U",
        Rarity::R => "R",
        Rarity::Sr => "SR",
        Rarity::L => "L",
        Rarity::Cap => "CAP",
    }
}

/// TS `CardDetail`'s right-hand column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDetailView {
    pub def_id: String,
    pub name: String,
    /// `"Personnage · Arme · SR"`.
    pub type_line: String,
    pub traits: Vec<Trait>,
    /// `"Sanji → +2 ATK (KO : +3)"`.
    pub synergies: Vec<String>,
    pub statuses: Vec<String>,
    pub equipment: Vec<EquipmentLine>,
    /// TS `hasSide` — when false the column shows one italic line instead.
    pub has_side: bool,
}

/// TS `CardDetail` — the full card plus its traits / synergies / statuses /
/// equipment side panel.
pub fn card_detail_view(
    state: &GameState,
    registry: &CardRegistry,
    def_id: &str,
    instance_id: Option<&str>,
) -> Option<CardDetailView> {
    let def = registry.card_def(def_id)?;
    let instance = instance_id.and_then(|id| state.card(id));

    let mut type_line = card_type_label(def.card_type).to_string();
    if let Some(subtype) = def.subtype {
        type_line.push_str(&format!(" \u{00B7} {}", subtype_label(subtype)));
    }
    type_line.push_str(&format!(" \u{00B7} {}", rarity_label(def.rarity)));

    let traits = def.traits.clone().unwrap_or_default();
    let synergies: Vec<String> = def
        .synergies
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|s| {
            let partner = registry
                .card_def(&s.partner_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| s.partner_id.clone());
            let mut line = format!("{partner} \u{2192} +{} ATK", s.atk_bonus);
            if let Some(ko) = s.on_partner_ko {
                line.push_str(&format!(" (KO : +{ko})"));
            }
            line
        })
        .collect();

    let statuses: Vec<String> = instance
        .map(|i| i.status_effects.iter().map(status_detail_line).collect())
        .unwrap_or_default();
    let equipment = instance
        .map(|i| equipment_lines(state, registry, &i.attached_objects))
        .unwrap_or_default();

    let has_side = !synergies.is_empty()
        || !statuses.is_empty()
        || !equipment.is_empty()
        || !traits.is_empty();

    Some(CardDetailView {
        def_id: def_id.to_string(),
        name: def.name.clone(),
        type_line,
        traits,
        synergies,
        statuses,
        equipment,
        has_side,
    })
}

// ============================================================
// Event / ship confirmation (EventConfirm.tsx)
// ============================================================

/// TS `EFFECT_DESCRIPTIONS[effect.type](effect)`, falling back to
/// `FullCard.describeEvent` where the TSX would have printed raw JSON.
pub fn describe_event_confirm(effect: &EventEffect) -> String {
    match effect {
        EventEffect::GainWill { amount } => format!("Gagne +{amount} Volonté ce tour."),
        EventEffect::Draw { amount, discard } => match discard {
            Some(d) => format!("Pioche {amount} carte(s), défausse {d}."),
            None => format!("Pioche {amount} carte(s)."),
        },
        // §8.36 — `healAlly` without `allAllies` heals **one** ally, and the
        // engine picks it: the most wounded, first on a tie. There is no
        // target to offer (the `playEvent` action carries `targets: None` and
        // the executor never reads the field), so the panel states the rule
        // instead of inventing a picker whose choice the engine would ignore.
        EventEffect::HealAlly { amount, all_allies } => {
            if all_allies.unwrap_or(false) {
                format!("Tous les alliés +{amount} PV.")
            } else {
                format!("+{amount} PV à l'allié le plus blessé.")
            }
        }
        EventEffect::BuffAllies {
            stat,
            amount,
            duration,
            ..
        } => format!(
            "Tous les alliés +{amount} {} ({}).",
            match stat {
                AtkDefStat::Atk => "ATK",
                AtkDefStat::Def => "DEF",
            },
            match duration {
                BuffDuration::Turn => "ce tour",
                BuffDuration::Permanent => "permanent",
            }
        ),
        // §8.36 — `single` hits exactly one enemy, chosen by the engine's
        // standing convention (highest effective ATK of the enemy front pool,
        // else of the whole board, else the enemy captain). Same reason as
        // `HealAlly` above: the rule is printed, not offered as a choice.
        EventEffect::DamageEnemies {
            amount,
            target,
            sand,
            ..
        } => {
            let mut line = format!(
                "{amount} dégâts à {}.",
                match target {
                    DamageTarget::AllFront => "toute la Ligne Avant ennemie",
                    DamageTarget::AllCursed => "tous les Maudits ennemis",
                    DamageTarget::Single =>
                        "l'ennemi le plus fort de la Ligne Avant (à défaut, le Capitaine)",
                    DamageTarget::All => "tous les ennemis",
                }
            );
            // §8.36 follow-up: `sand` is a permanent loss of maximum PV, not a
            // point of extra damage — a heal can never undo it.
            if sand.unwrap_or(false) {
                line.push_str(" Sable : \u{2212}1 PV maximum, définitivement.");
            }
            line
        }
        EventEffect::DodgeAll => {
            "Tous vos personnages esquivent toutes les attaques ce tour.".to_string()
        }
        EventEffect::Custom { description, .. } => description.clone(),
        // TS falls through to `JSON.stringify(effect)` here; the `FullCard`
        // wording is the readable equivalent.
        other => describe_event(other),
    }
}

/// TS `EventConfirm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmView {
    pub instance_id: String,
    pub def_id: String,
    pub name: String,
    pub is_ship: bool,
    pub cost: i32,
    pub volonte: i32,
    pub can_afford: bool,
    pub effect: Option<String>,
    pub counter: Option<String>,
    pub ship_passive: Option<String>,
    /// `"Coup de Burst (2V) — …"`.
    pub ship_active: Option<String>,
    /// *Confirmer* → `playEvent` / `deployShip`.
    pub confirm_command: UiCommand,
}

/// TS `EventConfirm` — the effect summary and the Volonté check.
///
/// `is_ship` comes from the [`UiMode`] arm
/// ([`ConfirmShip`](crate::selection::UiMode::ConfirmShip) vs
/// [`ConfirmEvent`](crate::selection::UiMode::ConfirmEvent)).
pub fn confirm_view(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
    is_ship: bool,
) -> Option<ConfirmView> {
    let instance = state.card(instance_id)?;
    let def = registry.card_def(&instance.def_id)?;
    let volonte = state.player(instance.owner).volonte;

    Some(ConfirmView {
        instance_id: instance_id.to_string(),
        def_id: instance.def_id.clone(),
        name: def.name.clone(),
        is_ship,
        cost: def.cost,
        volonte,
        can_afford: volonte >= def.cost,
        effect: def.event_effect.as_ref().map(describe_event_confirm),
        counter: def.counter_effect.as_ref().map(describe_counter_confirm),
        ship_passive: is_ship.then(|| def.ship_passive.clone()).flatten(),
        ship_active: is_ship
            .then_some(def.ship_active.as_ref())
            .flatten()
            .map(|a| format!("{} ({}V) \u{2014} {}", a.name, a.cost, a.description)),
        confirm_command: UiCommand::Dispatch(if is_ship {
            GameAction::DeployShip {
                instance_id: instance_id.to_string(),
            }
        } else {
            GameAction::PlayEvent {
                instance_id: instance_id.to_string(),
                targets: None,
            }
        }),
    })
}

/// TS `EventConfirm`'s counter box (`"Reduit les degats de N."`).
fn describe_counter_confirm(effect: &CounterEffect) -> String {
    match effect {
        CounterEffect::ReduceDamage { amount, .. } => {
            format!("Réduit les dégâts de {amount}.")
        }
        other => describe_counter(other),
    }
}

// ============================================================
// Counter window (Game.tsx::renderCounterWindow)
// ============================================================

/// Which button style a counter option wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterKind {
    /// Blue — play a Counter card from hand.
    PlayCounter,
    /// Gold — an adjacent Bouclier blocks.
    Shield,
    /// Purple — Haki de l'Observation.
    Haki,
    /// Ghost — "Subir les dégâts".
    Pass,
}

/// One button of the counter window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterOption {
    pub kind: CounterKind,
    pub label: String,
    pub command: UiCommand,
}

/// TS `renderCounterWindow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterView {
    pub attacker: String,
    pub target: String,
    pub damage: i32,
    pub element: Option<Element>,
    pub has_haki: bool,
    pub options: Vec<CounterOption>,
}

/// TS `renderCounterWindow` — `None` when no attack is pending or when the
/// human has no counter action at all (the TSX returns `null` then too, and the
/// driver auto-passes).
pub fn counter_view(
    state: &GameState,
    registry: &CardRegistry,
    valid: &[GameAction],
) -> Option<CounterView> {
    let pending = state.pending_attack.as_ref()?;

    let options: Vec<CounterOption> = valid
        .iter()
        .filter_map(|action| {
            let (kind, label) = match action {
                GameAction::PlayCounter { instance_id } => {
                    let def = state
                        .card(instance_id)
                        .and_then(|c| registry.card_def(&c.def_id))?;
                    (
                        CounterKind::PlayCounter,
                        format!("{} ({}V)", def.name, def.cost),
                    )
                }
                GameAction::UseShield {
                    blocker_instance_id,
                } => {
                    let def = state
                        .card(blocker_instance_id)
                        .and_then(|c| registry.card_def(&c.def_id))?;
                    (CounterKind::Shield, format!("Bloquer ({})", def.name))
                }
                GameAction::UseHaki {
                    haki_type: HakiType::Observation,
                    ..
                } => (CounterKind::Haki, "Haki Observation".to_string()),
                GameAction::PassCounter => (CounterKind::Pass, "Subir les dégâts".to_string()),
                _ => return None,
            };
            Some(CounterOption {
                kind,
                label,
                // TS `renderCounterWindow` dispatches **without** `resetUI()`.
                command: UiCommand::DispatchKeepUi(action.clone()),
            })
        })
        .collect();

    if options.is_empty() {
        return None;
    }

    let attacker = attacker_name(state, registry, &pending.attacker_id);
    let target = if pending.target_is_captain {
        "votre Capitaine".to_string()
    } else {
        state
            .card(&pending.target_id)
            .and_then(|c| registry.card_def(&c.def_id))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| pending.target_id.clone())
    };

    Some(CounterView {
        attacker,
        target,
        damage: pending.raw_damage,
        element: pending.element,
        has_haki: pending.has_haki,
        options,
    })
}

/// TS: a `captain_…` attacker resolves through the captain registry.
fn attacker_name(state: &GameState, registry: &CardRegistry, attacker_id: &str) -> String {
    if let Some(owner) = crate::selection::captain_key_owner(attacker_id) {
        return registry
            .captain_def(&state.player(owner).captain.def_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| attacker_id.to_string());
    }
    state
        .card(attacker_id)
        .and_then(|c| registry.card_def(&c.def_id))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| attacker_id.to_string())
}

// ============================================================
// Test helpers
// ============================================================

/// Drive a fresh session until the human may deploy a character, then do it,
/// returning the session and the deployed instance id.
///
/// [`advance`](crate::bridge::testkit::advance) only ever ends the human's
/// turn, so a test that needs a unit on the board has to place one itself.
#[cfg(test)]
pub(crate) fn deploy_one(seed: u64) -> Option<(crate::bridge::Session, String)> {
    use crate::bridge::Session;
    use crate::bridge::testkit::{advance, session as make_session};

    let mut session = make_session(seed);
    let human = session.human;
    let ready = |s: &Session| {
        s.state.current_player == human
            && s.state.pending_attack.is_none()
            && s.valid
                .iter()
                .any(|a| matches!(a, GameAction::DeployCharacter { .. }))
    };
    if !advance(&mut session, 200, ready) {
        return None;
    }
    let action = session
        .valid
        .iter()
        .find(|a| matches!(a, GameAction::DeployCharacter { .. }))
        .cloned()?;
    let GameAction::DeployCharacter { instance_id, .. } = &action else {
        return None;
    };
    let id = instance_id.clone();
    session.dispatch(action).ok()?;
    Some((session, id))
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tcgop_engine::types::Slot;

    use crate::bridge::Session;
    use crate::bridge::testkit::{advance, session as make_session};

    /// A session whose human has at least one character on the board.
    fn deployed_session() -> (Session, String) {
        deploy_one(7).expect("seed 7 must let the human deploy a character")
    }

    // --- status vocabulary ----------------------------------

    #[test]
    fn permanent_effects_read_as_infinity() {
        assert_eq!(turns_label(-1), "\u{221E}");
        assert_eq!(turns_label(0), "0");
        assert_eq!(turns_label(3), "3");
    }

    #[test]
    fn a_status_detail_line_carries_duration_and_tick_damage() {
        let burn = StatusEffect {
            effect_type: StatusEffectType::Burn,
            turns_remaining: 2,
            damage_per_turn: 3,
            source: "x".into(),
        };
        assert_eq!(status_detail_line(&burn), "Brûlé (2 t) — 3/tour");

        let permanent = StatusEffect {
            effect_type: StatusEffectType::Immobilize,
            turns_remaining: 0,
            damage_per_turn: 0,
            source: "x".into(),
        };
        assert_eq!(status_detail_line(&permanent), "Immobilisé (permanent)");
    }

    #[test]
    fn equipment_labels_only_print_non_zero_bonuses() {
        let line = EquipmentLine {
            name: "Wado Ichimonji".into(),
            bonus_atk: 2,
            bonus_def: 0,
        };
        assert_eq!(line.label(), "Wado Ichimonji +2 ATK");
        let both = EquipmentLine {
            name: "Dial".into(),
            bonus_atk: 1,
            bonus_def: 1,
        };
        assert_eq!(both.label(), "Dial +1 ATK +1 DEF");
    }

    // --- action menu ----------------------------------------

    #[test]
    fn the_action_menu_describes_a_real_unit() {
        let (session, id) = deployed_session();
        let view = action_menu_view(&session.state, &session.registry, &session.valid, &id)
            .expect("the unit has an action menu");
        assert_eq!(view.instance_id, id);
        assert!(!view.name.is_empty());
        assert_eq!(view.volonte, session.you().volonte);
        assert_eq!(
            view.detail_command,
            UiCommand::SetMode(UiMode::CardDetail {
                def_id: view.def_id.clone(),
                instance_id: Some(id.clone()),
            })
        );
    }

    #[test]
    fn an_unknown_instance_has_no_action_menu() {
        let session = make_session(1);
        assert!(
            action_menu_view(&session.state, &session.registry, &session.valid, "nope").is_none()
        );
    }

    #[test]
    fn a_base_attack_row_aims_at_the_board_when_it_is_legal() {
        let (session, id) = deployed_session();
        let view =
            action_menu_view(&session.state, &session.registry, &session.valid, &id).unwrap();
        let Some(base) = view.base else {
            return; // the def has no base action at all
        };
        if base.disabled {
            assert!(base.reason.is_some(), "a disabled row always says why");
        } else {
            assert!(base.reason.is_none());
            assert!(matches!(
                base.command,
                UiCommand::SetMode(UiMode::SelectingTarget {
                    kind: AttackKind::Base,
                    ..
                }) | UiCommand::SetMode(UiMode::SelectingSupportTarget { .. })
                    | UiCommand::Dispatch(GameAction::BaseSupportAction { .. })
            ));
        }
    }

    #[test]
    fn summoning_sickness_is_the_reason_right_after_a_deploy() {
        let (session, id) = deploy_one(11).expect("seed 11 must allow a deploy");
        let view =
            action_menu_view(&session.state, &session.registry, &session.valid, &id).unwrap();
        let def = session.registry.card_def(&view.def_id).unwrap();
        let rushes = def
            .traits
            .as_deref()
            .unwrap_or_default()
            .contains(&Trait::Rush);
        if rushes {
            return; // Rush ignores the mal de terre
        }
        assert!(
            view.flags.contains(&"Mal de terre"),
            "a freshly deployed unit without Rush is sick"
        );
        let base = view.base.expect("characters have a base action");
        assert!(base.disabled);
        assert_eq!(base.reason.as_deref(), Some("Mal de terre"));
    }

    #[test]
    fn a_special_attack_the_player_cannot_pay_says_so() {
        let (mut session, id) = deploy_one(3).expect("seed 3 must allow a deploy");
        let human = session.human;
        let def_id = session.state.card(&id).unwrap().def_id.clone();
        let Some(cost) = session
            .registry
            .card_def(&def_id)
            .and_then(|d| d.special_attack.as_ref())
            .map(|s| s.cost)
        else {
            return; // no special attack on this unit
        };
        session.state.player_mut(human).volonte = cost - 1;
        session.refresh_valid();

        let view =
            action_menu_view(&session.state, &session.registry, &session.valid, &id).unwrap();
        let special = view.special.expect("the def has a special attack");
        assert!(special.disabled);
        let reason = special.reason.unwrap();
        assert!(
            reason.starts_with("Volonté insuffisante")
                || reason == "Mal de terre"
                || reason == "Incliné"
                || reason == "Déjà utilisé ce tour"
                || reason == "Gelé !"
                || reason == "Immobilisé !"
                || reason == "Déjà utilisé (1x/partie)",
            "unexpected reason: {reason}"
        );
    }

    // --- captain menu ---------------------------------------

    #[test]
    fn the_recto_captain_menu_previews_the_verso() {
        let session = make_session(5);
        let view = captain_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            session.human,
            session.human,
        )
        .expect("the human has a captain");
        assert!(view.is_you);
        assert!(!view.flipped);
        assert!(view.verso_preview.is_some(), "the recto previews the verso");
        assert!(!view.show_attack, "a recto captain cannot attack");
        assert!(view.natural_haki.is_empty());
        assert!(
            view.abilities.is_empty(),
            "rulebook v3.1 §2.1: a recto has no attacks and no surcharge"
        );
        assert_eq!(view.pv, view.max_pv);
        assert!((view.ratio - 1.0).abs() < 1e-6);
        assert_eq!(
            view.flip_command,
            UiCommand::SetMode(UiMode::SelectingCaptainSlot)
        );
        assert_eq!(
            view.attack_command,
            UiCommand::SetMode(UiMode::SelectingTarget {
                attacker_id: captain_key(session.human),
                kind: AttackKind::Base,
            })
        );
        assert_eq!(
            view.king_haki_command,
            UiCommand::Dispatch(GameAction::UseHaki {
                haki_type: HakiType::King,
                target_instance_id: None,
            })
        );
    }

    #[test]
    fn the_enemy_captain_menu_offers_nothing_actionable() {
        let session = make_session(5);
        let view = captain_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            session.ai_player(),
            session.human,
        )
        .unwrap();
        assert!(!view.is_you);
        assert!(!view.can_flip);
        assert!(!view.can_attack);
        assert!(!view.can_king_haki);
    }

    #[test]
    fn a_flipped_captain_shows_its_verso_powers() {
        let mut session = make_session(5);
        let human = session.human;
        // The AI rarely engages within a test budget, so flip the captain in
        // the state directly: the view is a pure read of it.
        let verso_pv = session
            .registry
            .captain_def(&session.you().captain.def_id)
            .unwrap()
            .verso
            .pv;
        {
            let captain = &mut session.state.player_mut(human).captain;
            captain.flipped = true;
            captain.current_pv = verso_pv;
        }
        session.refresh_valid();
        let view = captain_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            human,
            human,
        )
        .unwrap();
        assert!(view.flipped);
        assert!(view.verso_preview.is_none());
        assert!(view.show_attack);
        assert!(
            view.abilities.iter().any(|a| a.kind == AbilityKind::Base),
            "the verso always exposes its base action"
        );
    }

    /// The engine offers the captain's ★ special on the same `captainAttack`
    /// action with `isSpecial: true` (§8.2 item 34(a)); the menu prices it in
    /// its ability list, so it must also have a way to *use* it.
    #[test]
    fn a_flipped_captain_can_reach_its_special_attack() {
        let mut session = make_session(5);
        let human = session.human;
        let def = session
            .registry
            .captain_def(&session.you().captain.def_id)
            .unwrap()
            .clone();
        {
            let captain = &mut session.state.player_mut(human).captain;
            captain.flipped = true;
            captain.current_pv = def.verso.pv;
            captain.slot = Some(Slot::V1);
            captain.deployed_turn = None;
            captain.tapped = false;
        }
        // Enough Volonté for the ★ ability to be enumerated.
        session.state.player_mut(human).volonte = def.verso.special_attack.cost + 5;
        session.refresh_valid();

        let view = captain_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            human,
            human,
        )
        .unwrap();

        assert!(
            view.show_special_attack,
            "an engaged captain has a ★ attack"
        );
        assert_eq!(view.special_attack_name, def.verso.special_attack.name);
        assert_eq!(view.special_attack_cost, def.verso.special_attack.cost);
        assert_eq!(
            view.special_attack_command,
            UiCommand::SetMode(UiMode::SelectingTarget {
                attacker_id: captain_key(human),
                kind: AttackKind::Special,
            }),
            "the button must arm the *special* aim, not the base one"
        );

        let offered = session.valid.iter().any(|a| {
            matches!(
                a,
                GameAction::CaptainAttack {
                    is_special: Some(true),
                    ..
                }
            )
        });
        assert_eq!(
            view.can_special_attack, offered,
            "the button is live exactly while the engine offers the action"
        );
        if !offered {
            assert!(
                view.special_attack_reason.is_some(),
                "a dead ★ button must say why"
            );
        }
    }

    /// A recto captain has no ★ button at all.
    #[test]
    fn a_recto_captain_has_no_special_attack_button() {
        let session = make_session(5);
        let view = captain_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            session.human,
            session.human,
        )
        .unwrap();
        assert!(!view.show_special_attack);
        assert!(!view.can_special_attack);
    }

    // --- card detail ----------------------------------------

    #[test]
    fn a_card_detail_without_extras_hides_its_side_column() {
        let session = make_session(2);
        // Every def in the registry: at least one has no traits / synergies.
        let plain = session
            .registry
            .all_card_defs()
            .values()
            .find(|d| {
                d.traits.as_ref().is_none_or(|t| t.is_empty())
                    && d.synergies.as_ref().is_none_or(|s| s.is_empty())
            })
            .map(|d| d.id.clone());
        let Some(def_id) = plain else { return };
        let view = card_detail_view(&session.state, &session.registry, &def_id, None).unwrap();
        assert!(!view.has_side);
        assert!(view.type_line.contains('\u{00B7}'));
    }

    #[test]
    fn a_card_detail_resolves_synergy_partner_names() {
        let session = make_session(2);
        let with_synergy = session
            .registry
            .all_card_defs()
            .values()
            .find(|d| d.synergies.as_ref().is_some_and(|s| !s.is_empty()))
            .map(|d| d.id.clone());
        let Some(def_id) = with_synergy else { return };
        let view = card_detail_view(&session.state, &session.registry, &def_id, None).unwrap();
        assert!(view.has_side);
        assert!(!view.synergies.is_empty());
        assert!(view.synergies[0].contains("ATK"));
    }

    #[test]
    fn an_unknown_def_has_no_detail() {
        let session = make_session(2);
        assert!(card_detail_view(&session.state, &session.registry, "NOPE", None).is_none());
    }

    // --- confirmation ---------------------------------------

    #[test]
    fn a_confirmation_checks_the_cost_against_the_owners_volonte() {
        let mut session = make_session(9);
        let human = session.human;
        let hand = session.you().hand.clone();
        let Some(instance_id) = hand.into_iter().find(|id| {
            session
                .state
                .card(id)
                .and_then(|c| session.registry.card_def(&c.def_id))
                .is_some_and(|d| d.card_type == CardType::Event || d.card_type == CardType::Ship)
        }) else {
            return;
        };
        let def_id = session.state.card(&instance_id).unwrap().def_id.clone();
        let def = session.registry.card_def(&def_id).unwrap();
        let is_ship = def.card_type == CardType::Ship;
        let cost = def.cost;

        session.state.player_mut(human).volonte = cost;
        let rich = confirm_view(&session.state, &session.registry, &instance_id, is_ship).unwrap();
        assert!(rich.can_afford);
        assert_eq!(rich.cost, cost);
        assert_eq!(rich.is_ship, is_ship);
        assert_eq!(
            rich.confirm_command,
            UiCommand::Dispatch(if is_ship {
                GameAction::DeployShip {
                    instance_id: instance_id.clone(),
                }
            } else {
                GameAction::PlayEvent {
                    instance_id: instance_id.clone(),
                    targets: None,
                }
            })
        );

        session.state.player_mut(human).volonte = cost - 1;
        let poor = confirm_view(&session.state, &session.registry, &instance_id, is_ship).unwrap();
        assert!(!poor.can_afford);
    }

    #[test]
    fn event_effects_are_described_in_french() {
        assert_eq!(
            describe_event_confirm(&EventEffect::GainWill { amount: 2 }),
            "Gagne +2 Volonté ce tour."
        );
        assert_eq!(
            describe_event_confirm(&EventEffect::Draw {
                amount: 2,
                discard: Some(1)
            }),
            "Pioche 2 carte(s), défausse 1."
        );
        assert_eq!(
            describe_event_confirm(&EventEffect::DamageEnemies {
                amount: 3,
                target: DamageTarget::AllFront,
                cursed_bonus: None,
                sand: None,
                destroy_ships: None,
            }),
            "3 dégâts à toute la Ligne Avant ennemie."
        );
        assert_eq!(
            describe_event_confirm(&EventEffect::BuffAllies {
                stat: AtkDefStat::Atk,
                amount: 1,
                filter: None,
                duration: BuffDuration::Turn,
            }),
            "Tous les alliés +1 ATK (ce tour)."
        );
        // An effect the TSX would have JSON-stringified.
        assert_eq!(
            describe_event_confirm(&EventEffect::RushBuff { atk: 2 }),
            "Rush : +2 ATK."
        );
    }

    // --- counter window -------------------------------------

    #[test]
    fn without_a_pending_attack_there_is_no_counter_window() {
        let session = make_session(4);
        assert!(counter_view(&session.state, &session.registry, &session.valid).is_none());
    }

    #[test]
    fn a_counter_window_lists_one_button_per_counter_action() {
        let mut session = make_session(13);
        let human = session.human;
        let reached = advance(&mut session, 900, |s| {
            s.state.pending_attack.is_some()
                && s.state.current_player != human
                && !s.valid.is_empty()
        });
        if !reached {
            return; // this seed never put the human on the defensive
        }
        let view = counter_view(&session.state, &session.registry, &session.valid)
            .expect("the human can answer");
        assert!(!view.attacker.is_empty());
        assert!(!view.target.is_empty());
        assert!(view.damage >= 0);
        assert!(!view.options.is_empty());
        assert!(
            view.options.iter().any(|o| o.kind == CounterKind::Pass),
            "'Subir les dégâts' is always available"
        );
        for option in &view.options {
            // `DispatchKeepUi`: the TS counter buttons are the only dispatch
            // sites that do not call `resetUI()`.
            assert!(matches!(option.command, UiCommand::DispatchKeepUi(_)));
        }
    }

    #[test]
    fn counter_options_mirror_the_valid_actions_exactly() {
        let mut session = make_session(21);
        let human = session.human;
        if !advance(&mut session, 900, |s| {
            s.state.pending_attack.is_some()
                && s.state.current_player != human
                && !s.valid.is_empty()
        }) {
            return;
        }
        let view = counter_view(&session.state, &session.registry, &session.valid).unwrap();
        let expected = session
            .valid
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    GameAction::PlayCounter { .. }
                        | GameAction::UseShield { .. }
                        | GameAction::PassCounter
                ) || matches!(
                    a,
                    GameAction::UseHaki {
                        haki_type: HakiType::Observation,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(view.options.len(), expected);
    }

    // --- ship menu ------------------------------------------

    #[test]
    fn an_enemy_ship_menu_can_never_be_activated() {
        let mut session = make_session(17);
        let ai = session.ai_player();
        if !advance(&mut session, 900, |s| {
            s.state.player(ai).active_ship.is_some()
        }) {
            return;
        }
        let ship = session.foe().active_ship.clone().unwrap();
        let view = ship_menu_view(
            &session.state,
            &session.registry,
            &session.valid,
            &ship,
            false,
        )
        .unwrap();
        assert!(!view.is_you);
        assert!(!view.can_activate);
        assert_eq!(
            view.activate_command,
            UiCommand::Dispatch(GameAction::ActivateShip {
                ship_instance_id: ship
            })
        );
    }

    #[test]
    fn deploy_slots_are_irrelevant_to_the_panels() {
        // Guard against an accidental dependency: nothing in this module reads
        // the board geometry.
        let session = make_session(1);
        let _ = Slot::V1;
        assert!(counter_view(&session.state, &session.registry, &[]).is_none());
    }

    /// **Documented divergence from `ActionMenu.tsx`.**
    ///
    /// The TSX derives "Mal de terre" from
    /// `instance.deployedTurn === state.turnNumber && !def.traits?.includes("rush")`,
    /// i.e. from the *definition*'s traits alone. This client asks the engine
    /// ([`has_summoning_sickness`]) instead, so a Rush granted at runtime — by
    /// an equipped Devil Fruit, say — is honoured and the reason falls through
    /// to the next one.
    ///
    /// The `disabled` flag itself is untouched: it comes from `validActions` in
    /// both clients, so the two never disagree about what is *legal* — only
    /// about the sentence explaining why.
    #[test]
    fn summoning_sickness_is_asked_of_the_engine_not_of_the_traits() {
        let Some((session, id)) = deploy_one(7) else {
            return;
        };
        let view = action_menu_view(&session.state, &session.registry, &session.valid, &id)
            .expect("the unit was just deployed");
        let engine_says =
            has_summoning_sickness(&session.state, &session.registry, &id).unwrap_or(false);

        let labelled = view
            .base
            .as_ref()
            .and_then(|b| b.reason.as_deref())
            .map(|r| r == "Mal de terre")
            .unwrap_or(false)
            || view.flags.contains(&"Mal de terre");

        if engine_says {
            assert!(labelled, "the engine says sick, the menu must say so too");
        } else {
            assert!(
                !view.flags.contains(&"Mal de terre"),
                "the engine says the unit may act, so no sickness pill"
            );
        }

        // Whatever the label says, each row's own `disabled` mirrors the
        // engine — and the two base rows are gated separately.
        if let Some(base) = &view.base {
            let can_attack = session.valid.iter().any(|a| {
                matches!(a, GameAction::BaseAttack { attacker_instance_id, .. } if attacker_instance_id == &id)
            });
            assert_eq!(base.disabled, !can_attack);
        }
        if let Some(support) = &view.support {
            let can_support = session.valid.iter().any(|a| {
                matches!(a, GameAction::BaseSupportAction { instance_id, .. } if instance_id == &id)
            });
            assert_eq!(support.disabled, !can_support);
        }
    }

    /// §8.38 — a support character's effect and its attack are **both** legal
    /// in the same turn (`actions.rs`: "even support chars … do their effect +
    /// attack"), so the menu has to carry two rows. Fusing them, as the TSX
    /// did, made one of the two engine actions unreachable.
    #[test]
    fn a_support_character_gets_its_effect_and_its_attack_as_two_rows() {
        use tcgop_engine::state::CardInstance;
        use tcgop_engine::types::{PlayerId, Zone};

        let mut session = make_session(8);
        let human = session.human;
        assert!(advance(&mut session, 200, |s: &Session| {
            s.state.current_player == human && s.state.pending_attack.is_none()
        }));
        session.state.players.get_mut(human).volonte = 10;

        // `MG-004` Chopper: `isSupport` heal *and* ATK 1.
        let mut put = |def_id: &str, owner: PlayerId, slot: Slot| {
            let pv = session
                .registry
                .card_def(def_id)
                .and_then(|d| d.pv)
                .unwrap_or(1);
            let id = session.ctx.generate_instance_id(def_id);
            let mut instance = CardInstance::new(id.clone(), def_id.to_string(), owner, pv);
            instance.zone = Zone::Board;
            instance.slot = Some(slot);
            instance.deployed_turn = Some(0);
            session.state.cards.insert(id.clone(), instance);
            *session.state.players.get_mut(owner).board.slot_mut(slot) = Some(id.clone());
            id
        };
        let chopper = put("MG-004", human, Slot::V1);
        // Someone to heal and someone to hit.
        let hurt = put("MG-002", human, Slot::V2);
        put("MR-001", human.opponent(), Slot::V1);
        session.state.card_mut(&hurt).unwrap().current_pv = 1;
        session.refresh_valid();

        let offers_attack = session.valid.iter().any(
            |a| matches!(a, GameAction::BaseAttack { attacker_instance_id, .. } if *attacker_instance_id == chopper),
        );
        let offers_support = session.valid.iter().any(
            |a| matches!(a, GameAction::BaseSupportAction { instance_id, .. } if *instance_id == chopper),
        );
        assert!(
            offers_attack && offers_support,
            "the engine offers both to a support character: {:?}",
            session.valid
        );

        let view = action_menu_view(&session.state, &session.registry, &session.valid, &chopper)
            .expect("the unit has a menu");
        let base = view.base.expect("the attack row");
        let support = view.support.expect("the support row");
        assert!(!base.disabled && !support.disabled);
        assert_eq!(
            base.command,
            UiCommand::SetMode(UiMode::SelectingTarget {
                attacker_id: chopper.clone(),
                kind: AttackKind::Base,
            }),
            "the attack row aims a base attack"
        );
        assert!(
            matches!(
                support.command,
                UiCommand::SetMode(UiMode::SelectingSupportTarget { .. })
                    | UiCommand::Dispatch(GameAction::BaseSupportAction { .. })
            ),
            "the support row plays the support action: {:?}",
            support.command
        );
    }

    /// §8.38 — a taunted unit is bound to its taunter, and the menu says which
    /// one instead of the misleading "Pas de cible".
    #[test]
    fn a_taunted_unit_is_told_who_it_must_hit() {
        use tcgop_engine::state::CardInstance;
        use tcgop_engine::types::{StatusEffect, Zone};

        let mut session = make_session(6);
        let human = session.human;
        assert!(advance(&mut session, 200, |s: &Session| {
            s.state.current_player == human && s.state.pending_attack.is_none()
        }));

        let id = session.ctx.generate_instance_id("MG-002");
        let mut zoro = CardInstance::new(id.clone(), "MG-002".into(), human, 5);
        zoro.zone = Zone::Board;
        zoro.slot = Some(Slot::V1);
        zoro.deployed_turn = Some(0);
        // A taunter that is *not* on the board: nothing is a legal target, so
        // the row is dead and the reason has to be the taunt.
        zoro.status_effects.push(StatusEffect {
            effect_type: StatusEffectType::Taunt,
            turns_remaining: 1,
            damage_per_turn: 0,
            source: "absent_taunter".into(),
        });
        session.state.cards.insert(id.clone(), zoro);
        *session
            .state
            .players
            .get_mut(human)
            .board
            .slot_mut(Slot::V1) = Some(id.clone());
        session.refresh_valid();

        let view = action_menu_view(&session.state, &session.registry, &session.valid, &id)
            .expect("the unit has a menu");
        assert!(view.flags.contains(&"Provoqué"));
        assert_eq!(
            view.taunt.as_deref(),
            Some("Provoqué : doit cibler absent_taunter"),
            "the menu names the unit the taunt binds this one to"
        );

        // And with the taunter actually on the board, the note names the card,
        // and the engine really does narrow the target list to it.
        let taunter = session.ctx.generate_instance_id("MR-001");
        let mut foe = CardInstance::new(taunter.clone(), "MR-001".into(), human.opponent(), 4);
        foe.zone = Zone::Board;
        foe.slot = Some(Slot::V1);
        foe.deployed_turn = Some(0);
        session.state.cards.insert(taunter.clone(), foe);
        *session
            .state
            .players
            .get_mut(human.opponent())
            .board
            .slot_mut(Slot::V1) = Some(taunter.clone());
        session
            .state
            .card_mut(&id)
            .unwrap()
            .status_effects
            .iter_mut()
            .for_each(|e| e.source = taunter.clone());
        session.refresh_valid();

        let view = action_menu_view(&session.state, &session.registry, &session.valid, &id)
            .expect("the unit has a menu");
        let name = session.registry.card_def("MR-001").unwrap().name.clone();
        assert_eq!(
            view.taunt.as_deref(),
            Some(format!("Provoqué : doit cibler {name}").as_str())
        );
        let targets: Vec<&String> = session
            .valid
            .iter()
            .filter_map(|a| match a {
                GameAction::BaseAttack {
                    attacker_instance_id,
                    target_instance_id,
                    ..
                } if *attacker_instance_id == id => Some(target_instance_id),
                _ => None,
            })
            .collect();
        assert_eq!(targets, vec![&taunter], "the taunter is the only target");
    }

    /// §8.36 — a "single" event has **no** target to pick: `playEvent` carries
    /// `targets: None` and the executor never reads the field, so the engine's
    /// own convention chooses. The confirmation states that rule; offering a
    /// picker would send a choice the engine throws away.
    #[test]
    fn a_single_target_event_states_the_rule_instead_of_asking() {
        use tcgop_engine::types::{DamageTarget, EventEffect};

        let damage = describe_event_confirm(&EventEffect::DamageEnemies {
            amount: 4,
            target: DamageTarget::Single,
            cursed_bonus: None,
            sand: Some(true),
            destroy_ships: None,
        });
        assert!(
            damage.contains("le plus fort de la Ligne Avant"),
            "the pick has to be spelled out: {damage}"
        );
        assert!(
            damage.contains("PV maximum"),
            "and `sand` is a permanent maximum-PV loss, not one more damage: {damage}"
        );

        let heal = describe_event_confirm(&EventEffect::HealAlly {
            amount: 3,
            all_allies: None,
        });
        assert!(
            heal.contains("le plus blessé"),
            "the heal names its own pick: {heal}"
        );

        // And the engine really does hand the client a target-less action.
        let session = make_session(2);
        assert!(
            session.valid.iter().all(|a| !matches!(
                a,
                GameAction::PlayEvent {
                    targets: Some(_),
                    ..
                }
            )),
            "the enumerator never fills `targets`"
        );
    }

    /// §8.42 — natural Haki is what pierces a Logia, and it is printed in two
    /// forms; the menu surfaces both.
    #[test]
    fn natural_haki_is_shown_on_the_units_that_have_it() {
        let (session, id) = deploy_one(11).expect("seed 11 must allow a deploy");
        let view =
            action_menu_view(&session.state, &session.registry, &session.valid, &id).unwrap();
        let def = session.registry.card_def(&view.def_id).unwrap();
        assert_eq!(
            view.flags.contains(&"Haki naturel"),
            tcgop_engine::haki::def_has_natural_haki(def),
            "the pill must agree with the engine's own predicate"
        );
    }
}
