"use client";

import type { CaptainInstance, CaptainDef, GameState, GameAction, SpecialAttack, BaseAction } from "@/types";
import { getCardDef } from "@/engine/cardRegistry";
import { onceSurchargeKey } from "@/engine/captain";
import { faction, hpColor, TRAIT_LABEL } from "@/data/cardArt";
import { useFlipZoom } from "@/lib/useFlipZoom";
import StatusBadges from "./StatusBadges";

const ELEMENT_FR: Record<string, string> = {
  fire: "Feu", water: "Eau", thunder: "Foudre", ice: "Glace", sand: "Sable", poison: "Poison",
};
const ATK_TRAIT_FR: Record<string, string> = {
  impact: "Impact", zone: "Zone", piercing: "Perçant", total: "Totale",
};

/**
 * Effets notables d'une attaque, en clair. Sans ça le joueur lit « Kong Gun »
 * ou « Ground Death » sans savoir ce que ça fait ni ce que ça coûte.
 */
function effectTags(a: {
  element?: string; attackTraits?: string[]; oncePerGame?: boolean;
  ignoreShield?: boolean; ignoreDef?: number | boolean; twoTargets?: boolean;
  permanentPvLoss?: number; noHeal?: boolean; immobilize?: boolean;
  sleep?: boolean; pushback?: boolean; cannotBeDodged?: boolean;
}): string[] {
  const t: string[] = [];
  if (a.element) t.push(ELEMENT_FR[a.element] ?? a.element);
  for (const tr of a.attackTraits ?? []) t.push(ATK_TRAIT_FR[tr] ?? tr);
  if (a.twoTargets) t.push("2 cibles");
  if (a.ignoreShield) t.push("ignore Bouclier");
  if (a.ignoreDef) t.push("ignore DEF");
  if (a.permanentPvLoss) t.push(`−${a.permanentPvLoss} PV max définitif`);
  if (a.noHeal) t.push("bloque les soins");
  if (a.immobilize) t.push("immobilise");
  if (a.sleep) t.push("endort");
  if (a.pushback) t.push("repousse");
  if (a.cannotBeDodged) t.push("inesquivable");
  if (a.oncePerGame) t.push("1×/partie");
  return t;
}

interface CaptainMenuProps {
  captain: CaptainInstance;
  def: CaptainDef;
  state: GameState;
  validActions: GameAction[];
  isYou: boolean;
  onFlip: () => void;
  onAttack: () => void;
  /** Decision §8.34(a): aim the captain's signature special attack. */
  onSpecialAttack: () => void;
  /** Decision §8.34(b): aim the active face's `surcharge`. */
  onSurcharge: () => void;
  /** Decision §8.28 (follow-up): awaken a fruit the captain wears. */
  onAwakenFruit: (fruitInstanceId: string) => void;
  /** Decision §8.28 (follow-up): aim the awakened fruit's special attack. */
  onFruitSpecial: (fruitInstanceId: string) => void;
  /** Decision §8.67 — active la capacité d'un objet porté par le Capitaine. */
  onActivateObject: (objectInstanceId: string, needsTarget: boolean) => void;
  onKingHaki: () => void;
  onClose: () => void;
  originRect?: DOMRect | null;
}

const ELEMENT_LABEL: Record<string, string> = { fire: "Feu", water: "Eau", thunder: "Foudre", ice: "Glace", sand: "Sable", poison: "Poison" };

function AbilityRow({ a, accent, kind }: { a: SpecialAttack | BaseAction; accent: string; kind: string }) {
  const cost = "cost" in a && a.cost ? `${a.cost} Vol.` : "0 Vol.";
  return (
    <div className="rounded-lg px-2 py-1.5" style={{ background: "rgba(255,255,255,.05)" }}>
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0">
          <span style={{ color: accent }} className="text-[11px]">{kind}</span>
          <span className="font-spectral font-bold text-[12.5px] text-white truncate" style={{ fontVariant: "small-caps" }}>{a.name}</span>
          {a.element && <span className="font-oswald text-[8px] px-1.5 rounded-full text-white" style={{ background: "rgba(255,255,255,.2)" }}>{ELEMENT_LABEL[a.element] ?? a.element}</span>}
        </div>
        <span className="font-oswald text-[10px] text-white/70 shrink-0">{cost}</span>
      </div>
      {a.description && <div className="font-spectral italic text-[10px] text-white/70 mt-0.5 leading-snug">{a.description}</div>}
    </div>
  );
}

