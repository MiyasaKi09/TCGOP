//! TCGOP — native game entry point (Bevy 0.19).
//!
//! The binary does three things and nothing else: it configures the window,
//! adds `DefaultPlugins` and registers [`TcgopPlugin`]. Every screen, system
//! and resource lives under [`app`] and the feature modules it wires together.
//!
//! Note on filtering: `ImagePlugin::default_nearest()` is deliberately **not**
//! used — the card illustrations are photographic JPEGs and must be sampled
//! with the default linear filter.

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
use bevy::window::{WindowResizeConstraints, WindowResolution};

use crate::app::{TcgopPlugin, layout};
use crate::board::geometry::{min_window_h, min_window_w};

/// The footer's CTA row and rolling log stop being readable below this width;
/// the terrain itself needs less ([`min_window_w`]).
const MIN_FOOTER_W: f32 = 720.0;

/// The requested client area, in **logical** pixels.
///
/// [`WindowResolution::new`] takes *physical* pixels, while every `px()` in
/// [`app::layout`] is logical, so on a HiDPI display (`scale_factor` 2) asking
/// for `1280x800` physical would hand the layout a 640x400 canvas and clip the
/// board from the first frame. Multiplying by the scale factor is not possible
/// before the monitor is known, so the resolution is built from the physical
/// size **and** told to report exactly [`layout::WINDOW_W`] x
/// [`layout::WINDOW_H`] logical pixels, whatever the display does.
fn resolution() -> WindowResolution {
    WindowResolution::new(layout::WINDOW_W as u32, layout::WINDOW_H as u32)
        .with_scale_factor_override(1.0)
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "TCGOP — One Piece Grand Line TCG".into(),
                resolution: resolution(),
                // The board re-fits itself on every `WindowResized`
                // (`board::relayout_on_resize`), so resizing is allowed — but
                // never below the size the two halves still fit in.
                resizable: true,
                // Below these the halves would be clipped by the terrain's
                // `overflow: clip`, so winit refuses to shrink further instead.
                resize_constraints: WindowResizeConstraints {
                    min_width: min_window_w().max(MIN_FOOTER_W),
                    min_height: min_window_h(),
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TcgopPlugin)
        .run();
}
