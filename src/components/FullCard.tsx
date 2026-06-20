"use client";

import type { CardDef, CardInstance, GameState } from "@/types";
import { getEffectiveAtk, getEffectiveDef } from "@/engine/board";
import { CARD_ART, faction, TRAIT_COLOR, TRAIT_LABEL, hpColor } from "@/data/cardArt";
import { Sword, Shield, Heart, Crest } from "./icons";

export interface CardActions {
  base?: { onClick: () => void; disabled: boolean; reason?: string | null };
  special?: { onClick: () => void; disabled: boolean; reason?: string | null };
}

interface FullCardProps {
  def: CardDef;
  instance?: CardInstance;
  state?: GameState;
  /** Rendered width in px (card keeps a 300×419 design ratio). */
  width?: number;
  /** When provided, the base/special rows become clickable on the card. */
  actions?: CardActions;
}

const ELEMENT_COLOR: Record<string, string> = {
  fire: "#E0653C", water: "#3C9FE0", thunder: "#E0C13C", ice: "#5CC6E0", sand: "#C2925A", poison: "#8FB84A",
};
const ELEMENT_LABEL: Record<string, string> = {
  fire: "Feu", water: "Eau", thunder: "Foudre", ice: "Glace", sand: "Sable", poison: "Poison",
};