export default function CaptainMenu({
  captain, def, state, validActions, isYou, onFlip, onAttack, onSpecialAttack, onSurcharge,
  onAwakenFruit, onFruitSpecial, onActivateObject, onKingHaki, onClose, originRect,
}: CaptainMenuProps) {
  const zoomRef = useFlipZoom<HTMLDivElement>(originRect);
  const fac = faction(def.faction);
  const side = captain.flipped ? def.verso : def.recto;
  const ratio = Math.max(0, captain.currentPv / side.pv);
  const hpc = hpColor(ratio);
  const edge = captain.flipped ? "var(--color-target)" : "var(--color-gold)";

  const canFlip = isYou && !captain.flipped && validActions.some((a) => a.type === "flipCaptain");
  const canAttack =
    isYou && captain.flipped && validActions.some((a) => a.type === "captainAttack" && !a.isSpecial);
  // Decision §8.34 — la grosse attaque du capitaine (★ du verso) et la
  // surcharge sont deux actions a part entiere : elles doivent etre visibles,
  // chiffrees, et dire pourquoi elles sont grisees.
  const canSpecial =
    isYou && captain.flipped && validActions.some((a) => a.type === "captainAttack" && a.isSpecial);
  const canSurcharge = isYou && captain.flipped && validActions.some((a) => a.type === "useSurcharge");
  const canKingHaki = isYou && validActions.some((a) => a.type === "useHaki" && a.hakiType === "king");

  // Decision §8.28 (follow-up) — the equipment the captain wears (the three
  // signature SR Devil Fruits are printed "Équipable sur Luffy / Crocodile /
  // Akainu", names only a captain carries), with its two captain-side actions:
  // the awakening and, once awakened, the fruit's special attack.
  // ATK réellement en vigueur : la base imprimée PLUS les modificateurs
  // (le bonus d'éveil d'un fruit en est un). Afficher la base seule donnait un
  // chiffre faux dès qu'un fruit était éveillé.
  const capAtkNow =
    def.verso.atk + (captain.modifiers ?? []).reduce((n, m) => n + (m.stat === "atk" ? m.amount : 0), 0);

  const gear = (captain.attachedObjects ?? [])
    .map((id) => state.cards[id])
    .filter((c): c is NonNullable<typeof c> => !!c);
  const capId = `captain_${captain.owner}`;
  const awakenableIds = new Set(
    validActions.flatMap((a) => (a.type === "awakenFruit" ? [a.fruitInstanceId] : []))
  );
  const fruitSpecialIds = new Set(
    validActions.flatMap((a) =>
      a.type === "fruitSpecialAttack" && a.attackerInstanceId === capId ? [a.fruitInstanceId] : []
    )
  );

  const isFrozen = captain.statusEffects.some((e) => e.type === "freeze");
  const isImmob = captain.statusEffects.some((e) => e.type === "immobilize");
  const isAsleep = captain.statusEffects.some((e) => e.type === "sleep");
  const attackReason = isFrozen ? "Gelé !" : isImmob ? "Immobilisé !" : isAsleep ? "Endormi !" : captain.tapped ? "Incliné" : captain.deployedTurn === state.turnNumber ? "Vient d'être engagé" : null;

  // Decision §8.34 — le motif exact du refus, dans l'ordre ou le moteur le
  // verifie (`declareCaptainSpecAttack`) : verso requis, incline / gele /
  // immobilise / endormi / mal de terre, action deja depensee, 1x/partie,
  // Volonte. Un bouton grise sans motif laisse le joueur deviner.
  const volonte = state.players[captain.owner].volonte;
  function powerReason(spec: SpecialAttack, onceKey: string, ok: boolean): string | null {
    if (ok) return null;
    if (!captain.flipped) return "Capitaine non engagé (verso requis)";
    if (attackReason) return attackReason;
    if (captain.usedSpecialAttack) return "A déjà agi ce tour";
    if (spec.oncePerGame && captain.usedOnceAbilities.includes(onceKey)) return "Déjà utilisé (1x/partie)";
    if (volonte < spec.cost) return `Volonté insuffisante (${volonte}/${spec.cost})`;
    return "Pas de cible";
  }

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-40 p-4" onClick={onClose} onContextMenu={(e) => e.preventDefault()}>
      <div ref={zoomRef} className="panel halftone p-4 flex flex-col gap-2.5 w-[330px] max-h-[88vh] overflow-y-auto"
        style={{ boxShadow: `inset 0 0 0 1.5px ${edge}, var(--shadow-modal)`, userSelect: "none", willChange: "transform" }}
        onClick={(e) => e.stopPropagation()}>

        {/* header */}
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="font-cinzel font-bold text-[16px] text-white leading-tight truncate">{def.name}</div>
            <div className="font-oswald text-[9px] uppercase tracking-[.16em]" style={{ color: fac.accent }}>{fac.label} · Capitaine</div>
          </div>
          <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full uppercase tracking-wider shrink-0" style={{ background: captain.flipped ? "rgba(224,70,63,.85)" : "rgba(232,184,75,.85)", color: "#0a0d12" }}>
            {captain.flipped ? "Verso" : "★ Recto"}
          </span>
        </div>

        {/* PV + stats */}
        <div className="flex items-center gap-3">
          <div className="flex-1">
            <div className="hp-gauge w-full h-2.5 rounded-full">
              <div className="h-full rounded-full" style={{ width: `${ratio * 100}%`, background: hpc }} />
            </div>
            <div className="font-oswald text-[10px] mt-0.5 font-bold" style={{ color: hpc }}>{captain.currentPv} / {side.pv} PV</div>
          </div>
          <div className="font-oswald font-bold text-[13px] flex gap-2">
            <span className="text-atk">⚔{side.atk}</span>
            <span className="text-def">🛡{side.def}</span>
          </div>
        </div>

        {captain.statusEffects.length > 0 && <StatusBadges effects={captain.statusEffects} />}

        {/* passive (current side) */}
        <div className="rounded-lg px-2 py-1.5" style={{ background: "rgba(232,184,75,.08)" }}>
          <div className="font-spectral font-bold text-[12px] text-amber-200" style={{ fontVariant: "small-caps" }}>✦ {side.passive.name}</div>
          <div className="font-spectral italic text-[10px] text-white/75 leading-snug">{side.passive.description}</div>
        </div>

        {/* powers of the current side */}
        <div className="flex flex-col gap-1.5">
          {!captain.flipped && def.recto.attacks.map((atk, i) => <AbilityRow key={i} a={atk} accent="var(--color-atk)" kind="★" />)}
          {!captain.flipped && def.recto.surcharge && <AbilityRow a={def.recto.surcharge} accent="var(--color-gold)" kind="⚡" />}
          {captain.flipped && <AbilityRow a={def.verso.baseAction} accent="var(--color-deploy)" kind="⚔" />}
          {captain.flipped && <AbilityRow a={def.verso.specialAttack} accent="var(--color-atk)" kind="★" />}
          {captain.flipped && def.verso.surcharge && <AbilityRow a={def.verso.surcharge} accent="var(--color-gold)" kind="⚡" />}
          {captain.flipped && def.verso.naturalHaki && def.verso.naturalHaki.length > 0 && (
            <div className="font-oswald text-[9px] text-purple-300/80">👁 Haki naturel : {def.verso.naturalHaki.join(", ")}</div>
          )}
          {captain.flipped && def.verso.traits && def.verso.traits.length > 0 && (
            <div className="flex flex-wrap gap-1">
              {def.verso.traits.map((t) => <span key={t} className="font-oswald text-[8px] px-1.5 py-0.5 rounded-full text-white/85" style={{ background: "rgba(255,255,255,.12)" }}>{TRAIT_LABEL[t] ?? t}</span>)}
            </div>
          )}
        </div>

        {/* equipment worn by the captain (decision §8.28 follow-up) */}
        {gear.length > 0 && (
          <div className="rounded-lg px-2 py-1.5 flex flex-col gap-1" style={{ background: "rgba(232,184,75,.08)" }}>
            <div className="font-oswald text-[9px] uppercase tracking-wider text-amber-400/80 font-bold">Équipement</div>
            {gear.map((obj) => {
              const objDef = getCardDef(obj.defId);
              const fruitSpec = objDef.fruitEffects?.awakening?.specialAttack;
              return (
                <div key={obj.instanceId} className="flex flex-col gap-1">
                  <div className="font-spectral text-[11px] text-amber-200/90">
                    {obj.isAwakened ? "⭐" : "⚔"} {objDef.name}
                    {obj.isAwakened ? " (éveillé)" : ""}
                    {objDef.bonusAtk ? ` +${objDef.bonusAtk} ATK` : ""}
                    {objDef.bonusDef ? ` +${objDef.bonusDef} DEF` : ""}
                  </div>
                  {isYou && awakenableIds.has(obj.instanceId) && (
                    <button onClick={() => onAwakenFruit(obj.instanceId)} className="btn btn-gold action-btn px-3 py-1.5 text-[11px]">
                      ⭐ Éveiller {objDef.name} ({objDef.fruitEffects?.awakening?.volCost ?? 0} Vol.)
                    </button>
                  )}
                  {isYou && obj.isAwakened && fruitSpec && (() => {
                    const ok = fruitSpecialIds.has(obj.instanceId);
                    // Un bouton grisé sans motif laisse le joueur deviner : on
                    // dit pourquoi, comme la ligne d'attaque du capitaine.
                    const reason = ok
                      ? null
                      : !captain.flipped
                        ? "Capitaine non engagé"
                        : attackReason
                          ? attackReason
                          : captain.usedSpecialAttack
                            ? "A déjà agi ce tour"
                            : fruitSpec.oncePerGame && captain.usedOnceAbilities.includes(fruitSpec.name)
                              ? "Déjà utilisé (1x/partie)"
                              : state.players[captain.owner].volonte < fruitSpec.cost
                                ? `Volonté insuffisante (${state.players[captain.owner].volonte}/${fruitSpec.cost})`
                                : "Pas de cible";
                    return (
                      <button
                        onClick={() => onFruitSpecial(obj.instanceId)}
                        disabled={!ok}
                        className="btn btn-danger action-btn px-3 py-1.5 text-[11px] text-left"
                      >
                        ★ {fruitSpec.name} ({fruitSpec.cost} Vol.){reason ? ` — ${reason}` : ""}
                        <span className="block font-spectral italic normal-case text-[9.5px] leading-snug" style={{ color: "rgba(255,255,255,.85)" }}>
                          +{fruitSpec.atkBonus} ATK → {capAtkNow + fruitSpec.atkBonus} ATK
                          {effectTags(fruitSpec).length > 0 ? ` · ${effectTags(fruitSpec).join(" · ")}` : ""}
                          {fruitSpec.description ? ` · ${fruitSpec.description}` : ""}
                        </span>
                      </button>
                    );
                  })()}
                  {/* Decision §8.67 — la ligne activable d'un objet porte par
                      le Capitaine (Gryphon nomme Shanks, §8.62 suite). */}
                  {(() => {
                    const fx = objDef.objectEffects;
                    const act = fx?.activated ?? (fx?.grantsAttack
                      ? { name: fx.grantsAttack.name, cost: fx.grantsAttack.cost, target: "enemy" as const, oncePerGame: false }
                      : null);
                    if (!act) return null;
                    const vol = state.players[captain.owner].volonte;
                    const offered = validActions.filter(
                      (a) => a.type === "activateObject" && a.objectInstanceId === obj.instanceId
                    );
                    const used = !!act.oncePerGame && captain.usedOnceAbilities.includes(`obj_${objDef.id}`);
                    const reason = used
                      ? "Déjà utilisé (1x/partie)"
                      : vol < act.cost
                        ? `Volonté insuffisante (${vol}/${act.cost})`
                        : offered.length === 0
                          ? "Aucune cible"
                          : null;
                    return (
                      <button
                        onClick={() => onActivateObject(obj.instanceId, act.target === "enemy")}
                        disabled={reason !== null}
                        className="btn action-btn px-3 py-1.5 text-[11px] text-left"
                        style={{ background: "linear-gradient(180deg,#38bdf8,#0284c7)", color: "#fff" }}
                      >
                        ⚡ {act.name} ({act.cost} Vol.){act.oncePerGame ? " · 1x/partie" : ""}{reason ? ` — ${reason}` : ""}
                      </button>
                    );
                  })()}
                </div>
              );
            })}
          </div>
        )}

        {/* what flipping unlocks (recto only preview) */}
        {!captain.flipped && (
          <div className="rounded-lg px-2 py-1.5" style={{ background: "rgba(224,70,63,.1)", boxShadow: "inset 0 0 0 1px rgba(224,70,63,.3)" }}>
            <div className="font-oswald text-[9px] uppercase tracking-wider text-red-300/80 font-bold mb-0.5">En Verso (engagé)</div>
            <div className="font-spectral text-[10.5px] text-white/80">
              {def.verso.atk} ATK · {def.verso.def} DEF · {def.verso.pv} PV — ✦ {def.verso.passive.name}
            </div>
          </div>
        )}

        {/* actions (your side only) */}
        {isYou && (
          <div className="flex flex-col gap-1.5 pt-0.5">
            {canFlip && (
              <button onClick={onFlip} className="btn btn-danger action-btn px-3 py-2 text-xs">⚔ Engager le Capitaine</button>
            )}
            {captain.flipped && (
              <button onClick={onAttack} disabled={!canAttack} className="btn btn-danger action-btn px-3 py-2 text-xs">
                ⚔ {def.verso.baseAction.name}{!canAttack && attackReason ? ` — ${attackReason}` : ""}
              </button>
            )}
            {/* Decision §8.34(a) — la grosse attaque du capitaine. */}
            {captain.flipped && (() => {
              const spec = def.verso.specialAttack;
              const reason = powerReason(spec, spec.name, canSpecial);
              return (
                <button onClick={onSpecialAttack} disabled={!canSpecial} className="btn btn-gold action-btn px-3 py-2 text-xs">
                  ★ {spec.name} ({spec.cost} Vol.){reason ? ` — ${reason}` : ""}
                  <span className="block font-spectral italic normal-case text-[9.5px] text-black/70 leading-snug">
                    +{spec.atkBonus} ATK → {capAtkNow + spec.atkBonus} ATK
                    {effectTags(spec).length > 0 ? ` · ${effectTags(spec).join(" · ")}` : ""}
                    {spec.description ? ` · ${spec.description}` : ""}
                  </span>
                </button>
              );
            })()}
            {/* Decision §8.34(b) — la surcharge de la face active, si la carte en imprime une. */}
            {captain.flipped && def.verso.surcharge && (() => {
              const sur = def.verso.surcharge!;
              const reason = powerReason(sur, onceSurchargeKey(sur.name), canSurcharge);
              return (
                <button onClick={onSurcharge} disabled={!canSurcharge} className="btn btn-gold action-btn px-3 py-2 text-xs">
                  ⚡ {sur.name} ({sur.cost} Vol.){reason ? ` — ${reason}` : ""}
                  <span className="block font-spectral italic normal-case text-[9.5px] text-black/70 leading-snug">
                    +{sur.atkBonus} ATK → {capAtkNow + sur.atkBonus} ATK
                    {effectTags(sur).length > 0 ? ` · ${effectTags(sur).join(" · ")}` : ""}
                    {sur.description ? ` · ${sur.description}` : ""}
                  </span>
                </button>
              );
            })()}
            {!captain.flipped && (
              <div className="font-spectral italic text-[10px] text-white/60 leading-snug px-1">
                Le Capitaine recto ne peut pas attaquer (Rulebook v3.1 §2.1) : engagez-le pour
                débloquer {def.verso.baseAction.name} et ★ {def.verso.specialAttack.name}.
              </div>
            )}
            {canKingHaki && (
              <button onClick={onKingHaki} className="btn btn-gold action-btn px-3 py-2 text-xs">👑 Haki des Rois</button>
            )}
            <button onClick={onClose} className="btn btn-ghost action-btn px-3 py-1.5 text-xs">Fermer</button>
          </div>
        )}
        {!isYou && (
          <button onClick={onClose} className="btn btn-ghost action-btn px-3 py-1.5 text-xs">Fermer</button>
        )}
      </div>
    </div>
  );
}
