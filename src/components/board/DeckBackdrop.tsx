"use client";

// The real top-down ship-deck art from the maquette (transparent PNG → the
// animated sea shows around it). The slot grid + captains overlay the deck.
export default function DeckBackdrop({ deckImg = "/decks/mugiwara-deck.png" }: { deckImg?: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center pointer-events-none select-none" aria-hidden>
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={deckImg}
        alt=""
        draggable={false}
        className="max-w-full max-h-full object-contain"
        style={{ width: "98%", height: "100%", filter: "drop-shadow(0 26px 70px rgba(0,0,0,.66))" }}
      />
    </div>
  );
}
