//! A tiny tweening engine — the Bevy equivalent of the CSS keyframes the web
//! client leans on (`vfx-proj`, `vfx-burst`, `vfx-num`, `vfx-reveal-pop`…).
//!
//! No extra crate: one [`Tween`] component interpolating a [`TweenState`]
//! (offset, scale, rotation, alpha) with an [`EaseFunction`], plus a
//! [`Lifetime`] for the entities that only need to disappear.
//!
//! The tween writes to [`UiTransform`] and to every colour component of the
//! entity **and of its descendants** (so a faded panel takes its text and its
//! art with it), scaling their spawn-time alpha rather than replacing it — the
//! base alphas are captured on the first tick.
//!
//! Everything is time-driven through `Res<Time>`, so a head-less `App` can run
//! a whole animation by advancing the clock (see the tests).

use bevy::prelude::*;

/// What a tween interpolates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TweenState {
    /// Offset from the node's laid-out position, in logical pixels.
    pub offset: Vec2,
    pub scale: Vec2,
    /// Clockwise rotation, in radians.
    pub rotation: f32,
    /// Multiplier applied to the spawn-time alpha of every colour.
    pub alpha: f32,
}

impl Default for TweenState {
    fn default() -> Self {
        TweenState::IDENTITY
    }
}

impl TweenState {
    pub const IDENTITY: TweenState = TweenState {
        offset: Vec2::ZERO,
        scale: Vec2::ONE,
        rotation: 0.0,
        alpha: 1.0,
    };

    /// Identity, moved by `offset`.
    pub const fn offset(offset: Vec2) -> Self {
        TweenState {
            offset,
            ..TweenState::IDENTITY
        }
    }

    /// Identity, scaled uniformly.
    pub const fn scale(scale: f32) -> Self {
        TweenState {
            scale: Vec2::splat(scale),
            ..TweenState::IDENTITY
        }
    }

    /// Identity, at `alpha`.
    pub const fn alpha(alpha: f32) -> Self {
        TweenState {
            alpha,
            ..TweenState::IDENTITY
        }
    }

    pub const fn with_scale(mut self, scale: Vec2) -> Self {
        self.scale = scale;
        self
    }

    pub const fn with_uniform_scale(mut self, scale: f32) -> Self {
        self.scale = Vec2::splat(scale);
        self
    }

    pub const fn with_rotation(mut self, radians: f32) -> Self {
        self.rotation = radians;
        self
    }

    pub const fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Component-wise interpolation.
    pub fn lerp(self, other: TweenState, t: f32) -> TweenState {
        TweenState {
            offset: self.offset.lerp(other.offset, t),
            scale: self.scale.lerp(other.scale, t),
            rotation: self.rotation.lerp(other.rotation, t),
            alpha: self.alpha.lerp(other.alpha, t),
        }
    }
}

/// What happens to the entity once the tween is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweenEnd {
    /// Keep the final state and the component.
    Keep,
    /// Keep the final state, drop the [`Tween`].
    Remove,
    /// Despawn the entity (and its children).
    Despawn,
}

/// Alphas an entity had before the tween touched it.
#[derive(Debug, Clone, Copy, Default)]
struct Bases {
    background: Option<f32>,
    text: Option<f32>,
    image: Option<f32>,
    border: Option<f32>,
}

/// One animation.
#[derive(Component, Debug, Clone)]
pub struct Tween {
    /// Runs once `delay` is over.
    pub timer: Timer,
    /// Optional wait before the tween starts (the `from` state is held).
    pub delay: Timer,
    pub ease: EaseFunction,
    pub from: TweenState,
    pub to: TweenState,
    pub on_end: TweenEnd,
}

/// Alphas the tweened entity and its descendants had **before** any tween
/// touched them. Captured once, then reused by every later tween of that
/// entity — a pop-in that ends at `alpha = 0` must not become the new base of
/// the fly-out that follows it.
#[derive(Component, Debug, Clone)]
pub struct TweenBases(Vec<(Entity, Bases)>);

/// A `Once` timer that is already over when the delay is zero — otherwise the
/// first tick would be spent waiting for nothing.
fn delay_timer(duration: core::time::Duration) -> Timer {
    let mut timer = Timer::new(duration, TimerMode::Once);
    if duration.is_zero() {
        timer.finish();
    }
    timer
}

