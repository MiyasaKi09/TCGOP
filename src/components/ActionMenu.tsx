"use client";

import type { CardDef, CardInstance, GameState, GameAction } from "@/types";
import { getEffectiveAtk } from "@/engine/board";
import { getCardDef } from "@/engine/cardRegistry";
import { CARD_ART, CARD_ART_VERSO } from "@/data/cardArt";
import FullCard, { type CardActions } from "./FullCard";
import { useFlipZoom } from "@/lib/useFlipZoom";

interface ActionMenuProps {
  instance: CardInstance;
  def: CardDef;
  state: GameState;
  validActions: GameAction[];
  onBaseAttack: () => void;
  onSpecialAttack: () => void;
  onSupportAction: () => void;
  /** Éveille un Fruit du Démon porté par cette unité. */
  onAwakenFruit: (fruitInstanceId: string) => void;
  /** Vise avec l'attaque spéciale d'un fruit éveillé porté par cette unité. */
  onFruitSpecial: (fruitInstanceId: string) => void;
  onViewDetail: () => void;
  onClose: () => void;
  originRect?: DOMRect | null;
}

export default function ActionMenu({
  instance, def, state, validActions,
  onBaseAttack, onSpecialAttack, onSupportAction, onAwakenFruit, onFruitSpecial,
  onViewDetail, onClose, originRect,
}: ActionMenuProps) {
  const zoomRef = useFlipZoom<HTMLDivElement>(originRect);
  const effectiveAtk = getEffectiveAtk(state, instance.instanceId);

  const canBaseAttack = validActions.some((a) => a.type === "baseAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId);
  const canSpecialAttack = validActions.some((a) => a.type === "specialAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId);
  const canSupport = validActions.some((a) => a.type === "baseSupportAction" && a.instanceId === instance.instanceId);

  const isTapped = instance.tapped;
  const usedBase = instance.usedBaseAction;
  const usedSpecial = instance.usedSpecialAttack;
  const hasSickness = instance.deployedTurn === state.turnNumber && !(def.traits?.includes("rush"));
  const isFrozen = instance.statusEffects.some((e) => e.type === "freeze");
  const isImmobilized = instance.statusEffects.some((e) => e.type === "immobilize");
  const playerVol = state.players[instance.owner].volonte;
  const base = def.baseAction;
  const isSupport = base?.isSupport;

  // Ce qui bloque TOUTE action de base, indépendamment de l'effet ou de l'attaque.
  const blockedReason = (() => {
    if (isFrozen) return "Gelé !";
    if (isImmobilized) return "Immobilisé !";
    if (hasSickness) return "Mal de terre";
    if (isTapped) return "Incliné";
    if (usedBase) return "Action déjà utilisée ce tour";
    return null;
  })();

  // L'effet de soutien et l'attaque sont gardés séparément par le moteur :
  // chacun affiche donc sa propre raison au lieu d'un motif fusionné trompeur.
  const supportReason = blockedReason ?? (canSupport ? null : "Aucune cible valide");

  const attackReason = (() => {
    if (blockedReason) return blockedReason;
    if (effectiveAtk <= 0) return "ATK 0";
    if (!canBaseAttack) return "Pas de cible";
    return null;
  })();

  const specReason = (() => {
    if (isFrozen) return "Gelé !";
    if (isImmobilized) return "Immobilisé !";
    if (hasSickness) return "Mal de terre";
    if (usedSpecial) return "Déjà utilisé ce tour";
    if (def.specialAttack?.oncePerGame && instance.usedOnceAbilities.includes(def.specialAttack.name)) return "Déjà utilisé (1x/partie)";
    if (def.specialAttack && playerVol < def.specialAttack.cost) return `Volonté insuffisante (${playerVol}/${def.specialAttack.cost})`;
    if (!canSpecialAttack) return "Pas de cible";
    return null;
  })();

  const actions: CardActions = {
    support: isSupport
      ? { onClick: onSupportAction, disabled: !canSupport, reason: supportReason }
      : undefined,
    // Un personnage de soutien sans ATK n'a pas de ligne d'attaque du tout.
    base: base && (!isSupport || effectiveAtk > 0)
      ? { onClick: onBaseAttack, disabled: !canBaseAttack, reason: attackReason }
      : undefined,
    special: def.specialAttack ? { onClick: onSpecialAttack, disabled: !canSpecialAttack, reason: specReason } : undefined,
  };

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-40 p-4" onClick={onClose} onContextMenu={(e) => e.preventDefault()}>
      <div className="panel panel-gold halftone p-4 flex flex-col gap-3 animate-fade-in" style={{ userSelect: "none" }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <span className="font-oswald text-[10px] uppercase tracking-widest text-white/45">Cliquez une action sur la carte</span>
          <span className="font-oswald font-bold text-sm text-gold">{playerVol} Vol.</span>
        </div>

        <div ref={zoomRef} style={{ willChange: "transform" }}><FullCard def={def} instance={instance} state={state} width={360} actions={actions} /></div>

        {(isTapped || hasSickness || isFrozen || isImmobilized) && (
          <div className="flex gap-1.5 flex-wrap">
            {isTapped && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-white/10 text-white/70">Incliné</span>}
            {hasSickness && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-yellow-800/40 text-yellow-300">Mal de terre</span>}
            {isFrozen && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-cyan-800/40 text-cyan-300">Gelé</span>}
            {isImmobilized && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-pink-800/40 text-pink-300">Immobilisé</span>}
          </div>
        )}

        {instance.attachedObjects.length > 0 && (
          <div className="rounded-xl p-2 flex flex-col gap-2" style={{ background: "rgba(232,184,75,.08)" }}>
            <div className="font-oswald text-[9px] uppercase tracking-wider text-amber-400/70 font-bold">Équipement</div>
            {instance.attachedObjects.map((objId) => {
              const obj = state.cards[objId];
              if (!obj) return null;
              const objDef = getCardDef(obj.defId);
              const awakened = !!obj.isAwakened;
              const art = awakened ? (CARD_ART_VERSO[objDef.id] ?? CARD_ART[objDef.id]) : CARD_ART[objDef.id];

              // Le moteur propose l'éveil et la spéciale du fruit : jusqu'ici
              // l'interface ne les exposait que pour le capitaine, donc un fruit
              // posé sur son porteur légitime restait bloqué.
              const canAwaken = validActions.some((a) => a.type === "awakenFruit" && a.fruitInstanceId === objId);
              const fruitSpecial = validActions.find(
                (a) => a.type === "fruitSpecialAttack" && "fruitInstanceId" in a && a.fruitInstanceId === objId
              );
              const awakening = objDef.fruitEffects?.awakening;
              const spec = awakening?.specialAttack;

              return (
                <div key={objId} className="flex gap-2">
                  {art && (
                    <div
                      className="flex-none rounded-lg overflow-hidden"
                      style={{ width: 44, height: 60, backgroundImage: `url('${art}')`, backgroundSize: "cover", backgroundPosition: "center 18%", border: "1.5px solid var(--ink-edge)" }}
                    />
                  )}
                  <div className="min-w-0 flex-1 flex flex-col gap-1">
                    <div className="font-spectral text-[11px] font-bold text-amber-200/95">
                      {awakened ? "⭐ " : "⚔ "}{objDef.name}
                      {objDef.bonusAtk ? <span className="text-atk"> +{objDef.bonusAtk} ATK</span> : null}
                      {objDef.bonusDef ? <span className="text-def"> +{objDef.bonusDef} DEF</span> : null}
                    </div>
                    {objDef.equipEffect && (
                      <div className="font-spectral italic text-[10px] leading-snug text-white/70">{objDef.equipEffect}</div>
                    )}
                    {awakening?.passiveDescription && (
                      <div className="font-spectral italic text-[10px] leading-snug" style={{ color: awakened ? "rgba(255,224,138,.9)" : "rgba(255,255,255,.45)" }}>
                        ⭐ Éveil : {awakening.passiveDescription}
                      </div>
                    )}
                    <div className="flex gap-1.5 flex-wrap">
                      {!awakened && awakening && (
                        <div className="flex flex-col gap-0.5">
                          <button
                            onClick={() => onAwakenFruit(objId)}
                            disabled={!canAwaken}
                            className="btn btn-gold action-btn px-2 py-1 text-[10px] self-start"
                          >
                            ⭐ Éveiller{awakening.volCost != null ? ` — ${awakening.volCost} Vol.` : ""}
                          </button>
                          {/* Nommer le blocage RÉEL, pas réciter les conditions :
                              une liste générique laisse croire à une panne. */}
                          {!canAwaken && (
                            <span className="font-oswald text-[9px]" style={{ color: "#FF8A80" }}>
                              • {(() => {
                                if (awakening.porteurLegitime && !def.name.includes(awakening.porteurLegitime))
                                  return `Réservé à ${awakening.porteurLegitime}`;
                                if (state.turnNumber < awakening.minTurns)
                                  return `Dès la manche ${awakening.minTurns} (nous sommes en ${state.turnNumber})`;
                                if (playerVol < awakening.volCost)
                                  return `Volonté insuffisante (${playerVol}/${awakening.volCost})`;
                                return "Indisponible pour l'instant";
                              })()}
                            </span>
                          )}
                        </div>
                      )}
                      {awakened && spec && (
                        <button
                          onClick={() => onFruitSpecial(objId)}
                          disabled={!fruitSpecial}
                          title={fruitSpecial ? "Déclencher l'attaque du fruit éveillé" : "Aucune cible, ou action déjà dépensée"}
                          className="btn action-btn px-2 py-1 text-[10px]"
                          style={{ background: "linear-gradient(180deg,#a855f7,#7c3aed)", color: "#fff" }}
                        >
                          ★ {spec.name}{spec.cost ? ` — ${spec.cost} Vol.` : ""}
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        <div className="flex gap-2">
          <button onClick={onViewDetail} className="btn btn-ghost action-btn flex-1 py-2 text-xs">Détails</button>
          <button onClick={onClose} className="btn btn-ghost action-btn flex-1 py-2 text-xs">Fermer</button>
        </div>
      </div>
    </div>
  );
}
