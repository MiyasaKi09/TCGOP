// ============================================================
// Play announcements — turn each GameAction into a centred card
// reveal + a short French caption so every play (both sides) is legible.
// ============================================================

import type { GameAction, GameState, PlayerId, CardDef } from "@/types";
import { getCardDef } from "@/engine/cardRegistry";
import { getOpponent } from "@/engine/gameState";

export interface PlayAnnouncement {
  id: number;
  side: "you" | "foe";
  defId: string | null;   // null → text-only toast
  instanceId?: string;
  kind: string;           // action.type
  destId?: string;        // data-inst key to fly toward
  caption: string;
  big: boolean;           // full card vs compact
  toast: boolean;         // no card, just a banner
}

let counter = 1;

function safeDef(state: GameState, instId?: string): CardDef | null {
  if (!instId) return null;
  try { return getCardDef(state.cards[instId].defId); } catch { return null; }
}

function describeEvent(e: NonNullable<CardDef["eventEffect"]>): string {
  switch (e.type) {
    case "gainWill": return `Gagne +${e.amount} Volonté.`;
    case "draw": return `Pioche ${e.amount}${e.discard ? `, défausse ${e.discard}` : ""}.`;
    case "healAlly": return e.allAllies ? `Tous les alliés +${e.amount} PV.` : `1 allié +${e.amount} PV.`;
    case "buffAllies": return `Alliés +${e.amount} ${e.stat.toUpperCase()}.`;
    case "rally": return `Ralliement : +ATK/+DEF et soin.`;
    case "buffSingle": return `Un allié +${e.amount} ${e.stat.toUpperCase()}.`;
    case "rushBuff": return `Un allié gagne Rush +${e.atk} ATK.`;
    case "damageEnemies": return `${e.amount} dégâts (${e.target}).`;
    case "tutor": return `Cherche un personnage.`;
    case "deployTokens": return `Déploie ${e.count} jeton(s).`;
    case "healAllBuff": return `Soigne ${e.heal} et +${e.atk} ATK.`;
    case "debuffAllEnemies": return `Ennemis −${e.atk} ATK.`;
    case "grantHakiAll": return `Vos attaques gagnent le Haki.`;
    case "dodgeAll": return "Esquive totale ce tour.";
    default: return (e as { description?: string }).description ?? "";
  }
}

function describeCounter(ce: NonNullable<CardDef["counterEffect"]>): string {
  switch (ce.type) {
    case "reduceDamage": return `Réduit les dégâts de ${ce.amount}.`;
    case "survive": return `Survit avec 1 PV.`;
    case "cancel": return `Annule l'attaque.`;
    case "untargetable": return `Devient inciblable.`;
  }
}

function actorOf(action: GameAction, state: GameState): PlayerId {
  const cur = state.currentPlayer;
  if (state.pendingAttack) {
    if (action.type === "playCounter" || action.type === "useShield" || action.type === "passCounter") return getOpponent(cur);
    if (action.type === "useHaki" && action.hakiType === "observation") return getOpponent(cur);
  }
  return cur;
}

/** Build a reveal from an action + the PRE-execution state, or null to skip. */
export function buildAnnouncement(
  action: GameAction,
  state: GameState,
  humanPlayer: PlayerId
): PlayAnnouncement | null {
  const actor = actorOf(action, state);
  const side: "you" | "foe" = actor === humanPlayer ? "you" : "foe";
  const base = { id: counter++, side, big: true, toast: false } as const;

  switch (action.type) {
    case "deployCharacter": {
      const def = safeDef(state, action.instanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.instanceId, kind: action.type, destId: action.instanceId, caption: `Déploie ${def.name} en ${action.slot}` };
    }
    case "deployShip": {
      const def = safeDef(state, action.instanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.instanceId, kind: action.type, caption: `Déploie le navire ${def.name}` };
    }
    case "equipObject": {
      const def = safeDef(state, action.objectInstanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.objectInstanceId, kind: action.type, destId: action.targetInstanceId, caption: `Équipe ${def.name}` };
    }
    case "playEvent": {
      const def = safeDef(state, action.instanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.instanceId, kind: action.type, caption: def.eventEffect ? describeEvent(def.eventEffect) : def.name };
    }
    case "playCounter": {
      const def = safeDef(state, action.instanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.instanceId, kind: action.type, caption: def.counterEffect ? describeCounter(def.counterEffect) : def.name };
    }
    case "useShield": {
      const def = safeDef(state, action.blockerInstanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.blockerInstanceId, kind: action.type, destId: action.blockerInstanceId, caption: `Bloque avec ${def.name} (Bouclier)` };
    }
    case "activateShip": {
      const def = safeDef(state, action.shipInstanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.shipInstanceId, kind: action.type, caption: `Active ${def.shipActive?.name ?? def.name}` };
    }
    case "awakenFruit": {
      const def = safeDef(state, action.fruitInstanceId);
      if (!def) return null;
      return { ...base, defId: def.id, instanceId: action.fruitInstanceId, kind: action.type, caption: `Éveille ${def.name} !` };
    }
    case "baseAttack":
    case "specialAttack": {
      const def = safeDef(state, action.attackerInstanceId);
      if (!def) return null;
      const name = action.type === "specialAttack" ? (def.specialAttack?.name ?? def.name) : def.name;
      return { ...base, big: false, defId: def.id, instanceId: action.attackerInstanceId, kind: action.type, destId: action.attackerInstanceId, caption: action.type === "specialAttack" ? `Spéciale : ${name}` : `${def.name} attaque` };
    }
    case "fruitSpecialAttack": {
      const def = safeDef(state, action.fruitInstanceId);
      if (!def) return null;
      return { ...base, big: false, defId: def.id, instanceId: action.fruitInstanceId, kind: action.type, destId: action.attackerInstanceId, caption: `Éveil : ${def.fruitEffects?.awakening?.specialAttack?.name ?? def.name}` };
    }
    case "moveCharacter": {
      const def = safeDef(state, action.instanceId);
      if (!def) return null;
      return { ...base, big: false, defId: def.id, instanceId: action.instanceId, kind: action.type, destId: action.instanceId, caption: `Déplace ${def.name}` };
    }
    case "flipCaptain":
      return { ...base, defId: null, kind: action.type, destId: `captain_${actor}`, caption: `Retourne son Capitaine !`, toast: true };
    case "captainAttack":
      return { ...base, big: false, defId: null, kind: action.type, destId: `captain_${actor}`, caption: `Le Capitaine attaque`, toast: true };
    case "useHaki":
      return { ...base, defId: null, kind: action.type, caption: action.hakiType === "king" ? `👑 Haki des Rois !` : action.hakiType === "observation" ? `Esquive (Haki Observation)` : `Haki`, toast: true };
    case "passCounter":
      return { ...base, big: false, defId: null, kind: action.type, caption: side === "foe" ? `L'adversaire encaisse` : `Vous encaissez`, toast: true };
    case "endTurn":
      return { ...base, big: false, defId: null, kind: action.type, caption: `Fin de tour`, toast: true };
    default:
      return null;
  }
}

/** Per-kind reveal duration in ms (also used to pace the AI loop). */
export function announceDuration(a: PlayAnnouncement): number {
  if (a.toast) return a.kind === "endTurn" ? 550 : 750;
  if (!a.big) return 700;
  return 1150;
}