impl Tween {
    /// A tween running for `duration`, linear-out cubic by default.
    pub fn new(duration: core::time::Duration, from: TweenState, to: TweenState) -> Self {
        Tween {
            timer: Timer::new(
                duration.max(core::time::Duration::from_millis(1)),
                TimerMode::Once,
            ),
            delay: delay_timer(core::time::Duration::ZERO),
            ease: EaseFunction::CubicOut,
            from,
            to,
            on_end: TweenEnd::Remove,
        }
    }

    /// Same, in seconds.
    pub fn secs(duration: f32, from: TweenState, to: TweenState) -> Self {
        Tween::new(
            core::time::Duration::from_secs_f32(duration.max(0.001)),
            from,
            to,
        )
    }

    pub fn with_ease(mut self, ease: EaseFunction) -> Self {
        self.ease = ease;
        self
    }

    pub fn with_delay(mut self, delay: core::time::Duration) -> Self {
        self.delay = delay_timer(delay);
        self
    }

    /// Despawn the entity when the tween ends.
    pub fn despawning(mut self) -> Self {
        self.on_end = TweenEnd::Despawn;
        self
    }

    pub fn keeping(mut self) -> Self {
        self.on_end = TweenEnd::Keep;
        self
    }

    /// Eased progress in `0..=1`.
    pub fn progress(&self) -> f32 {
        if !self.delay.is_finished() {
            return 0.0;
        }
        EasingCurve::new(0.0f32, 1.0, self.ease).sample_clamped(self.timer.fraction())
    }

    /// The state to apply right now.
    pub fn state(&self) -> TweenState {
        self.from.lerp(self.to, self.progress())
    }
}

/// Despawn the entity when the timer runs out (for the static bursts).
#[derive(Component, Debug, Clone)]
pub struct Lifetime(pub Timer);

impl Lifetime {
    // Constructors for the general "despawn after N" component. Every effect
    // the render pass spawns today ends through its own `Tween::despawning()`,
    // so only the `tick_lifetimes` test builds one.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(duration: core::time::Duration) -> Self {
        Lifetime(Timer::new(duration, TimerMode::Once))
    }

    // Seconds-flavoured `new`; same story.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn secs(duration: f32) -> Self {
        Lifetime::new(core::time::Duration::from_secs_f32(duration.max(0.001)))
    }
}

// ============================================================
// Systems
// ============================================================

/// The four colour components a tween may fade, queried together.
///
/// Named so `tick_tweens` and [`collect_bases`] stay under
/// `clippy::type_complexity`.
type ColorQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static mut BackgroundColor>,
        Option<&'static mut TextColor>,
        Option<&'static mut ImageNode>,
        Option<&'static mut BorderColor>,
    ),
>;

/// Advance every [`Tween`] and write the result to the UI transform / colours.
pub fn tick_tweens(
    time: Res<Time>,
    mut commands: Commands,
    mut tweens: Query<(
        Entity,
        &mut Tween,
        Option<&mut UiTransform>,
        Option<&TweenBases>,
    )>,
    children: Query<&Children>,
    mut colors: ColorQuery,
) {
    let delta = time.delta();
    for (entity, mut tween, transform, bases) in &mut tweens {
        let bases = match bases {
            Some(bases) => bases.0.clone(),
            None => {
                let mut collected = Vec::new();
                collect_bases(entity, &children, &colors, &mut collected);
                commands
                    .entity(entity)
                    .try_insert(TweenBases(collected.clone()));
                collected
            }
        };

        if !tween.delay.is_finished() {
            tween.delay.tick(delta);
        } else {
            tween.timer.tick(delta);
        }

        let state = tween.state();
        if let Some(mut transform) = transform {
            transform.translation = Val2::px(state.offset.x, state.offset.y);
            transform.scale = state.scale;
            transform.rotation = Rot2::radians(state.rotation);
        }
        for (target, bases) in bases {
            let Ok((background, text, image, border)) = colors.get_mut(target) else {
                continue;
            };
            if let (Some(mut background), Some(base)) = (background, bases.background) {
                background.0.set_alpha(base * state.alpha);
            }
            if let (Some(mut text), Some(base)) = (text, bases.text) {
                text.0.set_alpha(base * state.alpha);
            }
            if let (Some(mut image), Some(base)) = (image, bases.image) {
                image.color.set_alpha(base * state.alpha);
            }
            if let (Some(mut border), Some(base)) = (border, bases.border) {
                let mut color = border.top;
                color.set_alpha(base * state.alpha);
                *border = BorderColor::all(color);
            }
        }

        if tween.delay.is_finished() && tween.timer.is_finished() {
            match tween.on_end {
                TweenEnd::Keep => {}
                TweenEnd::Remove => {
                    commands.entity(entity).try_remove::<Tween>();
                }
                TweenEnd::Despawn => {
                    commands.entity(entity).try_despawn();
                }
            }
        }
    }
}

