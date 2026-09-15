"use client";

import type { GameState, PlayerId } from "@/types";
import { getHakiStatus, type HakiStatus } from "@/engine/haki";

interface HakiBarProps {
  state: GameState;
  playerId: PlayerId;
  /** Aligne le contenu à droite pour la barre adverse. */
  align?: "left" | "right";
  /** Ouvre le panneau d'aide détaillé. */
  onOpenHelp: () => void;
}

/** Classe d'état du pip, dans l'ordre de priorité d'affichage. */
function pipClass(h: HakiStatus): string {
  if (!h.unlocked) return "locked";
  if (h.type === "armament") return "passive";
  if (h.spent) return "spent";
  if (h.available) return "ready";
  return "locked"; // débloqué mais condition non remplie (ex. pas de Conquérant)
}

/** Une ligne de résumé lisible, utilisée comme infobulle native. */
function tooltip(h: HakiStatus): string {
  const head = `${h.glyph} ${h.label} — manche ${h.unlockTurn}`;
  const state = h.blockedReason ?? (h.type === "armament" ? "Actif (passif)" : "Prêt");
  return `${head}\n${h.effect}\n${h.limit}\n➜ ${state}`;
}

/**
 * Bandeau d'état des trois Haki. Toujours affiché pour les deux camps : voir que
 * l'adversaire a son Observation prête (ou déjà dépensée) fait partie de
 * l'information de jeu. Les règles viennent du moteur (`getHakiStatus`), jamais
 * de texte recopié ici.
 */
export default function HakiBar({ state, playerId, align = "left", onOpenHelp }: HakiBarProps) {
  const statuses = getHakiStatus(state, playerId);
  const anyReady = statuses.some((h) => h.type !== "armament" && h.available);

  return (
    <button
      type="button"
      onClick={onOpenHelp}
      title="Voir les règles des Haki"
      className={`flex items-center gap-1 ${align === "right" ? "justify-end" : ""}`}
      style={{ background: "none", border: 0, padding: 0, cursor: "pointer" }}
    >
      <span
        className="font-oswald text-[8px] uppercase tracking-widest"
        style={{ color: anyReady ? "var(--gold)" : "rgba(255,255,255,.55)" }}
      >
        Haki
      </span>
      {statuses.map((h) => (
        <span key={h.type} className={`haki-pip ${pipClass(h)}`} title={tooltip(h)}>
          {h.glyph}
        </span>
      ))}
      {/* Prochain déblocage : le joueur sait quoi attendre et quand. */}
      {(() => {
        const next = statuses.find((h) => !h.unlocked);
        return next ? <span className="haki-eta">M{next.unlockTurn}</span> : null;
      })()}
    </button>
  );
}
