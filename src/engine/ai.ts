import type { GameState, GameAction, PlayerId } from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import { getValidActions, executeAction } from "./turnManager";
import {
  getBoardCharacters,
  getEffectiveAtk,
  getEffectiveDef,
  hasFrontRow,
} from "./board";
import { getOpponent } from "./gameState";

export type Difficulty = "beginner" | "intermediate" | "expert";

/**
 * AI chooses an action. Three levels:
 * - beginner: noisy, passive, mostly basic actions and passes on defense
 * - intermediate: the score heuristic
 * - expert: 1-ply lookahead over a board-evaluation function
 */
export function aiChooseAction(
  state: GameState,
  playerId: PlayerId,
  difficulty: Difficulty = "intermediate"
): GameAction {
  const actions = getValidActions(state, playerId);
  if (actions.length === 0) return { type: "endTurn" };
  if (actions.length === 1) return actions[0];
  if (difficulty === "beginner") return chooseBeginner(state, playerId, actions);
  if (difficulty === "expert") return chooseExpert(state, playerId, actions);
  return chooseByScore(state, playerId, actions, 0);
}

function chooseByScore(state: GameState, playerId: PlayerId, actions: GameAction[], jitter: number): GameAction {
  let best = actions[0];
  let bestScore = -Infinity;
  for (const action of actions) {
    const score = scoreAction(state, playerId, action) + (jitter > 0 ? Math.random() * jitter : 0);
    if (score > bestScore) { bestScore = score; best = action; }
  }
  return best;
}

const BASIC = new Set(["deployCharacter", "baseAttack", "moveCharacter", "endTurn", "passCounter", "playCounter", "useShield"]);

function chooseBeginner(state: GameState, playerId: PlayerId, actions: GameAction[]): GameAction {
  // Defense: usually just take the hit.
  if (state.pendingAttack) {
    const pass = actions.find((a) => a.type === "passCounter");
    if (pass && Math.random() < 0.8) return pass;
  }
  const simple = actions.filter((a) => BASIC.has(a.type));
  const pool = simple.length ? simple : actions;
  // Often plays almost randomly; otherwise a very noisy heuristic.
  if (Math.random() < 0.35) return pool[Math.floor(Math.random() * pool.length)];
  return chooseByScore(state, playerId, pool, 7);
}

function chooseExpert(state: GameState, playerId: PlayerId, actions: GameAction[]): GameAction {
  let best = actions[0];
  let bestVal = -Infinity;
  for (const action of actions) {
    let val: number;
    try {
      const ns = executeAction(state, action);
      val = evaluateState(ns, playerId);
    } catch {
      val = -Infinity;
    }
    // small heuristic tie-break; rank ending the turn last.
    val += scoreAction(state, playerId, action) * 0.02;
    if (action.type === "endTurn") val -= 6;
    if (val > bestVal) { bestVal = val; best = action; }
  }
  return best;
}

/** Static evaluation of a position from playerId's point of view (higher = better). */
export function evaluateState(state: GameState, playerId: PlayerId): number {
  const opp = getOpponent(playerId);
  const me = state.players[playerId];
  const op = state.players[opp];
  let v = 0;

  v += (me.captain.currentPv - op.captain.currentPv) * 1.5;
  if (op.captain.currentPv <= 0) v += 1000;
  if (me.captain.currentPv <= 0) v -= 1000;
  if (state.winner === playerId) v += 1000;
  if (state.winner === opp) v -= 1000;

  for (const c of getBoardCharacters(state, playerId)) {
    v += getEffectiveAtk(state, c.instanceId) + getEffectiveDef(state, c.instanceId) + c.currentPv * 0.5 + 3;
  }
  for (const c of getBoardCharacters(state, opp)) {
    v -= getEffectiveAtk(state, c.instanceId) + getEffectiveDef(state, c.instanceId) + c.currentPv * 0.5 + 3;
  }

  v += me.hand.length * 1.4 - op.hand.length * 1.0;
  v += me.volonte * 0.4;
  return v;
}

function scoreAction(
  state: GameState,
  playerId: PlayerId,
  action: GameAction
): number {
  switch (action.type) {
    case "deployCharacter":
      return scoreDeployCharacter(state, playerId, action);
    case "deployShip":
      return 15; // Ships are usually good
    case "equipObject":
      return scoreEquipObject(state, action);
    case "baseAttack":
    case "specialAttack":
      return scoreAttack(state, playerId, action);
    case "captainAttack":
      return scoreAttack(state, playerId, action);
    case "playEvent":
      return scoreEvent(state, playerId, action);
    case "playCounter":
      return 50; // Counters during enemy turn are almost always good
    case "useShield":
      return scoreShield(state, action);
    case "passCounter":
      return 0; // Default — pass if no counter
    case "flipCaptain":
      return scoreCaptainFlip(state, playerId);
    case "useHaki":
      return action.hakiType === "observation" ? 40 : action.hakiType === "king" ? 60 : 10;
    case "moveCharacter":
      return 2; // Low priority
    case "endTurn":
      return -1; // Last resort
    default:
      return 0;
  }
}

