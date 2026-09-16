import { produce } from "immer";
import type { GameState, PlayerId, Slot, PassiveEffect, AllyFilter } from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import { getBoardCharacters, getAdjacentSlots, getSlotOf, shipPassiveScope, shipScopeMatches } from "./board";
import { addLog, getOpponent } from "./gameState";
import { ALL_SLOTS } from "./utils";

// ============================================================
// Start-of-turn passive effects
// ============================================================

/**
 * Apply all start-of-turn passives for the current player.
 * Called during startTurn after untap/draw/volonte.
 */
export function applyStartOfTurnPassives(
  state: GameState,
  playerId: PlayerId
): GameState {
  let next = state;
  const player = state.players[playerId];

  // Process each board character's passives
  for (const slot of ALL_SLOTS) {
    const charId = player.board[slot as Slot];
    if (!charId) continue;
    const card = state.cards[charId];
    if (!card) continue;
    const def = getCardDef(card.defId);
    if (!def.passive) continue;

    for (const effect of def.passive.effects) {
      next = resolveStartOfTurnEffect(next, playerId, charId, slot as Slot, effect);
    }
  }

  // Process captain passives (recto or verso)
  next = applyCaptainStartOfTurn(next, playerId);

  return next;
}

function resolveStartOfTurnEffect(
  state: GameState,
  playerId: PlayerId,
  sourceId: string,
  sourceSlot: Slot,
  effect: PassiveEffect
): GameState {
  switch (effect.type) {
    case "healAdjacent": {
      const adjacent = getAdjacentSlots(sourceSlot);
      const player = state.players[playerId];
      const sourceDef = getCardDef(state.cards[sourceId].defId);
      return produce(state, (draft) => {
        for (const adjSlot of adjacent) {
          const adjId = player.board[adjSlot as Slot];
          if (!adjId) continue;
          const adjCard = draft.cards[adjId];
          if (!adjCard) continue;
          // Decision §8.5: the single heal path — `noHeal` / `desiccation` skip
          // the unit, the cap follows the permanent max-PV loss, and a heal
          // never lowers PV.
          if (adjCard.statusEffects.some((e) => e.type === "noHeal" || e.type === "desiccation")) continue;
          const adjDef = getCardDef(adjCard.defId);
          const maxPv = adjDef.pv === undefined ? adjCard.currentPv : adjDef.pv - (adjCard.pvMaxLoss ?? 0);
          adjCard.currentPv = Math.max(Math.min(adjCard.currentPv + effect.amount, maxPv), adjCard.currentPv);
        }
        draft.log.push({
          turn: draft.turnNumber,
          player: playerId,
          message: `${sourceDef.name} soigne ${effect.amount} PV aux adjacents`,
        });
      });
    }
    case "startTurnBuffAlly": {
      // Usopp Vantardise: give one ally (highest ATK, not the source) +amount this turn.
      const { getEffectiveAtk } = require("./board");
      const player = state.players[playerId];
      const sourceDef = getCardDef(state.cards[sourceId].defId);
      let bestId: string | null = null;
      let bestAtk = -1;
      for (const slot of ALL_SLOTS) {
        const id = player.board[slot as Slot];
        if (!id || id === sourceId) continue;
        const a = getEffectiveAtk(state, id);
        if (a > bestAtk) { bestAtk = a; bestId = id; }
      }
      if (!bestId) return state;
      const targetId = bestId;
      return produce(state, (draft) => {
        draft.cards[targetId].modifiers.push({
          id: `vantardise_${targetId}_${Date.now()}`,
          stat: effect.stat, amount: effect.amount,
          // Decision §8.2 : la source etait `passive_<id>`, donc le
          // `recalculatePassiveBuffs` qui clot `startTurn` (et tout deploy /
          // equip / KO / bascule ulterieur) l'effacait aussitot. Source propre,
          // identifiant inchange ; le modificateur reste « turn ».
          source: `vantardise_${sourceId}`, duration: "turn",
        });
        draft.log.push({ turn: draft.turnNumber, player: playerId, message: `${sourceDef.name} : Vantardise — un allié gagne +${effect.amount} ${effect.stat.toUpperCase()} ce tour.` });
      });
    }
    default:
      return state;
  }
}

