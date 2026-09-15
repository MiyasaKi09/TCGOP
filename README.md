# TCGOP — One Piece Grand Line TCG

Un jeu de cartes à collectionner dans l'univers de One Piece : 4 decks
(Mugiwara, Marines, Baroque Works, Red Hair), 118 cartes, capitaines
recto/verso, navires, fruits du démon, Haki, Volonté, et une IA à 3 niveaux.

Deux implémentations vivent dans ce dépôt :

| | Web | Native |
|---|---|---|
| Stack | Next.js 16 · React 19 · Tailwind v4 · PixiJS | Rust · Bevy 0.19 |
| Code | `src/` | `rust/` (`crates/engine` + `crates/game`) |
| Lancer | `npm install && npm run dev` | `cd rust && cargo run -p tcgop_game --release` |
| Déployé | https://tcgop.vercel.app | binaires desktop (voir CI) |

Le moteur de règles Rust (`tcgop_engine`) est un portage vérifié 1:1 du moteur
TypeScript (spec : `rust/docs/PORT-SPEC.md`), puis corrigé et complété
(décisions : `rust/docs/RULES-DECISIONS.md`). Les illustrations (`public/`)
sont partagées par les deux versions.

Voir `rust/README.md` pour la version native (build par OS, structure, polices).