export default function FullCard({ def, instance, state, width = 300, actions }: FullCardProps) {
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

  // Compact, clickable action row — sized like the design examples (no big box).
  const actionRow = (
    icon: string, name: string, accent: string, rightTop: string, rightBot: string | null,
    desc: string | undefined, act: CardActions["base"] | undefined,
  ) => {
    const clickable = !!act;
    const disabled = !!act?.disabled;
    return (
      <div
        onClick={clickable && !disabled ? act!.onClick : undefined}
        className={clickable && !disabled ? "card-action" : undefined}
        style={{
          display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 6,
          borderRadius: 6, padding: "2px 5px", margin: "0 -5px",
          cursor: clickable ? (disabled ? "not-allowed" : "pointer") : "default",
          opacity: disabled ? 0.5 : 1,
        }}
      >
        <div style={{ minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
            <span style={{ color: accent, fontSize: 11 }}>{icon}</span>
            <span style={{ fontFamily: "var(--font-spectral)", fontWeight: 600, fontVariant: "small-caps", fontSize: 13, color: "#fff" }}>{name}</span>
            {clickable && !disabled && <span style={{ color: accent, fontSize: 10, fontWeight: 700 }}>▸</span>}
          </div>
          {desc && <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 10, lineHeight: 1.15, color: "rgba(255,255,255,.75)" }}>{desc}</div>}
          {disabled && act?.reason && <div style={{ fontFamily: "var(--font-oswald)", fontSize: 9, color: "#FF8A80" }}>• {act.reason}</div>}
        </div>
        <div style={{ textAlign: "right", flex: "none" }}>
          <div style={{ fontFamily: "var(--font-oswald)", fontSize: 10, color: "rgba(255,255,255,.9)" }}>{rightTop}</div>
          {rightBot && <div style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 10, color: "#E8B84B" }}>{rightBot}</div>}
        </div>
      </div>
    );
  };

  return (
    <div style={{ width, height: Math.round(H * scale), flex: "none" }}>
      <div style={{ position: "absolute", width: W, height: H, transform: `scale(${scale})`, transformOrigin: "top left", borderRadius: 16, overflow: "hidden", background: "#0b0e13", boxShadow: "0 10px 30px rgba(0,0,0,.5)" }}>
        {/* illustration */}
        {art ? (
          <div style={{ position: "absolute", inset: 0, backgroundImage: `url('${art}')`, backgroundSize: "cover", backgroundPosition: "center 18%" }} />
        ) : (
          <>
            <div style={{ position: "absolute", inset: 0, background: fac.frame }} />
            <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", opacity: 0.12 }}>
              <Crest which={fac.crest} size={200} color="#fff" />
            </div>
          </>
        )}

        {/* top scrim + cost + PV */}
        <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 64, background: "linear-gradient(180deg,rgba(6,9,14,.66),transparent)" }} />
        <div style={{ position: "absolute", top: 5, left: 13, fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 42, color: "#E8B84B", textShadow: "0 2px 6px rgba(0,0,0,.95)", lineHeight: 1 }}>{def.cost}</div>
        {isChar && (
          <div style={{ position: "absolute", top: 9, right: 12, display: "flex", alignItems: "flex-start", gap: 2 }}>
            <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 34, color: hpc, textShadow: "0 2px 6px rgba(0,0,0,.95)", lineHeight: 1 }}>{curPv}</span>
            <Heart size={14} color={hpc} style={{ marginTop: 4 }} />
          </div>
        )}

        {/* bottom text box — compact, leaves the upper half for the illustration */}
        <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, padding: "20px 13px 10px", display: "flex", flexDirection: "column", gap: 3, background: "linear-gradient(180deg,transparent,rgba(7,9,13,.74) 26%,rgba(7,9,13,.97))" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-end", gap: 8 }}>
            <div style={{ minWidth: 0 }}>
              <div style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 18, color: "#fff", lineHeight: 1.05, textShadow: "0 1px 4px rgba(0,0,0,.95)" }}>{def.name}</div>
              <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 10, color: "rgba(255,255,255,.6)" }}>{fac.label} · {def.type}{def.subtype ? ` · ${def.subtype}` : ""} · {def.rarity}</div>
            </div>
            {isChar && (
              <div style={{ display: "flex", gap: 8, flex: "none" }}>
                <span style={{ display: "flex", alignItems: "center", gap: 2, fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 15, color: "#FF7062" }}><Sword size={12} />{atk}</span>
                <span style={{ display: "flex", alignItems: "center", gap: 2, fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 15, color: "#7FB0E8" }}><Shield size={12} />{dfv}</span>
              </div>
            )}
          </div>

          {traits.length > 0 && (
            <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
              {traits.map((t) => (
                <span key={t} style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 9, color: "#fff", background: TRAIT_COLOR[t] ?? "rgba(255,255,255,.2)", borderRadius: 99, padding: "1px 7px" }}>{TRAIT_LABEL[t] ?? t}</span>
              ))}
            </div>
          )}

          {def.passive && (
            <div>
              <div style={{ fontFamily: "var(--font-spectral)", fontWeight: 600, fontVariant: "small-caps", fontSize: 12.5, color: "#fff", lineHeight: 1.1 }}>{def.passive.name}</div>
              <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 10, lineHeight: 1.18, color: "rgba(255,255,255,.78)" }}>{def.passive.description}</div>
            </div>
          )}

          {(base || spec) && <div style={{ height: 1, margin: "1px 0", background: "linear-gradient(90deg,rgba(255,255,255,.35),transparent)" }} />}

          {base && actionRow(base.isSupport ? "✦" : "⚔", base.name, "#5BC46A", "Gratuit", base.isSupport ? null : `${atk} ATK`, base.element ? `${base.description ?? ""} (${ELEMENT_LABEL[base.element]})` : base.description, actions?.base)}
          {spec && actionRow("★", spec.name, "#FF7062", `${spec.cost} Vol.`, `+${spec.atkBonus} ATK`, spec.element ? `${spec.description ?? ""} (${ELEMENT_LABEL[spec.element]})` : spec.description, actions?.special)}

          {!isChar && (
            <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 11, lineHeight: 1.25, color: "rgba(255,255,255,.82)" }}>
              {def.type === "object" && <>{def.bonusAtk ? `+${def.bonusAtk} ATK ` : ""}{def.bonusDef ? `+${def.bonusDef} DEF ` : ""}{def.equipEffect ?? ""}</>}
              {def.type === "ship" && <>{def.shipPassive}{def.shipActive ? ` — ${def.shipActive.name} (${def.shipActive.cost}V)` : ""}</>}
              {def.type === "counter" && def.counterEffect && (def.counterEffect.type === "survive" ? def.counterEffect.description : `Réduit ${def.counterEffect.amount} dégâts.`)}
              {def.type === "event" && def.eventEffect && describeEvent(def.eventEffect)}
            </div>
          )}

          <div style={{ fontFamily: "var(--font-oswald)", fontSize: 8, letterSpacing: ".06em", color: "rgba(255,255,255,.4)", textAlign: "right" }}>
            {(base?.element || spec?.element) && <span style={{ display: "inline-block", width: 5, height: 5, borderRadius: 99, marginRight: 4, verticalAlign: "middle", background: ELEMENT_COLOR[(spec?.element || base?.element)!] ?? "#888" }} />}
            {def.set}
          </div>
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