function applyCaptainStartOfTurn(
  state: GameState,
  playerId: PlayerId
): GameState {
  // Captain passives that trigger at start of turn are rare
  // Most captain passives are continuous (handled by recalculatePassiveBuffs)
  return state;
}

// ============================================================
// Recalculate all passive buffs (continuous effects)
// Called after deploy, KO, equip, captain flip
// ============================================================

/**
 * Remove old passive modifiers and reapply all continuous passive buffs.
 */
export function recalculatePassiveBuffs(
  state: GameState,
  playerId: PlayerId
): GameState {
  return produce(state, (draft) => {
    const player = draft.players[playerId];

    // Decision §8.63 — le bonus de PV d'un navire est un bonus de MAXIMUM, et
    // les PV courants doivent suivre. Comme ce recalcul reconstruit les
    // modificateurs a chaque appel, on releve le bonus DEJA applique avant de
    // tout effacer, pour n'appliquer ensuite que le delta : sans cela les PV
    // gonfleraient a chaque recalcul.
    const shipPvBefore: Record<string, number> = {};
    for (const slot of ALL_SLOTS) {
      const charId = player.board[slot as Slot];
      if (!charId) continue;
      const card = draft.cards[charId];
      if (!card) continue;
      shipPvBefore[charId] = card.modifiers
        .filter((m) => m.stat === "pv" && m.source.startsWith("passive_ship_"))
        .reduce((acc, m) => acc + m.amount, 0);
    }

    // Remove all passive-source modifiers from all board characters
    for (const slot of ALL_SLOTS) {
      const charId = player.board[slot as Slot];
      if (!charId) continue;
      const card = draft.cards[charId];
      if (card) {
        card.modifiers = card.modifiers.filter(
          (m) => !m.source.startsWith("passive_") && !m.source.startsWith("captain_") && !m.source.startsWith("synergy_")
        );
      }
    }

    // Collect all board characters
    const boardChars: { id: string; defId: string; slot: Slot }[] = [];
    for (const slot of ALL_SLOTS) {
      const charId = player.board[slot as Slot];
      if (charId && draft.cards[charId]) {
        boardChars.push({ id: charId, defId: draft.cards[charId].defId, slot: slot as Slot });
      }
    }

    // === Captain passive buffs ===
    const captain = player.captain;
    const capDef = getCaptainDef(captain.defId);
    const capPassive = captain.flipped ? capDef.verso.passive : capDef.recto.passive;

    for (const effect of capPassive.effects) {
      if (effect.type === "buffAlly") {
        for (const char of boardChars) {
          if (matchesFilter(draft, char.defId, effect.filter)) {
            draft.cards[char.id].modifiers.push({
              id: `captain_${effect.stat}_${char.id}`,
              stat: effect.stat,
              amount: effect.amount,
              source: `captain_${capDef.id}`,
              duration: "permanent",
            });
          }
        }
      }
    }

    // === Character passive buffs (buffAlly on other characters) ===
    for (const char of boardChars) {
      const def = getCardDef(char.defId);
      if (!def.passive) continue;
      for (const effect of def.passive.effects) {
        if (effect.type === "buffAlly") {
          for (const target of boardChars) {
            if (target.id === char.id) continue; // Don't buff self
            if (matchesFilter(draft, target.defId, effect.filter)) {
              draft.cards[target.id].modifiers.push({
                id: `passive_${char.id}_${effect.stat}_${target.id}`,
                stat: effect.stat,
                amount: effect.amount,
                source: `passive_${char.id}`,
                duration: "permanent",
              });
            }
          }
        }
      }
    }

    // === Ship passive buffs ===
    if (player.activeShip) {
      const shipCard = draft.cards[player.activeShip];
      if (shipCard) {
        const shipDef = getCardDef(shipCard.defId);
        if (shipDef.shipPassive) {
          const desc = shipDef.shipPassive.toLowerCase();
          // Parse common ship passive patterns
          const atkMatch = desc.match(/\+(\d+)\s*atk/i);
          const defMatch = desc.match(/\+(\d+)\s*def/i);
          const pvMatch = desc.match(/\+(\d+)\s*pv/i);
          const shipAtkBonus = atkMatch ? parseInt(atkMatch[1]) : 0;
          const shipDefBonus = defMatch ? parseInt(defMatch[1]) : 0;
          // Decision §8.63 — « Vos X gagnent +N PV » est un bonus CONTINU, qui
          // vit tant que le navire est en jeu. Le libelle « +N PV **au
          // deploiement** » (Going Merry) est une autre regle, ponctuelle,
          // deja traitee dans `deployCharacter` ; on ne la double pas ici.
          const atDeploy = desc.includes("deploiement") || desc.includes("déploiement");
          const shipPvBonus = pvMatch && !atDeploy ? parseInt(pvMatch[1]) : 0;

          // Decision §8.10 — le selecteur partage avec `deployCost` : sans lui,
          // « Vos Baroque Works … » buffait TOUS les personnages.
          const scope = shipPassiveScope(desc);

          for (const char of boardChars) {
            const charDef = getCardDef(char.defId);
            const factionMatch = shipScopeMatches(scope, charDef);

            if (factionMatch) {
              if (shipAtkBonus > 0) {
                draft.cards[char.id].modifiers.push({
                  id: `ship_passive_atk_${char.id}`,
                  stat: "atk",
                  amount: shipAtkBonus,
                  source: `passive_ship_${shipDef.id}`,
                  duration: "permanent",
                });
              }
              if (shipDefBonus > 0) {
                draft.cards[char.id].modifiers.push({
                  id: `ship_passive_def_${char.id}`,
                  stat: "def",
                  amount: shipDefBonus,
                  source: `passive_ship_${shipDef.id}`,
                  duration: "permanent",
                });
              }
              if (shipPvBonus > 0) {
                draft.cards[char.id].modifiers.push({
                  id: `ship_passive_pv_${char.id}`,
                  stat: "pv",
                  amount: shipPvBonus,
                  source: `passive_ship_${shipDef.id}`,
                  duration: "permanent",
                });
              }
            }
          }
        }
      }
    }

    // === Synergy bonuses ===
    // Decision §8.7 : un partenaire de synergie est une unite *sur le plateau*.
    // Le capitaine recto est explicitement hors plateau (§4.11), donc le verso
    // est la seule face qui puisse etre un partenaire — c'est ce qui fait vivre
    // la synergie `RH-004` Rockstar / `CAP-SHANKS`, muette jusqu'ici.
    const capOnBoard = captain.flipped && !!captain.slot;
    for (const char of boardChars) {
      const def = getCardDef(char.defId);
      if (!def.synergies) continue;
      for (const syn of def.synergies) {
        // Check if partner is on the board
        const partnerOnBoard =
          boardChars.some((c) => c.defId === syn.partnerId) ||
          (capOnBoard && captain.defId === syn.partnerId);
        if (partnerOnBoard) {
          draft.cards[char.id].modifiers.push({
            id: `synergy_${char.id}_${syn.partnerId}`,
            stat: "atk",
            amount: syn.atkBonus,
            source: `synergy_${syn.partnerId}`,
            duration: "permanent",
          });
        }
      }
    }

    // Decision §8.63 — les PV courants suivent le delta du bonus de navire :
    // le navire arrive, l'unite gagne reellement le point (sinon le « +1 PV »
    // ne change rien tant qu'elle est intacte) ; le navire coule, elle le
    // rend. Ramene au nouveau maximum, et **jamais mortel** : perdre un
    // navire ne doit pas mettre KO un equipage a 1 PV.
    for (const char of boardChars) {
      const card = draft.cards[char.id];
      if (!card) continue;
      const after = card.modifiers
        .filter((m) => m.stat === "pv" && m.source.startsWith("passive_ship_"))
        .reduce((acc, m) => acc + m.amount, 0);
      const delta = after - (shipPvBefore[char.id] ?? 0);
      if (delta === 0) continue;
      const printed = getCardDef(card.defId).pv;
      const maxPv =
        printed === undefined ? card.currentPv + delta : printed + after - (card.pvMaxLoss ?? 0);
      card.currentPv = Math.max(1, Math.min(card.currentPv + delta, maxPv));
    }
  });
}

