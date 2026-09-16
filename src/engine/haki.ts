import { produce } from "immer";
import type { GameState, PlayerId, HakiType, CardDef } from "@/types";
import { addLog, getOpponent } from "./gameState";
import { getBoardCharacters, getEffectiveDef, removeFromBoard } from "./board";
import { getCaptainDef } from "./cardRegistry";
import { grantKOBonus } from "./volonte";

/** Haki unlock thresholds (manche à partir de laquelle le Haki existe) */
export const HAKI_THRESHOLDS: Record<HakiType, number> = {
  observation: 5,
  armament: 7,
  king: 10,
};

/** Libellé et règle affichés au joueur — source unique, lue par l'UI. */
export const HAKI_INFO: Record<
  HakiType,
  { label: string; glyph: string; effect: string; limit: string }
> = {
  observation: {
    label: "Haki de l'Observation",
    glyph: "👁",
    effect: "En défense, esquive entièrement une attaque qui te vise.",
    limit: "Une fois par manche. Sans coût en Volonté.",
  },
  armament: {
    label: "Haki de l'Armement",
    glyph: "✊",
    effect: "Passif : tes attaques touchent les Logia, normalement intouchables.",
    limit: "Toujours actif une fois débloqué. Rien à activer.",
  },
  king: {
    label: "Haki des Rois",
    glyph: "👑",
    effect: "Met KO tous les personnages ennemis de DEF inférieure ou égale à 3.",
    limit: "Une fois par partie. Exige une unité Conquérant en jeu.",
  },
};

/** État complet d'un Haki pour un joueur, tel que l'UI doit l'afficher. */
export interface HakiStatus {
  type: HakiType;
  label: string;
  glyph: string;
  effect: string;
  limit: string;
  /** Manche de déblocage. */
  unlockTurn: number;
  /** La manche courante a atteint le seuil. */
  unlocked: boolean;
  /** Manches restantes avant déblocage (0 si débloqué). */
  turnsUntilUnlock: number;
  /** Utilisable immédiatement (hors fenêtre de contre pour l'Observation). */
  available: boolean;
  /** Déjà dépensé (Observation : cette manche ; Rois : cette partie). */
  spent: boolean;
  /** Pourquoi ce Haki n'est pas utilisable, en clair. `null` s'il l'est. */
  blockedReason: string | null;
}

/**
 * Décrit les trois Haki pour un joueur. L'UI ne recalcule jamais les règles :
 * elle affiche ce que le moteur applique réellement.
 */
export function getHakiStatus(state: GameState, playerId: PlayerId): HakiStatus[] {
  const player = state.players[playerId];

  return (Object.keys(HAKI_THRESHOLDS) as HakiType[]).map((type) => {
    const unlockTurn = HAKI_THRESHOLDS[type];
    const unlocked = state.turnNumber >= unlockTurn;
    const turnsUntilUnlock = Math.max(0, unlockTurn - state.turnNumber);

    const spent =
      type === "observation" ? player.observationUsed : type === "king" ? player.kingUsed : false;

    let blockedReason: string | null = null;
    if (!unlocked) {
      blockedReason = `Débloqué à la manche ${unlockTurn} (encore ${turnsUntilUnlock})`;
    } else if (type === "armament") {
      blockedReason = null; // passif : actif, rien à activer
    } else if (spent) {
      blockedReason = type === "king" ? "Déjà utilisé cette partie" : "Déjà utilisé cette manche";
    } else if (type === "king" && !hasConquerorInPlay(state, playerId)) {
      blockedReason = "Aucune unité Conquérant en jeu";
    }

    // `available` suit isHakiAvailable, qui renvoie false pour l'Armement (passif).
    const available =
      type === "armament" ? unlocked : blockedReason === null && isHakiAvailable(state, playerId, type);

    return {
      type,
      ...HAKI_INFO[type],
      unlockTurn,
      unlocked,
      turnsUntilUnlock,
      available,
      spent,
      blockedReason,
    };
  });
}

/**
 * Decision §8.42 — both `naturalHaki` forms are honoured: the `CardDef.naturalHaki`
 * array **and** the `PassiveEffect { type: "naturalHaki" }` passive. The two
 * shipped carriers (MR-002 Garde du Corps, MR-004 Poing de Garp, RH-002 Haki
 * d'Équipage) declare both, so reading the passive is inert on the shipped
 * catalogue and only removes a trap for future cards.
 * Rust: `haki::def_has_natural_haki`.
 */
