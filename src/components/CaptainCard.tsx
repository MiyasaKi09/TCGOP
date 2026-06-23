"use client";

import type { CaptainInstance, CaptainDef } from "@/types";
import { faction, hpColor, TRAIT_LABEL, CARD_ART, CARD_ART_VERSO, CARD_ART_FOCUS } from "@/data/cardArt";
import { Sword, Shield, Crest } from "./icons";
import StatusBadges from "./StatusBadges";

interface CaptainCardProps {
  captain: CaptainInstance;
  def: CaptainDef;
  isOpponent?: boolean;
  onClick?: () => void;
}

export default function CaptainCard({ captain, def, isOpponent, onClick }: CaptainCardProps) {
  const fac = faction(def.faction);
  const side = captain.flipped ? def.verso : def.recto;
  const maxPv = side.pv;
  const ratio = Math.max(0, captain.currentPv / maxPv);
  const pct = ratio * 100;
  const hpc = hpColor(ratio);
  const edge = captain.flipped ? "#E0463F" : "#E8B84B";
  const art = captain.flipped ? (CARD_ART_VERSO[def.id] ?? CARD_ART[def.id]) : CARD_ART[def.id];

  return (
    <div
      key={captain.flipped ? "verso" : "recto"}
      onClick={onClick}
      className={`cap-flip-face relative rounded-xl overflow-hidden w-40 p-2.5 transition-all duration-300 cursor-pointer ${captain.tapped ? "saturate-50 opacity-70" : isOpponent ? "" : "hover:brightness-110"}`}
      style={{ background: fac.frame, boxShadow: `inset 0 0 0 2px ${edge}, 0 8px 22px rgba(0,0,0,.5)` }}
    >
      {art ? (
        <>
          <div className="absolute inset-0 pointer-events-none" style={{ backgroundImage: `url('${art}')`, backgroundSize: "cover", backgroundPosition: CARD_ART_FOCUS[def.id] ?? "50% 24%" }} />
          <div className="absolute inset-0 pointer-events-none" style={{ background: "linear-gradient(180deg, rgba(8,10,14,.42) 0%, rgba(8,10,14,.78) 56%, rgba(8,10,14,.93) 100%)" }} />
        </>
      ) : (
        <div className="absolute inset-0 flex items-center justify-center opacity-[0.12] pointer-events-none">
          <Crest which={fac.crest} size={120} color="#fff" />
        </div>
      )}

      {/* header */}
      <div className="relative flex justify-between items-start gap-1">
        <div className="min-w-0">
          <div className="font-cinzel font-bold text-[13px] text-white leading-tight truncate">{def.name}</div>
          <div className="font-oswald text-[8px] uppercase tracking-[.16em] mt-0.5" style={{ color: fac.accent }}>{fac.label}</div>
        </div>
        <span className="font-oswald text-[8px] px-1.5 py-0.5 rounded-full uppercase tracking-wider shrink-0"
          style={{ background: captain.flipped ? "rgba(224,70,63,.85)" : "rgba(232,184,75,.85)", color: "#0a0d12" }}>
          {captain.flipped ? "Verso" : "★ Recto"}
        </span>
      </div>

      {/* PV bar */}
      <div className="relative mt-2">
        <div className="w-full h-2 rounded-full overflow-hidden" style={{ background: "rgba(8,12,18,.7)" }}>
          <div className="h-full rounded-full transition-all duration-500" style={{ width: `${pct}%`, background: hpc }} />
        </div>
        <div className="font-oswald text-[10px] text-center mt-0.5 font-bold" style={{ color: hpc }}>{captain.currentPv} / {maxPv} PV</div>
      </div>

      {/* stats */}
      <div className="relative flex gap-2 mt-2">
        <div className="flex-1 flex items-center justify-center gap-1 py-1 rounded-lg" style={{ background: "rgba(255,80,70,.12)" }}>
          <Sword size={12} /><span className="font-oswald font-bold text-[13px]" style={{ color: "#FF7062" }}>{side.atk}</span>
        </div>
        <div className="flex-1 flex items-center justify-center gap-1 py-1 rounded-lg" style={{ background: "rgba(120,170,230,.12)" }}>
          <Shield size={12} /><span className="font-oswald font-bold text-[13px]" style={{ color: "#7FB0E8" }}>{side.def}</span>
        </div>
      </div>

      {/* passive */}
      <div className="relative font-spectral italic text-[9.5px] text-white/75 mt-1.5 truncate">✦ {side.passive.name}</div>

      {/* traits (verso) */}
      {captain.flipped && def.verso.traits && def.verso.traits.length > 0 && (
        <div className="relative flex flex-wrap gap-1 mt-1.5">
          {def.verso.traits.map((t) => (
            <span key={t} className="font-oswald text-[8px] px-1.5 py-0.5 rounded-full text-white/85" style={{ background: "rgba(255,255,255,.12)" }}>{TRAIT_LABEL[t] ?? t}</span>
          ))}
        </div>
      )}

      {/* status */}
      {captain.statusEffects.length > 0 && (
        <div className="mt-1.5">
          <StatusBadges effects={captain.statusEffects} compact />
        </div>
      )}

      {captain.tapped && (
        <div className="absolute inset-0 flex items-center justify-center" style={{ background: "rgba(5,7,11,.5)" }}>
          <span className="font-oswald text-[9px] tracking-widest uppercase text-white px-2 py-1 rounded-full" style={{ background: "rgba(0,0,0,.6)" }}>Incliné</span>
        </div>
      )}
    </div>
  );
}