/**
 * Recompute enemy-debuff auras (Shanks "adjacent enemies -2 ATK", Goldenweek/Howling Gab
 * "one enemy -N"). Approximations: adjacent → enemy front row; one → strongest enemy.
 */
export function applyEnemyDebuffAuras(state: GameState): GameState {
  return produce(state, (draft) => {
    const players: PlayerId[] = ["player1", "player2"];
    for (const pid of players) {
      for (const slot of ALL_SLOTS) {
        const id = draft.players[pid].board[slot as Slot];
        if (id) draft.cards[id].modifiers = draft.cards[id].modifiers.filter((m) => m.source !== "debuffAura");
      }
    }
    for (const pid of players) {
      const opp: PlayerId = pid === "player1" ? "player2" : "player1";
      const sources: PassiveEffect[][] = [];
      for (const slot of ALL_SLOTS) {
        const id = draft.players[pid].board[slot as Slot];
        if (id) sources.push(getCardDef(draft.cards[id].defId).passive?.effects ?? []);
      }
      const cap = draft.players[pid].captain;
      const cd = getCaptainDef(cap.defId);
      sources.push((cap.flipped ? cd.verso.passive : cd.recto.passive).effects);

      let adj = 0, one = 0;
      for (const effs of sources) for (const e of effs) {
        if (e.type === "debuffAdjacentEnemies") adj += e.amount;
        if (e.type === "debuffOneEnemy") one = Math.max(one, e.amount);
      }
      // Decision §8.65 — « Les ennemis adjacents au porteur ont -1 ATK »
      // (RH-016 Cape de l'Empereur). Meme accumulateur que le passif de
      // personnage equivalent, donc meme approximation d'« adjacent » que le
      // moteur applique deja partout : la ligne avant adverse.
      for (const slot of ALL_SLOTS) {
        const id = draft.players[pid].board[slot as Slot];
        if (!id) continue;
        for (const objId of draft.cards[id].attachedObjects) {
          const o = draft.cards[objId];
          if (!o) continue;
          adj += getCardDef(o.defId).objectEffects?.adjacentEnemyAtk ?? 0;
        }
      }
      for (const objId of draft.players[pid].captain.attachedObjects ?? []) {
        const o = draft.cards[objId];
        if (!o) continue;
        adj += getCardDef(o.defId).objectEffects?.adjacentEnemyAtk ?? 0;
      }
      if (adj > 0) {
        for (const s of ["V1", "V2", "V3"] as Slot[]) {
          const id = draft.players[opp].board[s];
          if (id) draft.cards[id].modifiers.push({ id: `debuffAura_adj_${id}`, stat: "atk", amount: -adj, source: "debuffAura", duration: "permanent" });
        }
      }
      if (one > 0) {
        let best: string | null = null, bestAtk = -1;
        for (const slot of ALL_SLOTS) {
          const id = draft.players[opp].board[slot as Slot];
          if (!id) continue;
          const a = getCardDef(draft.cards[id].defId).atk ?? 0;
          if (a > bestAtk) { bestAtk = a; best = id; }
        }
        if (best) draft.cards[best].modifiers.push({ id: `debuffAura_one_${best}`, stat: "atk", amount: -one, source: "debuffAura", duration: "permanent" });
      }
    }
  });
}

