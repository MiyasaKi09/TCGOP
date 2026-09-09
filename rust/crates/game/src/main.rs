//! TCGOP — native game entry point (Bevy).
//!
//! This is the bootstrap: window, camera, asset pipeline sanity (loads the sea
//! backdrop and a card illustration) and a title. The real screens live in the
//! modules added by the port.

use bevy::prelude::*;
use bevy::text::FontSize;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "TCGOP — One Piece Grand Line TCG".into(),
                resolution: (1280u32, 800u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn(Camera2d);

    // Sea backdrop (from the shared `public/` assets).
    commands.spawn((
        Sprite::from_image(assets.load("decks/sea.png")),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));

    // A card illustration, to prove JPEG decoding works.
    commands.spawn((
        Sprite {
            image: assets.load("cards/luffy-recto.jpg"),
            custom_size: Some(Vec2::new(300.0, 419.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    commands.spawn((
        Text2d::new("TCGOP"),
        TextFont { font_size: FontSize::Px(64.0), ..default() },
        TextColor(Color::srgb(0.91, 0.72, 0.29)),
        Transform::from_xyz(0.0, 320.0, 1.0),
    ));
}
