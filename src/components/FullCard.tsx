"use client";

import type { CardDef, CardInstance, GameState } from "@/types";
import { getEffectiveAtk, getEffectiveDef } from "@/engine/board";
import { getCardDef } from "@/engine/cardRegistry";
import { CARD_ART, faction, TRAIT_COLOR, TRAIT_LABEL, QUOTE, roleOf, setCode } from "@/data/cardArt";
import { Heart, Crest } from "./icons";

export interface CardActions {
  base?: { onClick: () => void; disabled: boolean; reason?: string | null };
  special?: { onClick: () => void; disabled: boolean; reason?: string | null };
}

interface FullCardProps {
  def: CardDef;
  instance?: CardInstance;
  state?: GameState;
  width?: number;
  actions?: CardActions;
}

const ELEMENT = { fire: ["Feu", "#E0653C"], water: ["Eau", "#3C9FE0"], thunder: ["Foudre", "#C9A82E"], ice: ["Glace", "#5CC6E0"], sand: ["Sable", "#C2925A"], poison: ["Poison", "#8FB84A"] } as const;
const ATK_TRAIT: Record<string, string> = { range: "Portée", piercing: "Perçant", zone: "Zone", total: "Total" };
const SHADOW = "0 1px 3px #000, 0 0 6px rgba(0,0,0,.85)";

