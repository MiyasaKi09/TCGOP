"use client";

import type { GameState, PlayerId } from "@/types";
import { getHakiStatus, type HakiStatus } from "@/engine/haki";
import { VOLONTE_CAP, STARTING_HAND_SIZE, HAND_LIMIT, ALLY_KO_BONUS_VOL } from "@/engine/utils";

interface HelpPanelProps {
  state: GameState;
  humanPlayer: PlayerId;
  onClose: () => void;
}

/** Résumé d'état lisible pour un camp, dérivé du moteur. */
function stateLabel(h: HakiStatus): { text: string; color: string } {
  if (!h.unlocked) return { text: `Manche ${h.unlockTurn}`, color: "rgba(255,255,255,.35)" };
  if (h.type === "armament") return { text: "Actif", color: "#7FB0E8" };
  if (h.spent) return { text: "Dépensé", color: "#FF7062" };
  if (h.available) return { text: "Prêt", color: "#5BC46A" };
  return { text: h.blockedReason ?? "Indisponible", color: "rgba(255,255,255,.45)" };
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className="font-cinzel text-[13px] font-bold text-gold uppercase tracking-wider">{title}</h3>
      <div className="font-spectral text-[12px] text-white/75 leading-relaxed flex flex-col gap-1">{children}</div>
    </section>
  );
}

/**
 * Panneau de règles. Les seuils, coûts et états des Haki sont lus dans le moteur
 * (`getHakiStatus`, constantes de `utils`), donc cette aide ne peut pas mentir :
 * si une règle change dans le moteur, ce panneau suit.
 */
export default function HelpPanel({ state, humanPlayer, onClose }: HelpPanelProps) {
  const opponent: PlayerId = humanPlayer === "player1" ? "player2" : "player1";
  const mine = getHakiStatus(state, humanPlayer);
  const theirs = getHakiStatus(state, opponent);
  const volonteThisTurn = Math.min(state.turnNumber, VOLONTE_CAP);

  return (
    <div
      className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        className="panel panel-gold halftone p-5 max-w-2xl w-full max-h-[90vh] overflow-y-auto flex flex-col gap-4 animate-modal-enter"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between">
          <div>
            <h2 className="font-cinzel text-xl font-bold text-white">Règles du jeu</h2>
            <p className="font-oswald text-[10px] uppercase tracking-widest text-white/45 mt-0.5">
              Manche {state.turnNumber}
            </p>
          </div>
          <button onClick={onClose} className="text-white/40 hover:text-white text-2xl leading-none" aria-label="Fermer">
            ✕
          </button>
        </div>

        {/* --- Haki : la demande explicite — quand, à quel prix, sous quelle condition --- */}
        <Section title="Les trois Haki">
          <p className="text-white/60">
            Les Haki ne coûtent aucune Volonté. Ils se débloquent au numéro de manche et se rechargent
            selon leur propre limite.
          </p>
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse mt-1">
              <thead>
                <tr className="font-oswald text-[9px] uppercase tracking-wider text-white/45">
                  <th className="py-1 pr-2 font-bold">Haki</th>
                  <th className="py-1 pr-2 font-bold">Dès</th>
                  <th className="py-1 pr-2 font-bold">Limite</th>
                  <th className="py-1 pr-2 font-bold text-right">Toi</th>
                  <th className="py-1 font-bold text-right">Adversaire</th>
                </tr>
              </thead>
              <tbody>
                {mine.map((h, i) => {
                  const me = stateLabel(h);
                  const them = stateLabel(theirs[i]);
                  return (
                    <tr key={h.type} style={{ borderTop: "1px solid rgba(255,255,255,.08)" }}>
                      <td className="py-1.5 pr-2">
                        <div className="font-oswald text-[11px] font-bold text-white/90">
                          {h.glyph} {h.label.replace("Haki de l'", "").replace("Haki des ", "")}
                        </div>
                        <div className="text-[11px] text-white/55">{h.effect}</div>
                      </td>
                      <td className="py-1.5 pr-2 font-oswald text-[11px] text-white/70 whitespace-nowrap">
                        M{h.unlockTurn}
                      </td>
                      <td className="py-1.5 pr-2 text-[11px] text-white/55">{h.limit}</td>
                      <td className="py-1.5 pr-2 font-oswald text-[11px] font-bold text-right whitespace-nowrap" style={{ color: me.color }}>
                        {me.text}
                      </td>
                      <td className="py-1.5 font-oswald text-[11px] font-bold text-right whitespace-nowrap" style={{ color: them.color }}>
                        {them.text}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <p className="text-white/60 mt-1">
            L&apos;Observation se déclenche <strong className="text-white/85">quand tu es attaqué</strong> : le bouton
            apparaît dans la fenêtre de contre, à côté des cartes de contre et du blocage. Certaines attaques
            sont marquées inesquivables et l&apos;ignorent.
          </p>
        </Section>

        {/* --- Fenêtre de contre : le moment le plus opaque du jeu --- */}
        <Section title="Quand tu es attaqué">
          <p>
            Une attaque ne s&apos;applique pas immédiatement : elle ouvre une <strong className="text-white/85">fenêtre
            de contre</strong> où tu choisis ta réaction.
          </p>
          <ul className="list-disc pl-5 flex flex-col gap-0.5 text-white/70">
            <li><strong className="text-white/85">Carte de contre</strong> : payée en Volonté, elle réduit ou annule.</li>
            <li><strong className="text-white/85">Bloquer</strong> : un allié Bouclier adjacent et non engagé encaisse à la place.</li>
            <li><strong className="text-white/85">Haki de l&apos;Observation</strong> : esquive totale, une fois par manche.</li>
            <li><strong className="text-white/85">Subir</strong> : les dégâts passent.</li>
          </ul>
        </Section>

        {/* --- Économie du tour --- */}
        <Section title="Volonté et tour">
          <p>
            À chaque manche, ta Volonté est <strong className="text-white/85">remise</strong> au numéro de manche,
            plafonnée à {VOLONTE_CAP}. Cette manche : <strong className="text-gold">{volonteThisTurn}</strong>. Elle ne
            se cumule pas d&apos;une manche à l&apos;autre, donc dépenser ce qui reste est rarement une perte.
          </p>
          <p>
            Quand un de tes personnages est mis KO, tu reçois <strong className="text-white/85">+{ALLY_KO_BONUS_VOL} Volonté</strong> :
            perdre une unité te redonne du tempo.
          </p>
          <p>
            Main de départ : {`${STARTING_HAND_SIZE} cartes`}. Au-delà de {`${HAND_LIMIT} cartes`}, l&apos;excédent
            est défaussé en fin de tour.
          </p>
        </Section>

        {/* --- Plateau --- */}
        <Section title="Placement et actions">
          <p>
            Chaque camp a une <strong className="text-white/85">ligne avant</strong> et une{" "}
            <strong className="text-white/85">ligne arrière</strong> de trois cases. La ligne avant protège
            l&apos;arrière : tant qu&apos;elle est occupée, les unités sans Portée ne peuvent pas frapper derrière.
          </p>
          <p>
            Un personnage agit <strong className="text-white/85">une fois par tour</strong> et s&apos;engage en agissant.
            Fraîchement déployé, il ne peut pas attaquer le tour même, sauf s&apos;il a Rush.
          </p>
          <p className="text-white/60">
            Clique une unité pour voir ses actions, avec la raison affichée quand l&apos;une d&apos;elles est refusée.
          </p>
        </Section>

        <button onClick={onClose} className="btn btn-gold action-btn px-4 py-2 text-sm self-end">
          Fermer
        </button>
      </div>
    </div>
  );
}
