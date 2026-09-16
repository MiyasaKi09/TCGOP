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

/** Abréviation tenant dans la barre de commandement, sans perdre le nom. */
const SHORT: Record<string, string> = {
  observation: "Observ.",
  armament: "Armement",
  king: "Rois",
};

/** Classe d'état, dans l'ordre de priorité d'affichage. */
function chipClass(h: HakiStatus): string {
  if (!h.unlocked) return "locked";
  if (h.type === "armament") return "passive";
  if (h.spent) return "spent";
  if (h.available) return "ready";
  return "locked"; // débloqué mais condition non remplie (ex. pas de Conquérant)
}

/**
 * Cartouche de droite : ce que le joueur doit pouvoir anticiper d'un coup d'œil.
 * Verrouillé → la manche d'arrivée. Débloqué → l'état.
 */
function badge(h: HakiStatus): string {
  if (!h.unlocked) return `M${h.unlockTurn}`;
  if (h.type === "armament") return "Actif";
  if (h.spent) return "Utilisé";
  if (h.available) return "Prêt";
  return "Bloqué";
}

/** Infobulle native : règle complète, sans ouvrir le panneau. */
function tooltip(h: HakiStatus): string {
  const state = h.blockedReason ?? (h.type === "armament" ? "Actif (passif)" : "Prêt à l'emploi");
  return `${h.glyph} ${h.label}\nDisponible à la manche ${h.unlockTurn}\n${h.effect}\n${h.limit}\n➜ ${state}`;
}

/**
 * Bandeau d'état des trois Haki, affiché pour les deux camps : voir que
 * l'adversaire a son Observation prête, ou déjà dépensée, fait partie de
 * l'information de jeu. Les règles viennent du moteur (`getHakiStatus`),
 * jamais de texte recopié ici.
 */
export default function HakiBar({ state, playerId, align = "left", onOpenHelp }: HakiBarProps) {
  const statuses = getHakiStatus(state, playerId);
  const anyReady = statuses.some((h) => h.type !== "armament" && h.available);

  return (
    <button
      type="button"
      onClick={onOpenHelp}
      title="Voir les règles des Haki"
      className={`flex items-center gap-1 flex-wrap ${align === "right" ? "justify-end" : ""}`}
      style={{ background: "none", border: 0, padding: 0, cursor: "pointer" }}
    >
      {/* Les Haki ne coûtent aucune Volonté : c'est dit une fois, à la source. */}
      <span
        className="font-oswald text-[8px] uppercase tracking-widest"
        style={{ color: anyReady ? "var(--gold)" : "rgba(255,255,255,.6)" }}
      >
        Haki · gratuit
      </span>
      {statuses.map((h) => (
        <span key={h.type} className={`haki-chip ${chipClass(h)}`} title={tooltip(h)}>
          <span className="g">{h.glyph}</span>
          <span>{SHORT[h.type] ?? h.type}</span>
          <span className="w">{badge(h)}</span>
        </span>
      ))}
    </button>
  );
}