fn collect_bases(
    entity: Entity,
    children: &Query<&Children>,
    colors: &ColorQuery,
    out: &mut Vec<(Entity, Bases)>,
) {
    if let Ok((background, text, image, border)) = colors.get(entity) {
        out.push((
            entity,
            Bases {
                background: background.map(|c| c.0.alpha()),
                text: text.map(|c| c.0.alpha()),
                image: image.map(|c| c.color.alpha()),
                border: border.map(|c| c.top.alpha()),
            },
        ));
    }
    if let Ok(kids) = children.get(entity) {
        for child in kids.iter() {
            collect_bases(child, children, colors, out);
        }
    }
}

/// Despawn whatever ran out of [`Lifetime`].
pub fn tick_lifetimes(
    time: Res<Time>,
    mut commands: Commands,
    mut lifetimes: Query<(Entity, &mut Lifetime)>,
) {
    for (entity, mut lifetime) in &mut lifetimes {
        if lifetime.0.tick(time.delta()).is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_systems(Update, (tick_tweens, tick_lifetimes));
        app
    }

    fn advance(app: &mut App, millis: u64) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(millis));
        app.update();
    }

    #[test]
    fn interpolation_is_component_wise() {
        let from = TweenState::IDENTITY;
        let to = TweenState::offset(Vec2::new(100.0, -50.0))
            .with_uniform_scale(2.0)
            .with_alpha(0.0);
        let mid = from.lerp(to, 0.5);
        assert_eq!(mid.offset, Vec2::new(50.0, -25.0));
        assert_eq!(mid.scale, Vec2::splat(1.5));
        assert_eq!(mid.alpha, 0.5);
    }

    #[test]
    fn a_delay_holds_the_starting_state() {
        let mut tween = Tween::secs(1.0, TweenState::alpha(1.0), TweenState::alpha(0.0))
            .with_delay(Duration::from_millis(500));
        assert_eq!(tween.progress(), 0.0);
        tween.delay.tick(Duration::from_millis(400));
        assert_eq!(tween.state().alpha, 1.0, "still waiting");
        tween.delay.tick(Duration::from_millis(200));
        tween.timer.tick(Duration::from_millis(1000));
        assert_eq!(tween.state().alpha, 0.0);
    }

    #[test]
    fn a_tween_moves_the_ui_transform_and_fades_its_children() {
        let mut app = app();
        let child = app
            .world_mut()
            .spawn((Node::default(), TextColor(Color::WHITE)))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Node::default(),
                UiTransform::default(),
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                Tween::secs(
                    1.0,
                    TweenState::IDENTITY,
                    TweenState::offset(Vec2::new(0.0, -20.0)).with_alpha(0.0),
                )
                .with_ease(EaseFunction::Linear)
                .despawning(),
            ))
            .id();
        app.world_mut().entity_mut(root).add_child(child);

        advance(&mut app, 500);
        let world = app.world();
        let transform = world.entity(root).get::<UiTransform>().unwrap();
        assert_eq!(transform.translation, Val2::px(0.0, -10.0));
        let background = world.entity(root).get::<BackgroundColor>().unwrap();
        assert!(
            (background.0.alpha() - 0.25).abs() < 0.01,
            "half of the base alpha"
        );
        let text = world.entity(child).get::<TextColor>().unwrap();
        assert!((text.0.alpha() - 0.5).abs() < 0.01, "children fade too");

        advance(&mut app, 600);
        assert!(
            app.world().get_entity(root).is_err(),
            "a despawning tween cleans up after itself"
        );
    }

    #[test]
    fn a_lifetime_despawns_on_time() {
        let mut app = app();
        let entity = app
            .world_mut()
            .spawn((Node::default(), Lifetime::secs(0.5)))
            .id();
        advance(&mut app, 300);
        assert!(app.world().get_entity(entity).is_ok());
        advance(&mut app, 300);
        assert!(app.world().get_entity(entity).is_err());
    }
}