export function defHasNaturalHaki(def: CardDef): boolean {
  if (def.naturalHaki && def.naturalHaki.length > 0) return true;
  return def.passive?.effects.some((e) => e.type === "naturalHaki") ?? false;
}

/** Check if a Haki type is available this turn */
/**
 * Decision §8.64 — « Vos personnages beneficient de l'esquive Observation
 * (1x/tour), meme avant le tour 5 » (RH-001 Ben Beckman).
 *
 * Le passif `grantObservationAll` etait declare dans les types et sur la carte,
 * mais AUCUN code ne le lisait : le seuil de manche etait inconditionnel. Le
 * passif leve le seuil, rien d'autre — l'esquive reste une fois par manche et
 * gratuite, et Beckman doit etre en jeu.
 */
export function hasObservationGrant(state: GameState, playerId: PlayerId): boolean {
  const { getCardDef } = require("./cardRegistry");
  return getBoardCharacters(state, playerId).some((c) => {
    const d = getCardDef(c.defId) as CardDef;
    return d.passive?.effects.some((e) => e.type === "grantObservationAll") ?? false;
  });
}

export function isHakiAvailable(
  state: GameState,
  playerId: PlayerId,
  hakiType: HakiType
): boolean {
  const player = state.players[playerId];
  const thresholdLifted = hakiType === "observation" && hasObservationGrant(state, playerId);
  if (!thresholdLifted && state.turnNumber < HAKI_THRESHOLDS[hakiType]) return false;

  switch (hakiType) {
    case "observation":
      return !player.observationUsed;
    case "armament":
      // Armament is a passive from T7+ (Rulebook v3.1 §7) — never "used".
      return false;
    case "king":
      return !player.kingUsed;
  }
}

/** Use Observation Haki: dodge 1 attack (called during counter window) */
export function useObservationHaki(
  state: GameState,
  playerId: PlayerId
): GameState {
  if (!isHakiAvailable(state, playerId, "observation")) {
    throw new Error("Observation Haki not available");
  }
  if (!state.pendingAttack) throw new Error("No pending attack to dodge");
  if (state.pendingAttack.cannotBeDodged) {
    throw new Error("This attack cannot be dodged");
  }

  let next = produce(state, (draft) => {
    draft.players[playerId].observationUsed = true;
    // Cancel the pending attack
    draft.pendingAttack = null;
  });

  return addLog(next, playerId, `Haki de l'Observation ! Attaque esquivee !`);
}

/** Does this player control a Conqueror unit (board character or captain)? */
export function hasConquerorInPlay(state: GameState, playerId: PlayerId): boolean {
  const captain = state.players[playerId].captain;
  const capDef = getCaptainDef(captain.defId);
  const capTraits = captain.flipped ? capDef.verso.traits : capDef.traits;
  if (capTraits?.includes("conqueror")) return true;
  if (capDef.traits?.includes("conqueror")) return true;

  return getBoardCharacters(state, playerId).some((c) => {
    const def = require("./cardRegistry").getCardDef(c.defId);
    return def.traits?.includes("conqueror");
  });
}

/**
 * Use Roi Haki (T10+): KO every enemy character with effective DEF <= 3. 1x/game.
 * Requires a Conqueror unit in play (Rulebook v3.1 §7).
 */
export function useKingHaki(state: GameState, playerId: PlayerId): GameState {
  if (!isHakiAvailable(state, playerId, "king")) {
    throw new Error("Roi Haki not available");
  }
  if (!hasConquerorInPlay(state, playerId)) {
    throw new Error("Roi Haki requires a Conquerant unit in play");
  }

  const opponentId = getOpponent(playerId);
  let next = produce(state, (draft) => {
    draft.players[playerId].kingUsed = true;
  });
  next = addLog(next, playerId, `👑 Haki des Rois ! Tous les ennemis DEF ≤ 3 sont KO !`);

  const { applyOnKOEffects } = require("./passives");
  // Snapshot victims first (board mutates as we remove them).
  const victims = getBoardCharacters(next, opponentId)
    .filter((c) => getEffectiveDef(next, c.instanceId) <= 3)
    .map((c) => ({ id: c.instanceId, defId: c.defId, owner: c.owner }));

  for (const v of victims) {
    const card = next.cards[v.id];
    if (!card || card.zone !== "board") continue;
    next = addLog(next, v.owner, `${require("./cardRegistry").getCardDef(v.defId).name} est KO (Haki des Rois) !`);
    next = grantKOBonus(next, v.owner);
    next = removeFromBoard(next, v.id);
    next = applyOnKOEffects(next, v.owner, playerId, v.defId);
  }

  return next;
}
