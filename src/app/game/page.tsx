"use client";

import { Suspense } from "react";
import { useSearchParams } from "next/navigation";
import Game from "@/components/Game";
import { mugiwaraDeck, marinesDeck } from "@/data/decks";

function GameContent() {
  const params = useSearchParams();
  const deckChoice = params.get("deck") ?? "mugiwara";

  const playerDeck = deckChoice === "marines" ? marinesDeck : mugiwaraDeck;
  const aiDeck = deckChoice === "marines" ? mugiwaraDeck : marinesDeck;

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
