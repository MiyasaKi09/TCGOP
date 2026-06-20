"use client";

import type { CardDef, CardInstance, GameState } from "@/types";
import { getEffectiveAtk, getEffectiveDef } from "@/engine/board";
import { CARD_ART, faction, TRAIT_COLOR, TRAIT_LABEL, hpColor } from "@/data/cardArt";
import { Heart, Shield, Crest } from "./icons";

interface FullCardProps {
  def: CardDef;
  instance?: CardInstance;
  state?: GameState;
  /** Rendered width in px (the card keeps a 300×419 design ratio). */
  width?: number;
}

const ELEMENT_COLOR: Record<string, string> = {
  fire: "#E0653C", water: "#3C9FE0", thunder: "#E0C13C", ice: "#5CC6E0", sand: "#C2925A", poison: "#8FB84A",
};
const ELEMENT_LABEL: Record<string, string> = {
  fire: "Feu", water: "Eau", thunder: "Foudre", ice: "Glace", sand: "Sable", poison: "Poison",
};

// A single card rendered in the unified "Claude Design" frame. The illustration
// (when present) is used only as the artwork; all formatting is consistent.
export default function FullCard({ def, instance, state, width = 300 }: FullCardProps) {
  const W = 300, H = 419;
  const scale = width / W;
  const art = CARD_ART[def.id];
  const fac = faction(def.faction);
  const isChar = def.type === "character";

  const atk = state && instance ? getEffectiveAtk(state, instance.instanceId) : def.atk ?? 0;
  const dfv = state && instance ? getEffectiveDef(state, instance.instanceId) : def.def ?? 0;
  const maxPv = def.pv ?? 0;
  const curPv = instance ? instance.currentPv : maxPv;
  const hpc = hpColor(maxPv ? curPv / maxPv : 1);
  const traits = def.traits ?? [];

  const base = def.baseAction;
  const spec = def.specialAttack;

  return (
    <div style={{ width, height: Math.round(H * scale), flex: "none" }}>
      <div style={{ position: "absolute", width: W, height: H, transform: `scale(${scale})`, transformOrigin: "top left", borderRadius: 16, overflow: "hidden", background: "#0b0e13", boxShadow: "0 10px 30px rgba(0,0,0,.5)" }}>
        {/* art layer */}
        {art ? (
          <div style={{ position: "absolute", inset: 0, backgroundImage: `url('${art}')`, backgroundSize: "cover", backgroundPosition: "center" }} />
        ) : (
          <>
            <div style={{ position: "absolute", inset: 0, background: fac.frame }} />
            <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", opacity: 0.12 }}>
              <Crest which={fac.crest} size={200} color="#fff" />
            </div>
          </>
        )}

        {/* scrims */}
        <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 96, background: "linear-gradient(180deg,rgba(6,9,14,.6),transparent)" }} />
        <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: isChar ? "52%" : "40%", background: "linear-gradient(180deg,transparent,rgba(7,9,13,.78) 24%,rgba(7,9,13,.96))" }} />

        {/* cost */}
        <div style={{ position: "absolute", top: 6, left: 13, fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 48, color: "#E8B84B", textShadow: "0 2px 4px rgba(0,0,0,.85)", lineHeight: 1 }}>{def.cost}</div>

        {/* PV (characters) */}
        {isChar && (
          <div style={{ position: "absolute", top: 10, right: 12, display: "flex", alignItems: "flex-start", gap: 3 }}>
            <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 40, color: hpc, textShadow: "0 2px 4px rgba(0,0,0,.85)", lineHeight: 1 }}>{curPv}</span>
            <Heart size={17} color={hpc} style={{ marginTop: 4 }} />
          </div>
        )}

        {/* vertical name */}
        <div style={{ position: "absolute", left: 5, top: "15%", height: "62%", writingMode: "vertical-rl", transform: "rotate(180deg)", display: "flex", alignItems: "center", gap: 9 }}>
          <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 21, letterSpacing: ".04em", color: "#fff", textShadow: "0 1px 4px rgba(0,0,0,.95)" }}>{def.name}</span>
          <span style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 12, color: "rgba(255,255,255,.75)", textShadow: "0 1px 3px rgba(0,0,0,.95)" }}>{fac.label} · {def.rarity}</span>
        </div>

        {/* DEF + traits (characters) */}
        {isChar && (
          <div style={{ position: "absolute", right: 11, top: "52%", display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 6 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <Shield size={20} color="rgba(255,255,255,.92)" />
              <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 24, color: "#fff", textShadow: "0 1px 3px rgba(0,0,0,.9)" }}>{dfv}</span>
            </div>
            <div style={{ display: "flex", gap: 4, flexWrap: "wrap", justifyContent: "flex-end", maxWidth: 130 }}>
              {traits.map((t) => (
                <span key={t} style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 11, color: "#fff", background: TRAIT_COLOR[t] ?? "rgba(255,255,255,.2)", borderRadius: 99, padding: "2px 9px", textShadow: "0 1px 2px rgba(0,0,0,.4)" }}>{TRAIT_LABEL[t] ?? t}</span>
              ))}
            </div>
          </div>
        )}

        {/* bottom block */}
        <div style={{ position: "absolute", left: 14, right: 11, bottom: 9, display: "flex", flexDirection: "column", gap: 5 }}>
          {def.passive && (
            <div>
              <div style={{ fontFamily: "var(--font-spectral)", fontWeight: 600, fontVariant: "small-caps", fontSize: 15, letterSpacing: ".03em", color: "#fff" }}>{def.passive.name}</div>
              <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 11.5, lineHeight: 1.25, color: "rgba(255,255,255,.82)" }}>{def.passive.description}</div>
            </div>
          )}

          {(base || spec) && <div style={{ height: 1, background: "linear-gradient(90deg,rgba(255,255,255,.45),transparent)" }} />}

          {base && (
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 8 }}>
              <div style={{ minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
                  <span style={{ color: "#E8B84B", fontSize: 12 }}>{base.isSupport ? "✦" : "⚔"}</span>
                  <span style={{ fontFamily: "var(--font-spectral)", fontWeight: 600, fontVariant: "small-caps", fontSize: 14, color: "#fff" }}>{base.name}</span>
                  {base.element && <span style={{ fontFamily: "var(--font-oswald)", fontSize: 9, color: "#fff", background: ELEMENT_COLOR[base.element] ?? "#888", borderRadius: 99, padding: "1px 7px" }}>{ELEMENT_LABEL[base.element] ?? base.element}</span>}
                </div>
                {base.description && <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 11, lineHeight: 1.2, color: "rgba(255,255,255,.8)" }}>{base.description}</div>}
              </div>
              <div style={{ textAlign: "right", flex: "none" }}>
                <div style={{ fontFamily: "var(--font-oswald)", fontSize: 11, color: "rgba(255,255,255,.9)" }}>Gratuit</div>
                {!base.isSupport && <div style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 11, color: "#E8B84B" }}>{atk} ATK</div>}
              </div>
            </div>
          )}

          {spec && (
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 8 }}>
              <div style={{ minWidth: 0 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
                  <span style={{ color: "#E8B84B", fontSize: 12 }}>★</span>
                  <span style={{ fontFamily: "var(--font-spectral)", fontWeight: 600, fontVariant: "small-caps", fontSize: 14, color: "#fff" }}>{spec.name}</span>
                  {spec.element && <span style={{ fontFamily: "var(--font-oswald)", fontSize: 9, color: "#fff", background: ELEMENT_COLOR[spec.element] ?? "#888", borderRadius: 99, padding: "1px 7px" }}>{ELEMENT_LABEL[spec.element] ?? spec.element}</span>}
                </div>
                {spec.description && <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 11, lineHeight: 1.2, color: "rgba(255,255,255,.8)" }}>{spec.description}</div>}
              </div>
              <div style={{ textAlign: "right", flex: "none" }}>
                <div style={{ fontFamily: "var(--font-oswald)", fontSize: 11, color: "rgba(255,255,255,.9)" }}>{spec.cost} Vol.</div>
                <div style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 11, color: "#E8B84B" }}>+{spec.atkBonus} ATK</div>
              </div>
            </div>
          )}

          {/* non-character effects */}
          {!isChar && (
            <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 12, lineHeight: 1.3, color: "rgba(255,255,255,.85)" }}>
              {def.type === "object" && <>{def.bonusAtk ? `+${def.bonusAtk} ATK ` : ""}{def.bonusDef ? `+${def.bonusDef} DEF ` : ""}{def.equipEffect ?? ""}</>}
              {def.type === "ship" && <>{def.shipPassive}{def.shipActive ? ` — ${def.shipActive.name} (${def.shipActive.cost}V)` : ""}</>}
              {def.type === "counter" && def.counterEffect && (def.counterEffect.type === "survive" ? def.counterEffect.description : `Réduit ${def.counterEffect.amount} dégâts.`)}
              {def.type === "event" && def.eventEffect && <>{describeEvent(def.eventEffect)}</>}
            </div>
          )}

          <div style={{ fontFamily: "var(--font-oswald)", fontSize: 8.5, letterSpacing: ".06em", color: "rgba(255,255,255,.5)", textAlign: "right" }}>{def.set} · {def.type}{def.subtype ? ` · ${def.subtype}` : ""}</div>
        </div>
      </div>
    </div>
  );
}

function describeEvent(e: NonNullable<CardDef["eventEffect"]>): string {
  switch (e.type) {
    case "gainWill": return `Gagne +${e.amount} Volonté.`;
    case "draw": return `Pioche ${e.amount}${e.discard ? `, défausse ${e.discard}` : ""}.`;
    case "healAlly": return e.allAllies ? `Tous les alliés +${e.amount} PV.` : `1 allié +${e.amount} PV.`;
    case "buffAllies": return `Alliés +${e.amount} ${e.stat.toUpperCase()}.`;
    case "damageEnemies": return `${e.amount} dégâts (${e.target}).`;
    case "dodgeAll": return "Esquive totale ce tour.";
    default: return (e as { description?: string }).description ?? "";
  }
}