function scoreDeployCharacter(
  state: GameState,
  playerId: PlayerId,
  action: Extract<GameAction, { type: "deployCharacter" }>
): number {
  const card = state.cards[action.instanceId];
  if (!card) return 0;
  const def = getCardDef(card.defId);

  let score = 10; // Base score for deploying

  // Higher cost cards are generally stronger
  score += (def.atk ?? 0) * 2;
  score += (def.pv ?? 0);

  // Prefer deploying if we have few board characters
  const boardCount = getBoardCharacters(state, playerId).length;
  if (boardCount < 3) score += 10;
  if (boardCount === 0) score += 20;

  // Prefer support in back, fighters in front
  const isFrontSlot = ["V1", "V2", "V3"].includes(action.slot);
  if (def.preferredRow === "front" && isFrontSlot) score += 3;
  if (def.preferredRow === "back" && !isFrontSlot) score += 3;

  return score;
}

function scoreEquipObject(
  state: GameState,
  action: Extract<GameAction, { type: "equipObject" }>
): number {
  const objDef = getCardDef(state.cards[action.objectInstanceId].defId);
  let score = 8;
  score += (objDef.bonusAtk ?? 0) * 3;
  return score;
}

function scoreAttack(
  state: GameState,
  playerId: PlayerId,
  action: GameAction
): number {
  let score = 5;
  const opponentId = getOpponent(playerId);

  // Captain attacks are high value
  if ("targetIsCaptain" in action && action.targetIsCaptain) {
    score += 15;
  }

  // Check if we can KO the target
  if ("attackerInstanceId" in action && "targetInstanceId" in action) {
    if (!("targetIsCaptain" in action && action.targetIsCaptain)) {
      const targetId = (action as { targetInstanceId: string }).targetInstanceId;
      const target = state.cards[targetId];
      if (target) {
        const attackerAtk = "attackerInstanceId" in action
          ? getEffectiveAtk(state, (action as { attackerInstanceId: string }).attackerInstanceId)
          : 0;
        const targetDefVal = getEffectiveDef(state, targetId);
        const damage = Math.max(0, attackerAtk - targetDefVal);
        if (damage >= target.currentPv) {
          score += 20; // Can KO!
        }
        // Prefer targets with low PV
        score += Math.max(0, 5 - target.currentPv);
      }
    }
  }

  // Special attacks are big commitments — slightly lower base score
  if (action.type === "specialAttack") {
    score += 5; // But they do more damage
  }

  return score;
}

function scoreEvent(
  state: GameState,
  playerId: PlayerId,
  action: Extract<GameAction, { type: "playEvent" }>
): number {
  const card = state.cards[action.instanceId];
  if (!card) return 0;
  const def = getCardDef(card.defId);
  const effect = def.eventEffect;
  if (!effect) return 5;

  switch (effect.type) {
    case "gainWill":
      return 18; // Very good early
    case "draw":
      return 14;
    case "healAlly":
      return 8;
    case "buffAllies":
      return 12 + getBoardCharacters(state, playerId).length * 2;
    case "damageEnemies":
      return 15 + effect.amount * 2;
    case "dodgeAll":
      return 3; // Defensive, AI rarely needs
    default:
      return 6;
  }
}

function scoreShield(
  state: GameState,
  action: Extract<GameAction, { type: "useShield" }>
): number {
  const pending = state.pendingAttack;
  if (!pending) return 0;
  const blockerDef = getEffectiveDef(state, action.blockerInstanceId);
  const blocker = state.cards[action.blockerInstanceId];
  if (!blocker) return 0;
  const atkPower = pending.attackPower ?? pending.rawDamage;
  const redirected = Math.max(0, atkPower - blockerDef);

  // Worthwhile if the block saves the original target from a KO and the blocker survives.
  const savesKO = !pending.targetIsCaptain
    ? (state.cards[pending.targetId]?.currentPv ?? 0) <= pending.rawDamage
    : pending.rawDamage >= 6; // protecting the captain from a heavy hit
  const blockerSurvives = redirected < blocker.currentPv;

  if (savesKO && blockerSurvives) return 32;
  if (redirected === 0) return 20; // fully negated, free
  if (redirected < pending.rawDamage && blockerSurvives) return 6;
  return 0;
}

function scoreCaptainFlip(
  state: GameState,
  playerId: PlayerId
): number {
  const boardCount = getBoardCharacters(state, playerId).length;
  // Never flip before turn 4
  if (state.turnNumber < 4) return -10;
  // Flip when desperate (0-1 allies) and late game
  if (boardCount === 0 && state.turnNumber >= 5) return 30;
  if (boardCount <= 1 && state.turnNumber >= 6) return 25;
  if (state.turnNumber >= 7) return 15;
  if (state.turnNumber >= 5 && boardCount <= 2) return 10;
  return -5;
}
