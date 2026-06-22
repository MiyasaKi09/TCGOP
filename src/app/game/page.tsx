"use client";

import { Suspense } from "react";
import { useSearchParams } from "next/navigation";
import Game from "@/components/Game";
import { mugiwaraDeck, marinesDeck, baroqueDeck, redhairDeck } from "@/data/decks";
import type { DeckDef } from "@/types";

const DECKS: Record<string, DeckDef> = {
  mugiwara: mugiwaraDeck,
  marines: marinesDeck,
  baroque: baroqueDeck,
  redhair: redhairDeck,
};

function GameContent() {
  const params = useSearchParams();
  const deckChoice = params.get("deck") ?? "mugiwara";
  const foeChoice = params.get("foe") ?? "";

  const playerDeck = DECKS[deckChoice] ?? mugiwaraDeck;
  const aiDeck =
    DECKS[foeChoice] ??
    (deckChoice === "marines" ? mugiwaraDeck : marinesDeck);

  return <Game playerDeck={playerDeck} aiDeck={aiDeck} />;
}

export default function GamePage() {
  return (
    <Suspense
      fallback={
        <div className="flex items-center justify-center min-h-screen text-gray-400">
          Chargement du combat...
        </div>
      }
    >
      <GameContent />
    </Suspense>
  );
}
