# TCGOP — native game (Rust + Bevy)

Native rewrite of the One Piece Grand Line TCG: the complete rules engine
(every card, every mechanic, the three AI levels) and the game client
(board, hand, menus, animations) in Rust.

```
rust/
├── Cargo.toml              # workspace
├── crates/
│   ├── engine/             # tcgop_engine — pure, deterministic rules library (no rendering)
│   └── game/               # tcgop_game   — Bevy client; binary name `tcgop`
│       └── assets/         # cards/ & decks/ → symlinks to ../../public ; fonts/ bundled (OFL)
└── README.md
```

## Design

* **Engine first.** `tcgop_engine` mirrors the original TypeScript engine 1:1:
  `GameState` is plain serialisable data, `valid_actions(&state)` lists every
  legal `GameAction`, `apply(&mut state, action)` resolves it and appends to the
  log / announcement queue. Randomness is a seeded `ChaCha8` RNG, so any game is
  reproducible from its seed. The crate has no Bevy dependency and is covered by
  `cargo test` (unit tests per mechanic + seeded AI-vs-AI games).
* **Client second.** `tcgop_game` drives the engine from Bevy systems: a single
  flat battlefield (opponent's ship deck on top, yours below, cut at the
  waterline), front/back lines of three slots per side, dedicated captain /
  ship / Volonté command bars, full-illustration unit tiles (details on click),
  fanned hand, animated sea, per-element combat VFX and cut-ins.
* **Nothing the engine allows is out of reach.** Every `GameAction` that
  `valid_actions()` can return has a path in the UI, and every rule the engine
  enforces is *shown* rather than left to be discovered:
  * the captain menu prices and plays the verso's ★ special attack, its
    `surcharge`, and the awakened Devil Fruit the captain wears — a recto
    captain's printed attacks are listed with the rule that forbids them
    ("Le Capitaine recto ne peut pas attaquer");
  * *Engager* says what the flip costs, or which printed clause is making it
    free (an ally KO'd, a Cursed enemy in play, enough allies, the turn count);
  * a unit's popover carries its base attack **and**, separately, its support
    effect (a support character may do both in one turn), each awakened
    fruit's own special, *Éveiller* for a fruit it has not awakened, and the
    once-per-turn free move — every one of them greyed out with the reason
    when the engine is not offering it;
  * permanent maximum-PV loss ("Sable") shortens the HP gauge and is spelled
    out, blocked healing and *Provocation* (which unit a taunt binds you to)
    are named, and natural Haki — in either printed form — is flagged on the
    units that pierce a Logia;
  * ally-facing and self-targeting specials light up on your own half, and
    every special attack — the captain's, the surcharge, an awakened fruit's
    — gets the same cut-in, under its own name.

## Build & run

Requires a stable Rust toolchain (≥ 1.95 for Bevy 0.19).

```sh
cd rust
cargo test -p tcgop_engine          # rules engine tests
cargo run  -p tcgop_game --release  # launch the game
```

Bevy is built with a trimmed feature set (X11 window backend, 2D + UI, PNG/JPEG).
Audio and gamepad support are disabled, so on Linux only these dev packages
are needed:

```sh
sudo apt-get install -y pkg-config libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxkbcommon-dev
```

macOS and Windows need nothing beyond the Rust toolchain.

The first build compiles Bevy (several minutes); later builds are incremental.

## Shipping a build

`crates/game/assets/cards` and `assets/decks` are symlinks into the web app's
`public/` folder so both versions share the same artwork. When distributing the
binary, copy the assets **dereferenced** next to it (see `.github/workflows/rust.yml`):

```sh
cp -RL crates/game/assets dist/assets
```

## Fonts

`assets/fonts/` bundles Bangers, Cinzel, Oswald and Spectral (SIL Open Font
License, via the `@fontsource/*` packages — licence text included) plus DejaVu
as a fallback.
