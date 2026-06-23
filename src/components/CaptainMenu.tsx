"use client";

import type { CaptainInstance, CaptainDef, GameState, GameAction, SpecialAttack, BaseAction } from "@/types";
import { faction, hpColor, TRAIT_LABEL } from "@/data/cardArt";
import { useFlipZoom } from "@/lib/useFlipZoom";
import StatusBadges from "./StatusBadges";

interface CaptainMenuProps {
  captain: CaptainInstance;
  def: CaptainDef;
  state: GameState;
  validActions: GameAction[];
  isYou: boolean;
  onFlip: () => void;
  onAttack: () => void;
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
  captain, def, state, validActions, isYou, onFlip, onAttack, onKingHaki, onClose, originRect,
}: CaptainMenuProps) {
  const zoomRef = useFlipZoom<HTMLDivElement>(originRect);
  const fac = faction(def.faction);
  const side = captain.flipped ? def.verso : def.recto;
  const ratio = Math.max(0, captain.currentPv / side.pv);
  const hpc = hpColor(ratio);
  const edge = captain.flipped ? "var(--color-target)" : "var(--color-gold)";

  const canFlip = isYou && !captain.flipped && validActions.some((a) => a.type === "flipCaptain");
  const canAttack = isYou && captain.flipped && validActions.some((a) => a.type === "captainAttack");
  const canKingHaki = isYou && validActions.some((a) => a.type === "useHaki" && a.hakiType === "king");

  const isFrozen = captain.statusEffects.some((e) => e.type === "freeze");
  const isImmob = captain.statusEffects.some((e) => e.type === "immobilize");
  const attackReason = isFrozen ? "Gelé !" : isImmob ? "Immobilisé !" : captain.tapped ? "Incliné" : captain.deployedTurn === state.turnNumber ? "Vient d'être engagé" : null;

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
            <div className="w-full h-2 rounded-full overflow-hidden" style={{ background: "rgba(8,12,18,.7)" }}>
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
                ⚔ Attaquer{!canAttack && attackReason ? ` — ${attackReason}` : ""}
              </button>
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