function ShieldOutline({ size = 22 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth={1.7} strokeLinejoin="round" style={{ filter: "drop-shadow(0 1px 2px rgba(0,0,0,.9))" }}>
      <path d="M12 2 4 5v6c0 5 3.4 9 8 11 4.6-2 8-6 8-11V5z" />
    </svg>
  );
}
function RoleFigure({ size = 26 }: { size?: number }) {
  return (
    <svg width={size * 0.6} height={size} viewBox="0 0 16 26" fill="#fff" opacity={0.92} style={{ filter: "drop-shadow(0 1px 2px rgba(0,0,0,.9))" }}>
      <ellipse cx="8" cy="4" rx="7" ry="2.2" />
      <circle cx="8" cy="3.4" r="2.6" />
      <path d="M4 9c0-2 1.8-3.4 4-3.4s4 1.4 4 3.4v13a1.4 1.4 0 0 1-1.4 1.4H5.4A1.4 1.4 0 0 1 4 22z" />
    </svg>
  );
}

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
  const traits = def.traits ?? [];
  const base = def.baseAction;
  const spec = def.specialAttack;
  const quote = QUOTE[def.id];

  // synergy text lines (vertical, right side)
  let synLines: string[] | null = null;
  if (def.synergies && def.synergies.length) {
    const s = def.synergies[0];
    let partner = s.partnerId;
    try { partner = getCardDef(s.partnerId).name; } catch { /* keep id */ }
    const short = partner.split(" ").pop()!.toUpperCase();
    synLines = [`COMBO — RIVALITÉ (${short})`, `+${s.atkBonus} ATK si ${short} est en jeu`];
    if (s.onPartnerKO) synLines.push(`+${s.onPartnerKO} ATK au tour suivant si ${short} est KO`);
  }

  // a clickable / static action row
  const renderAction = (a: NonNullable<CardDef["baseAction"]> | NonNullable<CardDef["specialAttack"]>, isSpecial: boolean, act: CardActions["base"]) => {
    const support = "isSupport" in a && a.isSupport;
    const heal = support && "healAmount" in a && a.healAmount != null;
    const cost = isSpecial ? `${(a as { cost: number }).cost} Vol.` : "0 Vol.";
    const atkVal = heal ? null : `ATK ${isSpecial ? atk + (a as { atkBonus: number }).atkBonus : atk}`;
    const icon = isSpecial ? "★" : support ? "✦" : "⚔";
    const accent = isSpecial ? "#FF7062" : "#5BC46A";
    const el = a.element ? ELEMENT[a.element as keyof typeof ELEMENT] : null;
    const clickable = !!act;
    const disabled = !!act?.disabled;
    return (
      <div
        onClick={clickable && !disabled ? act!.onClick : undefined}
        className={clickable && !disabled ? "card-action" : undefined}
        style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 6, borderRadius: 6, padding: "2px 5px", margin: "0 -5px", cursor: clickable ? (disabled ? "not-allowed" : "pointer") : "default", opacity: disabled ? 0.5 : 1 }}
      >
        <div style={{ minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 5, flexWrap: "wrap" }}>
            <span style={{ color: accent, fontSize: 11 }}>{icon}</span>
            <span style={{ fontFamily: "var(--font-spectral)", fontWeight: 700, fontVariant: "small-caps", fontSize: 13.5, color: "#fff", letterSpacing: ".02em" }}>{a.name}</span>
            {el && <span style={{ fontFamily: "var(--font-oswald)", fontSize: 8.5, color: "#fff", background: el[1], borderRadius: 99, padding: "0 6px" }}>{el[0]}</span>}
            {a.attackTraits?.map((t) => <span key={t} style={{ fontFamily: "var(--font-oswald)", fontSize: 8.5, color: "#fff", background: "rgba(255,255,255,.22)", borderRadius: 99, padding: "0 6px" }}>{ATK_TRAIT[t] ?? t}</span>)}
            {clickable && !disabled && <span style={{ color: accent, fontSize: 10, fontWeight: 700 }}>▸</span>}
          </div>
          {a.description && <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 10, lineHeight: 1.15, color: "rgba(255,255,255,.78)" }}>{a.description}</div>}
          {disabled && act?.reason && <div style={{ fontFamily: "var(--font-oswald)", fontSize: 9, color: "#FF8A80" }}>• {act.reason}</div>}
        </div>
        <div style={{ textAlign: "right", flex: "none" }}>
          <div style={{ fontFamily: "var(--font-oswald)", fontSize: 10.5, color: "rgba(255,255,255,.92)" }}>{cost}</div>
          {atkVal && <div style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 10.5, color: "rgba(255,255,255,.92)" }}>{atkVal}</div>}
        </div>
      </div>
    );
  };

  return (
    <div style={{ width, height: Math.round(H * scale), flex: "none" }}>
      <div style={{ position: "absolute", width: W, height: H, transform: `scale(${scale})`, transformOrigin: "top left", borderRadius: 16, overflow: "hidden", background: "#0b0e13", boxShadow: "0 10px 30px rgba(0,0,0,.5)" }}>
        {/* illustration */}
        {art ? (
          <div style={{ position: "absolute", inset: 0, backgroundImage: `url('${art}')`, backgroundSize: "cover", backgroundPosition: "center 16%" }} />
        ) : (
          <>
            <div style={{ position: "absolute", inset: 0, background: fac.frame }} />
            <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", opacity: 0.12 }}><Crest which={fac.crest} size={200} color="#fff" /></div>
          </>
        )}

        {/* scrims */}
        <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 70, background: "linear-gradient(180deg,rgba(6,9,14,.6),transparent)" }} />
        <div style={{ position: "absolute", top: 0, bottom: 0, left: 0, width: 46, background: "linear-gradient(90deg,rgba(6,9,14,.55),transparent)" }} />

        {/* cost (top-left, gold) */}
        <div style={{ position: "absolute", top: 2, left: 12, fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 52, color: "#E8B84B", textShadow: "0 2px 6px rgba(0,0,0,.95)", lineHeight: 1, zIndex: 5 }}>{def.cost}</div>

        {/* PV (top-right, gold + heart) */}
        <div style={{ position: "absolute", top: 6, right: 12, display: "flex", alignItems: "flex-start", gap: 3, zIndex: 5 }}>
          <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 46, color: "#E8B84B", textShadow: "0 2px 6px rgba(0,0,0,.95)", lineHeight: 1 }}>{curPv}</span>
          <Heart size={18} color="#D8453C" style={{ marginTop: 6 }} />
        </div>

        {/* name + quote — two parallel vertical columns (left) */}
        <div style={{ position: "absolute", left: 6, top: "14%", display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 6, writingMode: "vertical-rl", transform: "rotate(180deg)", zIndex: 4 }}>
          <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 23, letterSpacing: ".03em", color: "#fff", textShadow: SHADOW, whiteSpace: "nowrap" }}>{def.name}</span>
          {quote && <span style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 11, color: "rgba(255,255,255,.8)", textShadow: SHADOW, whiteSpace: "nowrap" }}>“{quote}”</span>}
        </div>

        {/* synergy (vertical, right) */}
        {synLines && (
          <div style={{ position: "absolute", right: 7, top: "12%", display: "flex", flexDirection: "row", gap: 6, writingMode: "vertical-rl", transform: "rotate(180deg)", zIndex: 4, maxHeight: "56%" }}>
            {synLines.map((l, i) => (
              <span key={i} style={{ fontFamily: i === 0 ? "var(--font-oswald)" : "var(--font-spectral)", fontStyle: i === 0 ? "normal" : "italic", fontVariant: i === 0 ? "small-caps" : "normal", fontSize: i === 0 ? 11 : 10, fontWeight: i === 0 ? 700 : 400, color: "rgba(255,255,255,.88)", textShadow: SHADOW, whiteSpace: "nowrap" }}>{l}</span>
            ))}
          </div>
        )}

        {/* DEF (right) */}
        {isChar && (
          <div style={{ position: "absolute", right: 12, top: "48%", display: "flex", alignItems: "center", gap: 3, zIndex: 4 }}>
            <ShieldOutline size={22} />
            <span style={{ fontFamily: "var(--font-oswald)", fontWeight: 700, fontSize: 22, color: "#fff", textShadow: SHADOW }}>{dfv}</span>
          </div>
        )}

        {/* traits (right, below DEF) */}
        {traits.length > 0 && (
          <div style={{ position: "absolute", right: 10, top: "55%", maxWidth: 150, display: "flex", flexWrap: "wrap", justifyContent: "flex-end", gap: 4, zIndex: 4 }}>
            {traits.map((t) => (
              <span key={t} style={{ fontFamily: "var(--font-oswald)", fontWeight: 600, fontSize: 10, color: "#fff", background: TRAIT_COLOR[t] ?? "rgba(255,255,255,.2)", borderRadius: 99, padding: "1px 8px", boxShadow: "0 1px 2px rgba(0,0,0,.5)" }}>{TRAIT_LABEL[t] ?? t}</span>
            ))}
          </div>
        )}

        {/* role figure (left, above text box) */}
        <div style={{ position: "absolute", left: 13, top: "51%", zIndex: 4 }}><RoleFigure size={26} /></div>

        {/* bottom text box — inset with a margin from the card edges */}
        <div style={{ position: "absolute", left: 8, right: 8, bottom: 8, padding: "11px 11px 9px", borderRadius: 12, display: "flex", flexDirection: "column", gap: 2, background: "linear-gradient(180deg,rgba(7,9,13,.55),rgba(7,9,13,.9) 42%,rgba(7,9,13,.96))", zIndex: 2 }}>
          {def.passive && (
            <div>
              <div style={{ fontFamily: "var(--font-spectral)", fontWeight: 700, fontVariant: "small-caps", fontSize: 14, color: "#fff", letterSpacing: ".02em" }}>{def.passive.name}</div>
              <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 10.5, lineHeight: 1.2, color: "rgba(255,255,255,.82)" }}>{def.passive.description}</div>
            </div>
          )}
          {(base || spec) && <div style={{ height: 1, margin: "2px 0", background: "linear-gradient(90deg,rgba(255,255,255,.4),transparent)" }} />}
          {base && renderAction(base, false, actions?.base)}
          {spec && renderAction(spec, true, actions?.special)}
          {!isChar && (
            <div style={{ fontFamily: "var(--font-spectral)", fontStyle: "italic", fontSize: 11, lineHeight: 1.25, color: "rgba(255,255,255,.82)" }}>
              {def.type === "object" && <>{def.bonusAtk ? `+${def.bonusAtk} ATK ` : ""}{def.bonusDef ? `+${def.bonusDef} DEF ` : ""}{def.equipEffect ?? ""}</>}
              {def.type === "ship" && <>{def.shipPassive}{def.shipActive ? ` — ${def.shipActive.name} (${def.shipActive.cost}V)` : ""}</>}
              {def.type === "counter" && def.counterEffect && (def.counterEffect.type === "survive" ? def.counterEffect.description : `Réduit ${def.counterEffect.amount} dégâts.`)}
              {def.type === "event" && def.eventEffect && describeEvent(def.eventEffect)}
            </div>
          )}
          <div style={{ fontFamily: "var(--font-oswald)", fontSize: 8.5, letterSpacing: ".05em", color: "rgba(255,255,255,.5)", textAlign: "right", marginTop: 1 }}>
            {setCode(def.faction)} · {fac.label} · {roleOf(def)}
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