function matchesFilter(
  draft: GameState,
  defId: string,
  filter?: AllyFilter
): boolean {
  if (!filter) return true;
  const def = getCardDef(defId);
  if (filter.faction && def.faction !== filter.faction) return false;
  if (filter.tag && !(def.tags?.includes(filter.tag))) return false;
  if (filter.trait && !(def.traits?.includes(filter.trait))) return false;
  return true;
}

// ============================================================
// On-KO effects
// ============================================================

/**
 * Handle effects that trigger when a character is KO'd.
 */
export function applyOnKOEffects(
  state: GameState,
  koPlayerId: PlayerId,
  killerPlayerId: PlayerId,
  koDefId: string
): GameState {
  let next = state;
  const koPlayer = state.players[koPlayerId];

  // Track game/turn KO flags (free captain flip, Flashback condition).
  next = produce(next, (draft) => {
    draft.players[koPlayerId].charKOedThisGame = true;
  });

  // Captain onAllyKO passive
  const capDef = getCaptainDef(koPlayer.captain.defId);
  const capPassive = koPlayer.captain.flipped ? capDef.verso.passive : capDef.recto.passive;

  for (const effect of capPassive.effects) {
    if (effect.type === "onAllyKO" && effect.effect === "bonusWill") {
      next = produce(next, (draft) => {
        draft.players[koPlayerId].volonte += effect.amount;
        draft.log.push({
          turn: draft.turnNumber,
          player: koPlayerId,
          message: `Passif Capitaine : +${effect.amount} Vol. (allie KO)`,
        });
      });
    }
    // Luffy verso: +1 ATK permanent per Mugiwara ally KO (max +3).
    if (effect.type === "selfBuffOnAllyKO" && matchesFilter(next, koDefId, effect.filter)) {
      const cap = next.players[koPlayerId].captain;
      const current = cap.modifiers.filter((m) => m.source === "captainSelfKO").reduce((s, m) => s + m.amount, 0);
      if (current < effect.max) {
        next = produce(next, (draft) => {
          draft.players[koPlayerId].captain.modifiers.push({
            id: `captainSelfKO_${Date.now()}`, stat: effect.stat, amount: effect.amount,
            source: "captainSelfKO", duration: "permanent",
          });
          draft.log.push({ turn: draft.turnNumber, player: koPlayerId, message: `${capDef.name} : +${effect.amount} ATK permanent (Mugiwara KO).` });
        });
      }
    }
  }

  // Banish-on-KO (Akainu Justice Implacable): the killer's flipped captain banishes the victim.
  const killer = next.players[killerPlayerId].captain;
  const killerDef = getCaptainDef(killer.defId);
  const killerPassive = killer.flipped ? killerDef.verso.passive : killerDef.recto.passive;
  if (killer.flipped && killerPassive.effects.some((e) => e.type === "banishOnKO")) {
    next = produce(next, (draft) => {
      const gy = draft.players[koPlayerId].graveyard;
      for (let i = gy.length - 1; i >= 0; i--) {
        if (draft.cards[gy[i]].defId === koDefId) {
          draft.cards[gy[i]].zone = "banished";
          gy.splice(i, 1);
          break;
        }
      }
      draft.log.push({ turn: draft.turnNumber, player: killerPlayerId, message: `${getCardDef(koDefId).name} est banni (Justice Implacable) !` });
    });
  }

  // Explode-on-KO (Mr. 5): the KO'd unit deals damage to an enemy.
  const koDefStatic = getCardDef(koDefId);
  const explode = koDefStatic.passive?.effects.reduce((s, e) => s + (e.type === "explodeOnKO" ? e.amount : 0), 0) ?? 0;
  if (explode > 0) {
    const enemies = getBoardCharacters(next, killerPlayerId);
    if (enemies.length > 0) {
      const target = enemies.reduce((a, b) => (b.currentPv < a.currentPv ? b : a));
      const tid = target.instanceId;
      next = produce(next, (d) => { d.cards[tid].currentPv -= explode; });
      next = addLog(next, koPlayerId, `Corps Explosif : ${explode} dégâts à ${getCardDef(target.defId).name} !`);
      if (next.cards[tid] && next.cards[tid].currentPv <= 0) {
        const { removeFromBoard } = require("./board");
        const exDef = next.cards[tid].defId; const exOwner = next.cards[tid].owner;
        next = addLog(next, exOwner, `${getCardDef(exDef).name} est KO !`);
        next = removeFromBoard(next, tid);
      }
    }
  }

  // Synergy rage bonus (partner KO'd → surviving partner gets temp ATK)
  const boardChars = getBoardCharacters(next, koPlayerId);
  for (const char of boardChars) {
    const def = getCardDef(char.defId);
    if (!def.synergies) continue;
    for (const syn of def.synergies) {
      if (syn.partnerId === koDefId && syn.onPartnerKO) {
        next = produce(next, (draft) => {
          draft.cards[char.instanceId].modifiers.push({
            id: `synrage_${char.instanceId}_${Date.now()}`,
            stat: "atk",
            amount: syn.onPartnerKO!,
            source: `synergy_rage_${koDefId}`,
            duration: "turn",
          });
          draft.log.push({
            turn: draft.turnNumber,
            player: koPlayerId,
            message: `${def.name} : rage ! +${syn.onPartnerKO} ATK (${getCardDef(koDefId).name} KO)`,
          });
        });
      }
    }
  }

  // Recalculate passive buffs (someone left the board)
  next = recalculatePassiveBuffs(next, koPlayerId);

  return next;
}
