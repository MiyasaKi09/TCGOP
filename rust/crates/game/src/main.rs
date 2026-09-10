//! TCGOP — native game entry point (Bevy 0.19).
//!
//! The binary does three things and nothing else: it configures the window,
//! adds `DefaultPlugins` and registers [`TcgopPlugin`]. Every screen, system
//! and resource lives under [`app`] and the feature modules it wires together.
//!
//! Note on filtering: `ImagePlugin::default_nearest()` is deliberately **not**
//! used — the card illustrations are photographic JPEGs and must be sampled
//! with the default linear filter.

// This is a binary crate, so `pub` items are not part of any public API and
// `dead_code` fires on everything the skeleton exposes for the feature modules
// (palette fields, layout constants, `Session` accessors, the pure selection
// helpers). Drop this attribute once the feature modules consume them.
#![allow(dead_code)]

mod ai_driver;
mod app;
mod art;
mod board;
mod bridge;
#[cfg(test)]
mod e2e;
mod hand;
mod panels;
mod screens;
mod selection;
mod vfx;

use bevy::prelude::*;
use bevy::window::WindowResolution;

use crate::app::{TcgopPlugin, layout};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "TCGOP — One Piece Grand Line TCG".into(),
                resolution: WindowResolution::new(
                    layout::WINDOW_W as u32,
                    layout::WINDOW_H as u32,
                ),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TcgopPlugin)
        .run();
}
