# Bevy 0.19.1 API cheat sheet (verified against crate source)

Every item below was read from the crate sources in
`/root/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (paths cited as `crate/src/file.rs:line`).
Only APIs reachable with the game crate's feature set are listed
(`rust/crates/game/Cargo.toml`: `std, async_executor, multi_threaded, bevy_asset, bevy_log, bevy_state,
reflect_auto_register, scene, bevy_winit, x11, custom_cursor, default_font, 2d_bevy_render, ui_bevy_render,
bevy_ui_widgets, picking, bevy_picking, sprite_picking, ui_picking, png, jpeg`).

Feature graph consequences (bevy-0.19.1/Cargo.toml, bevy_internal-0.19.1/Cargo.toml):
- `2d_bevy_render` = `2d_api + bevy_render + bevy_core_pipeline + bevy_post_process + bevy_sprite_render + bevy_gizmos_render`
  -> sprites, Text2d, Mesh2d/Material2d/ColorMaterial, **Gizmos are available**.
- `ui_bevy_render` = `ui_api` + `bevy_render` + `bevy_core_pipeline` + `bevy_ui_render`, and
  `ui_api = ["default_app", "common_api", "bevy_input_focus", "bevy_ui"]` (bevy-0.19.1/Cargo.toml:2893).
  So `ui_api` (like `2d_api`) pulls the whole `common_api` closure — hand-picking features instead of `default_app`
  does **not** narrow it: `common_api = ["bevy_animation", "bevy_camera", "bevy_color", "bevy_gizmos", "bevy_image",
  "bevy_mesh", "bevy_shader", "bevy_material", "bevy_text", "bevy_window", "hdr", "png"]` (:2722), and
  `default_app = ["async_executor", "bevy_asset", "bevy_log", "bevy_state", "reflect_auto_register"]` (:2748).
- `picking` = `bevy_picking + mesh_picking + sprite_picking + ui_picking`.
- `bevy_sprite_render` pulls `bevy_material`; `bevy_render` pulls `bevy_camera`, `bevy_shader`.
- `bevy_math/curve` is enabled transitively by `bevy_color` (bevy_color-0.19.1/Cargo.toml:77) -> `EasingCurve`/`EaseFunction` are usable.
- **`bevy_animation` IS enabled** (transitively): `2d_bevy_render` -> `2d_api` (bevy-0.19.1/Cargo.toml:2597, :2593)
  -> `common_api` (:2722, first entry `bevy_animation`) -> `bevy_animation = ["bevy_internal/bevy_animation"]` (:2659)
  -> bevy_internal's `bevy_animation = ["dep:bevy_animation", "bevy_mesh"]`. `ui_api` reaches it the same way.
  So `bevy::animation` exists (bevy_internal-0.19.1/src/lib.rs:20-21) and `AnimationPlugin` is in `DefaultPlugins`
  (bevy_internal-0.19.1/src/default_plugins.rs:82-83); `bevy_animation-0.19.1` is present in the vendored registry.
- `scene` = `["bevy_world_serialization", "bevy_scene"]` (bevy-0.19.1/Cargo.toml:2840), so `bevy::world_serialization`
  and `WorldSerializationPlugin` are also enabled.
- Not enabled: audio, gilrs, pbr/3D, gltf. Do not use `bevy::audio`, `bevy::pbr`, `bevy::gltf`.

Facade module names (bevy_internal-0.19.1/src/lib.rs:15-112) available with this feature set:
`bevy::a11y` (:17-18, cfg `bevy_window`), `animation` (:20-21), `app`, `asset`, `camera`, `color`, `core_pipeline`,
`diagnostic` (:41), `ecs`, `gizmos`, `gizmos_render`, `image`, `input`, `input_focus`, `log`, `material`, `math`, `mesh`,
`picking`, `platform` (:70), `ptr` (:72), `reflect` (:73), `render`, `scene`, `shader`, `sprite`, `sprite_render`,
`state`, `tasks` (:92), `text`, `time`, `transform`, `ui`, `ui_render`, `ui_widgets`, `utils` (:103), `window`, `winit`,
`world_serialization` (:111-112, cfg `bevy_world_serialization`, enabled via `scene`).
(`app, diagnostic, ecs, input, math, platform, ptr, reflect, tasks, time, transform, utils` are unconditional re-exports.)

`bevy::prelude::*` re-exports **most, but not all**, enabled crates' preludes (bevy_internal-0.19.1/src/prelude.rs).
Notable holes to import explicitly:
- **`bevy_tasks` has no prelude line** (prelude.rs re-exports app/ecs/input/math/platform/reflect/time/transform/utils
  plus per-feature crate preludes; there is no `tasks::prelude::*`). `AsyncComputeTaskPool`, `ComputeTaskPool`,
  `IoTaskPool`, `block_on`, `ParallelIterator` need `use bevy::tasks::{..};`.
- **`bevy_ui_widgets` and `bevy_input_focus` have no `prelude` module at all** (`grep -c 'pub mod prelude'` -> 0 in
  both `bevy_ui_widgets-0.19.1/src/lib.rs` and `bevy_input_focus-0.19.1/src/lib.rs`), so nothing from
  `bevy::ui_widgets` / `bevy::input_focus` (`Activate`, `ValueChange`, `observe`, `InputFocus`, `FocusedInput`) is
  in `bevy::prelude` — always use an explicit path/import.

---

## 1. App, plugins, Window, States, schedules, ECS basics

### App / Plugin / DefaultPlugins
- `bevy::app::App` (prelude). `bevy_app-0.19.1/src/app.rs`
  - `pub fn new() -> App` (:139), `pub fn run(&mut self) -> AppExit` (:185)
  - `pub fn add_plugins<M>(&mut self, plugins: impl Plugins<M>) -> &mut Self` (:655)
  - `pub fn add_systems<M>(&mut self, schedule: impl ScheduleLabel, systems: impl IntoScheduleConfigs<..>) -> &mut Self` (:321)
  - `pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self` (:451), `pub fn init_resource<R: Resource + FromWorld>(&mut self) -> &mut Self` (:485)  (chainable)
  - `pub fn add_message<M: Message>(&mut self) -> &mut Self` (:427)  **(was `add_event` in <=0.16)**
  - `pub fn add_observer<M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self` (:1474)
  - `pub fn configure_sets<M>(..)` (:402), `pub fn register_type<T>(..)` (:677)
- `pub trait Plugin: Downcast + Any + Send + Sync { fn build(&self, app: &mut App); fn ready(&self, _app: &App) -> bool {..} ... }` `bevy_app-0.19.1/src/plugin.rs:57`
- `pub enum AppExit { #[default] Success, Error(NonZero<u8>) }` `bevy_app-0.19.1/src/app.rs:1560`
- `PluginGroup` (`bevy_app-0.19.1/src/plugin_group.rs`): `fn set<T: Plugin>(self, plugin: T) -> PluginGroupBuilder` (:211);
  `PluginGroupBuilder::{set, add, add_before::<Target>, add_after::<Target>, disable::<T>}` (:312-:501).
- `DefaultPlugins` (`bevy_internal-0.19.1/src/default_plugins.rs:5`) with this feature set includes, in order:
  PanicHandlerPlugin, LogPlugin, TaskPoolPlugin, FrameCountPlugin, TimePlugin, TransformPlugin, DiagnosticsPlugin, InputPlugin,
  InputFocusPlugin, InputDispatchPlugin, WindowPlugin, AccessibilityPlugin, TerminalCtrlCHandlerPlugin, AssetPlugin,
  **`bevy_world_serialization::WorldSerializationPlugin`** (:33-34, from the `scene` feature), ScenePlugin (:35-36),
  WinitPlugin, RenderPlugin, **`bevy_image::ImagePlugin`**, MeshPlugin, CameraPlugin, PipelinedRenderingPlugin, CorePipelinePlugin,
  PostProcessPlugin, SpritePlugin, SpriteRenderPlugin, TextPlugin, UiPlugin, UiRenderPlugin,
  **`bevy_animation::AnimationPlugin`** (:82-83, from `common_api`), GizmoPlugin (:84-85), GizmoRenderPlugin,
  `bevy_state::app::StatesPlugin`, `bevy_ui_widgets::UiWidgetsPlugins`, `bevy_picking::DefaultPickingPlugins`.
  Gotcha: `ImagePlugin` now lives in `bevy::image` (prelude), not `bevy::render`.

### Window / WindowPlugin
`bevy_window-0.19.1/src/lib.rs:64`
```rust
pub struct WindowPlugin {
    pub primary_window: Option<Window>,          // default Some(Window::default())
    pub primary_cursor_options: Option<CursorOptions>, // default Some(CursorOptions::default())
    pub exit_condition: ExitCondition,           // OnPrimaryClosed | OnAllClosed | DontExit
    pub close_when_requested: bool,
}
```
`Window` component (`bevy_window-0.19.1/src/window.rs:164`), relevant fields:
```rust
pub struct Window {
    pub present_mode: PresentMode,        // AutoVsync, AutoNoVsync, #[default] Fifo, FifoRelaxed, Immediate, Mailbox (:1219)
    pub mode: WindowMode,                 // Windowed, BorderlessFullscreen(MonitorSelection), Fullscreen(MonitorSelection, VideoModeSelection) (:1338)
    pub position: WindowPosition,
    pub resolution: WindowResolution,
    pub title: String,
    pub name: Option<String>,
    pub resizable: bool, pub decorations: bool, pub transparent: bool, pub focused: bool,
    pub visible: bool, pub resize_constraints: WindowResizeConstraints, ...
}
```
- `WindowResolution` (:895) has private fields; construct with
  `pub fn new(physical_width: u32, physical_height: u32) -> Self` (:923) **(u32 now, not f32)**,
  `.with_scale_factor_override(f32)` (:932); `From<(u32,u32)>`, `From<[u32;2]>`, `From<UVec2>` (:1044-:1056).
  Read: `.width() -> f32`, `.height() -> f32`, `.size() -> Vec2`, `.physical_size() -> UVec2`, `.scale_factor() -> f32`.
- `Window` methods: `width()`, `height()`, `size() -> Vec2`, `physical_size() -> UVec2`, `scale_factor() -> f32`,
  `cursor_position(&self) -> Option<Vec2>` (:620, logical px, origin top-left), `physical_cursor_position()`, `set_cursor_position(Option<Vec2>)`.
- `PrimaryWindow` marker component (window.rs:56). **NOT in the prelude** — `bevy_window`'s prelude is only
  `{CursorEntered, CursorLeft, CursorMoved, FileDragAndDrop, Ime, MonitorSelection, VideoModeSelection, Window,
  WindowMoved, WindowPlugin, WindowPosition, WindowResizeConstraints}` (`bevy_window-0.19.1/src/lib.rs:38-45`).
  Write `use bevy::window::PrimaryWindow;` (reachable via `pub use window::*;` at lib.rs:33). `CursorOptions { visible: bool, grab_mode: CursorGrabMode /*None|Confined|Locked*/, hit_test: bool }` (:752) is a **separate component** now.
- Cursor icon: `bevy::window::CursorIcon` component `enum CursorIcon { Custom(CustomCursor), System(SystemCursorIcon) }` (`bevy_window-0.19.1/src/cursor/mod.rs:27`);
  `SystemCursorIcon::{Default, Pointer, Grab, Grabbing, NotAllowed, Move, Text, Crosshair, Wait, ...}`. Insert on the window entity.
- Window messages (`bevy_window-0.19.1/src/event.rs`, all `#[derive(Message)]`): `WindowResized { window: Entity, width: f32, height: f32 }` (:39),
  `CursorMoved { window: Entity, position: Vec2, delta: Option<Vec2> }` (:192), `CursorEntered`, `CursorLeft`, `WindowCloseRequested`.
- `WinitSettings { focused_mode: UpdateMode, unfocused_mode: UpdateMode }` resource, `WinitSettings::game()/desktop_app()` (`bevy_winit-0.19.1/src/winit_config.rs:6-:31`); `UpdateMode::{Continuous, Reactive{..}}`.

```rust
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "TCGOP".into(),
                resolution: WindowResolution::new(1600, 900),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }).set(ImagePlugin::default_nearest()))
        .init_state::<GameState>()
        .add_systems(Startup, setup)
        .add_systems(Update, tick.run_if(in_state(GameState::InGame)))
        .add_systems(FixedUpdate, physics)
        .run();
}
```

### States (bevy_state)
`bevy_state-0.19.1/src/state/states.rs:64`: `pub trait States: 'static + Send + Sync + Clone + PartialEq + Eq + Hash + Debug { const DEPENDENCY_DEPTH: usize = 1; }`
Derive: `#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)] enum GameState { #[default] MainMenu, InGame }` (doc example :28).
- `App` ext (`bevy_state-0.19.1/src/app.rs:24`, trait `AppExtStates` in prelude): `init_state::<S>()`, `insert_state(S)`, `add_sub_state::<S>()`, `add_computed_state::<S>()`.
- Schedules (`bevy_state-0.19.1/src/state/transitions.rs`): `pub struct OnEnter<S: States>(pub S)` (:19), `pub struct OnExit<S: States>(pub S)` (:25),
  `pub struct OnTransition<S> { pub exited: S, pub entered: S }` (:34), `StateTransition` schedule (:61),
  `StateTransitionEvent<S: States> { pub exited: Option<S>, pub entered: Option<S>, pub allow_same_state_transitions: bool }` (:68-75) (a Message)
  — **three** public fields; the third gates identity transitions, so struct literals / exhaustive destructuring must include it.
- Conditions (`bevy_state-0.19.1/src/condition.rs`): `pub fn in_state<S: States>(state: S) -> impl FnMut(Option<Res<State<S>>>) -> bool + Clone` (:103), `state_exists::<S>`, `state_changed::<S>`.
- Resources (`bevy_state-0.19.1/src/state/resources.rs`): `State<S>` with `.get(&self) -> &S` (:69);
  `pub enum NextState<S: FreelyMutableState> { #[default] Unchanged, Pending(S), PendingIfNeq(S) }` (:181-190)
  — **three** variants; `PendingIfNeq(S)` skips the transition schedules when the target equals the current state,
  so an exhaustive `match` must cover it. `.set(state)` (:198) -> `Pending`, `.set_if_neq(state)` (:206) -> `PendingIfNeq`;
  from `Commands`, `set_state(s)` / `set_state_if_neq(s)` (`bevy_state-0.19.1/src/commands.rs:24, :38`).
- `CommandsStatesExt::set_state<S: FreelyMutableState>(&mut self, state: S)` on `Commands` (`bevy_state-0.19.1/src/commands.rs:7`).
- State-scoped entities (`bevy_state-0.19.1/src/state_scoped.rs`): `pub struct DespawnOnExit<S: States>(pub S)` (:149), `DespawnOnEnter<S>(pub S)` (:230), plus `DisableOnExit/EnableOnEnter/...` (prelude).
  **`StateScoped` was renamed to `DespawnOnExit`.**

```rust
fn go_ingame(mut next: ResMut<NextState<GameState>>) { next.set(GameState::InGame); }
app.add_systems(OnEnter(GameState::InGame), spawn_board)
   .add_systems(OnExit(GameState::InGame), cleanup);
commands.spawn((Sprite::default(), DespawnOnExit(GameState::InGame)));
```

### Schedules (bevy_app-0.19.1/src/main_schedule.rs)
`PreStartup`(:63) `Startup`(:69) `PostStartup`(:75) `First`(:81) `PreUpdate`(:92) `RunFixedMainLoop`(:104)
`FixedFirst`(:111) `FixedPreUpdate`(:118) `FixedUpdate`(:133) `FixedPostUpdate`(:141) `FixedLast`(:148) `Update`(:173) `PostUpdate`(:190) `Last`(:196).
Fixed timestep default: `Duration::from_micros(15625)` = 64 Hz (`bevy_time-0.19.1/src/fixed.rs:74-76`).
**`DEFAULT_TIMESTEP` is a private associated const** (`const DEFAULT_TIMESTEP: Duration = ...`, no `pub`), so naming
`Time::<Fixed>::DEFAULT_TIMESTEP` in game code is E0624. Only the resulting default is observable: read it back with
`time.timestep()` , or set your own with `Time::<Fixed>::from_hz(60.0)` / `from_duration(..)` / `set_timestep(..)`.

### ECS basics (bevy_ecs-0.19.1)
- Prelude (`src/lib.rs`) includes: `Component, Resource, Entity, Commands, EntityCommands, Query, Single, Populated, Res, ResMut, Local, With, Without, Added, Changed, Has, Or,
  ChildOf, Children, children!, Name, On, Observer, Event, EntityEvent, Message, MessageReader, MessageWriter, Messages, Add, Insert, Remove, Despawn, RemovedComponents,
  IntoScheduleConfigs, SystemSet, common_conditions::*, DetectChanges, DetectChangesMut, Mut, Ref, BevyError, Result`.
- `#[derive(Resource)] struct Score(u32);` — `pub trait Resource: Component {}` (`src/resource.rs:87`; resources are components internally, still used via `Res`/`ResMut`).
- `Commands` (`src/system/commands/mod.rs`): `spawn(bundle) -> EntityCommands` (:398), `spawn_empty()` (:346), `entity(Entity) -> EntityCommands` (:439),
  `get_entity(Entity) -> Result<EntityCommands, InvalidEntityError>` (:490), `spawn_batch(iter)` (:587), `insert_resource(R)` (:895), `remove_resource::<R>()` (:932),
  `trigger(event)` (:1169), `write_message::<M>(m)` (:1217), `add_observer(observer) -> EntityCommands` (:1200), `queue(cmd)` (:641).
- `EntityCommands`: `id() -> Entity` (:1324), `insert(bundle)` (:1435), `insert_if_new` (:1483), `try_insert` (:1626), `remove::<B>()` (:1726), `despawn()` (:1906), `try_despawn()` (:1921),
  `observe(observer)` (:2071), `trigger(|entity| Event{..})` (:2358), `entry::<T>()` (:1381),
  hierarchy (`src/hierarchy.rs`): `with_children(|p: &mut ChildSpawnerCommands| ..)` (:383), `with_child(bundle)` (:484), `add_child(Entity)` (:423), `add_children(&[Entity])` (:392),
  `remove_children`, `replace_children`, `despawn_children()` (`src/relationship/related_methods.rs:529`), `despawn_related::<Children>()`.
  **`despawn()` is recursive by default** (Children is `linked_spawn`, `src/hierarchy.rs:148`); `despawn_recursive` no longer exists.
- `Query` (`src/system/query.rs`): `iter()`, `iter_mut()`, `get(Entity) -> Result<_, QueryEntityError>` (:1577), `get_mut` (:1713),
  `single() -> Result<_, QuerySingleError>` (:2097), `single_mut()` (:2126), `is_empty()`, `contains(Entity)`, `count()`, `iter_many(entities)`.
  **`single()` returns `Result`, not panicking `get_single`**; use `Single<D, F>` system param (:2850) when exactly one match is expected.
- Run conditions (`src/schedule/condition.rs`): `run_once`, `resource_exists::<T>`, `resource_equals(v)`, `resource_changed::<T>`, `on_message::<M>` (:1136), `any_with_component::<T>`, `not(cond)`.
  Input: `bevy::input::common_conditions::{input_pressed, input_just_pressed}` (`bevy_input-0.19.1/src/common_conditions.rs:66,:88`).
- Scheduling: `(a, b).chain()`, `.run_if(cond)`, `.after(x)`, `.before(x)`, `.in_set(S)` on `IntoScheduleConfigs`.

```rust
#[derive(Component)] struct Hp(i32);
#[derive(Resource, Default)] struct Score(u32);
fn setup(mut commands: Commands) { commands.spawn((Name::new("p"), Hp(10))); }
fn tick(mut q: Query<(Entity, &mut Hp)>, mut score: ResMut<Score>, mut commands: Commands) {
    for (e, mut hp) in &mut q { hp.0 -= 1; if hp.0 <= 0 { commands.entity(e).despawn(); score.0 += 1; } }
}
```

---

## 2. Camera2d, Transform, Sprite, Text2d, TextFont/FontSize, assets

### Camera
- `bevy::camera::Camera2d` (prelude), `bevy_camera-0.19.1/src/components.rs:16`:
  `#[require(Camera, Projection::Orthographic(OrthographicProjection::default_2d()), Frustum = ...)] pub struct Camera2d;`
  Spawn: `commands.spawn(Camera2d);` (add `Transform`, `IsDefaultUiCamera` etc. as needed).
- `Camera` (`bevy_camera-0.19.1/src/camera.rs:384`): `viewport: Option<Viewport>, order: isize, is_active: bool, clear_color: ClearColorConfig, msaa_writeback, output_mode, ...`.
  - `pub fn viewport_to_world_2d(&self, camera_transform: &GlobalTransform, viewport_position: Vec2) -> Result<Vec2, ViewportConversionError>` (:709)
  - `pub fn world_to_viewport(&self, camera_transform: &GlobalTransform, world_position: Vec3) -> Result<Vec2, ViewportConversionError>` (:579)
  - `pub fn viewport_to_world(&self, ..) -> Result<Ray3d, _>` (:647), `logical_viewport_size() -> Option<Vec2>` (:479).
- `ClearColor(pub Color)` resource (`bevy_camera-0.19.1/src/clear_color.rs:55`, default `Color::srgb_u8(43,44,47)`); `ClearColorConfig::{Default, Custom(Color), None}` (:13).
- `Projection::Orthographic(OrthographicProjection)` (`bevy_camera-0.19.1/src/projection.rs:216`); `OrthographicProjection::default_2d()` (:771);
  `scaling_mode: ScalingMode::{WindowSize, Fixed{width,height}, AutoMin{min_width,min_height}, AutoMax{..}, FixedVertical{viewport_height}, FixedHorizontal{viewport_width}}` (:523-555), fields `scale: f32, near, far, viewport_origin: Vec2, area: Rect`.
- `Visibility::{#[default] Inherited, Hidden, Visible}` (`bevy_camera-0.19.1/src/visibility/mod.rs:83`); `InheritedVisibility`, `ViewVisibility` in prelude.

```rust
use bevy::window::PrimaryWindow;   // NOT in bevy::prelude

fn cursor_world(window: Single<&Window, With<PrimaryWindow>>, cam: Single<(&Camera, &GlobalTransform), With<Camera2d>>) -> Option<Vec2> {
    let (camera, cam_tf) = *cam;
    let p = window.cursor_position()?;
    camera.viewport_to_world_2d(cam_tf, p).ok()
}
```

### Transform / GlobalTransform (`bevy_transform-0.19.1/src/components/`)
```rust
pub struct Transform { pub translation: Vec3, pub rotation: Quat, pub scale: Vec3 }   // transform.rs:86
```
`Transform::IDENTITY`, `from_xyz(x,y,z)` (:119), `from_translation(Vec3)` (:139), `from_rotation(Quat)` (:149), `from_scale(Vec3)` (:159),
`with_translation/with_rotation/with_scale` (:242-:258), `rotate(Quat)` (:344), `rotate_z(f32)` (:389), `looking_at(Vec3, up)` (:187), `to_isometry()` (:614).
`pub struct GlobalTransform(Affine3A)` (global_transform.rs:60): `translation() -> Vec3` (:210), `rotation() -> Quat` (:229), `scale() -> Vec3` (:240), `compute_transform() -> Transform` (:129), `to_scale_rotation_translation()` (:200).
Z ordering for 2D: larger `translation.z` draws on top. `Sprite`/`Text2d`/`Mesh2d` all `#[require(Transform)]`.

### Sprite (`bevy_sprite-0.19.1/src/sprite.rs:19`)
```rust
#[derive(Component, Debug, Default, Clone, Reflect, FromTemplate)]
#[require(Transform, Visibility, VisibilityClass, Anchor)]
pub struct Sprite {
    pub image: Handle<Image>,
    pub texture_atlas: Option<TextureAtlas>,
    pub color: Color,                 // tint, default WHITE
    pub flip_x: bool,
    pub flip_y: bool,
    pub custom_size: Option<Vec2>,
    pub rect: Option<Rect>,           // sub-rect of the image
    pub image_mode: SpriteImageMode,
}
```
Constructors: `Sprite::from_image(Handle<Image>)` (:54), `Sprite::sized(Vec2)` (:46), `Sprite::from_atlas_image(image, TextureAtlas)` (:62), `Sprite::from_color(impl Into<Color>, Vec2)` (:71).
**`Anchor` is now a separate required component** (`pub struct Anchor(pub Vec2)` :257, constants `Anchor::CENTER, TOP_LEFT, BOTTOM_LEFT, CENTER_LEFT, TOP_CENTER, ...` :267-:275). It is NOT in the prelude: `use bevy::sprite::Anchor;`.
```rust
pub enum SpriteImageMode {          // sprite.rs:168
    #[default] Auto,
    Scale(SpriteScalingMode),       // FillCenter | FillStart | FillEnd | FitCenter | FitStart | FitEnd  (sprite.rs:216)
    Sliced(TextureSlicer),
    Tiled { tile_x: bool, tile_y: bool, stretch_value: f32 },
}
pub struct TextureSlicer { pub border: BorderRect, pub center_scale_mode: SliceScaleMode, pub sides_scale_mode: SliceScaleMode, pub max_corner_scale: f32 } // texture_slice/slicer.rs:15
pub enum SliceScaleMode { #[default] Stretch, Tile { stretch_value: f32 } }                    // slicer.rs:29
pub struct BorderRect { pub min_inset: Vec2, pub max_inset: Vec2 }  // BorderRect::ZERO, ::all(f32), ::axes(h, v)  (texture_slice/border_rect.rs:10)
```
```rust
commands.spawn((
    Sprite { image: assets.load("cards/op01-001.png"), custom_size: Some(Vec2::new(120., 168.)), ..default() },
    Anchor::BOTTOM_LEFT,
    Transform::from_xyz(-200., 0., 1.),
));
// 9-slice frame
Sprite { image: frame, custom_size: Some(Vec2::new(300., 80.)),
         image_mode: SpriteImageMode::Sliced(TextureSlicer { border: BorderRect::all(12.), ..default() }), ..default() }
// fade: needs `use bevy::color::Alpha` (in prelude via color_ops)
sprite.color.set_alpha(0.5);
```
Sprite picking marker: `SpritePickingCamera`, `SpritePickingSettings { require_markers: bool /*false*/, picking_mode: SpritePickingMode /*AlphaThreshold(0.1)*/ }` (`bevy_sprite-0.19.1/src/picking_backend.rs:53`).

### Text2d and text components (`bevy_sprite-0.19.1/src/text2d.rs`, `bevy_text-0.19.1/src/text.rs`)
```rust
#[require(TextLayout, TextFont, TextColor, LineHeight, LetterSpacing, TextBounds, Anchor, Visibility, VisibilityClass, Transform, FontHinting::Disabled)]
pub struct Text2d(pub String);                       // text2d.rs:102 ; Text2d::new(impl Into<String>)
pub struct TextFont {                                // text.rs:376
    pub font: FontSource,            // 15-variant enum, see below  (:282)
    pub font_size: FontSize,         // enum, NOT f32
    pub weight: FontWeight,          // FontWeight(pub u16); FontWeight::{THIN=100, ..., NORMAL=400, BOLD=700, ...}  (:597)
    pub width: FontWidth,
    pub style: FontStyle,            // Normal | Italic | Oblique(Option<f32>)  (:705)
    pub font_smoothing: FontSmoothing,   // None | #[default] AntiAliased  (:1183)
    pub font_features: FontFeatures,
    pub font_variations: FontVariations,
}
pub enum FontSource {                                                                // text.rs:282-340  (15 variants)
    #[default] Handle(Handle<Font>),   // a loaded `Font` asset
    Family(SmolStr),                   // resolve by family name from the font database
    // CSS-style generic families (0.19 feature — no asset needed):
    Serif, SansSerif, Cursive, Fantasy, Monospace,
    SystemUi, UiSerif, UiSansSerif, UiMonospace, UiRounded,
    Emoji, Math, FangSong,
}   // exhaustive matches must cover all 15; needs `parley/system` for system-font discovery
pub enum FontSize { Px(f32), Vw(f32), Vh(f32), VMin(f32), VMax(f32), Rem(f32) }   // text.rs:487 ; Default = Px(20.), From<f32> -> Px, `FontSize * f32`
pub struct TextColor(pub Color);                                                     // text.rs:1066
pub struct TextLayout { pub justify: Justify, pub linebreak: LineBreak }             // text.rs:133 ; TextLayout::new(j, lb), ::justify(j), ::linebreak(lb), ::no_wrap()
pub enum Justify { #[default] Left, Center, Right, Justified, Start, End }           // text.rs:233   (was JustifyText)
pub enum LineBreak { #[default] WordBoundary, AnyCharacter, WordOrCharacter, NoWrap } // text.rs:1114
pub enum LineHeight { Px(f32), RelativeToFont(f32) }  // component; default RelativeToFont(1.2)  (text.rs:1013)
pub enum LetterSpacing { Px(f32), Rem(f32) }          // component  (text.rs:1041)
pub struct TextSpan(pub String);  // child span; #[require(TextFont, TextColor, LineHeight, LetterSpacing)]  (text.rs:193)
pub struct TextBounds { pub width: Option<f32>, pub height: Option<f32> }             // bounds.rs:15
pub struct Text2dShadow { pub offset: Vec2, pub color: Color }                        // text2d.rs:143
```
`TextFont` builders (:413-:451): `TextFont::from_font_size(impl Into<FontSize>)`, `from_font_weight(..)`, `.with_font(Handle<Font>)`, `.with_family(&str)`, `.with_font_size(..)`, `.with_font_weight(..)`, `.with_font_smoothing(..)`;
`impl<T: Into<FontSource>> From<T> for TextFont` so `TextFont::from(handle)` works. `FontSource: From<Handle<Font>>, From<&Handle<Font>>, From<&str> (family)`.
Read/write text: `Text2dReader/Text2dWriter` = `TextReader/TextWriter<Text2d>` (`bevy_text-0.19.1/src/text_access.rs`): `writer.text(root, index) -> Mut<String>`, `writer.color(root, idx)`, `writer.font(root, idx)`.
Gotchas: `font_size: 24.0` no longer compiles -> `FontSize::Px(24.0)` or `24.0.into()`. `font: handle` -> `font: handle.into()` or `FontSource::Handle(handle)`. `JustifyText` -> `Justify`. Text2d uses the primary window's logical size for Vw/Vh.
```rust
commands.spawn((
    Text2d::new("Deck: 40"),
    TextFont { font: assets.load::<Font>("fonts/Pirata.ttf").into(), font_size: FontSize::Px(28.0), ..default() },
    TextColor(Color::WHITE),
    TextLayout::new(Justify::Center, LineBreak::WordBoundary),
    Anchor::TOP_CENTER,
    Transform::from_xyz(0., 300., 5.),
    children![(TextSpan::new(" (+2)"), TextColor(Color::srgb(0.2, 0.9, 0.2)))],
));
```

### Assets (`bevy_asset-0.19.1/src`)
- `AssetServer::load<'a, A: Asset>(&self, path: impl Into<AssetPath<'a>>) -> Handle<A>` (server/mod.rs:364);
  `load_with_settings<'a, A: Asset, S: Settings>(&self, path, settings: impl Fn(&mut S) + Send + Sync + 'static) -> Handle<A>` (:449);
  `add<A: Asset>(&self, asset: A) -> Handle<A>` (:1008); `load_state(id) -> LoadState` (:1291), `is_loaded_with_dependencies(id) -> bool` (:1331).
  `LoadState::{NotLoaded, Loading, Loaded, Failed(Arc<AssetLoadError>)}` (:2248).
- `pub enum Handle<A: Asset> { Strong(Arc<StrongHandle>), Uuid(Uuid, PhantomData<..>) }` (handle.rs:134). **No `Handle::Weak` any more**; `Handle::default()` = `Uuid(AssetId::DEFAULT_UUID)`. `handle.id() -> AssetId<A>` (:192), `.path()`, `.is_strong()`.
- `Assets<A>` resource (assets.rs): `add(impl Into<A>) -> Handle<A>` (:401), `get(id) -> Option<&A>` (:430), `get_mut(id) -> Option<AssetMut<A>>` (:440), `remove`, `contains`, `iter`.
- Loaders: images `png/jpeg` via `ImageLoader` (bevy_image), fonts `ttf/otf` (`bevy_text-0.19.1/src/font_loader.rs:38`), shaders `wgsl/spv/vert/frag/comp/wesl` (`bevy_shader-0.19.1/src/shader.rs:380`).
- Asset paths are relative to `assets/` next to the binary/workspace root (AssetPlugin default). `Handle<Image>`, `Handle<Font>`, `Handle<Mesh>`, `Handle<Shader>`.
- Image sampler per asset: `bevy::image::ImageLoaderSettings { format, texture_format, is_srgb: bool, sampler: ImageSampler, asset_usage, array_layout }` (`bevy_image-0.19.1/src/image_loader.rs:134`).
```rust
let img: Handle<Image> = assets.load_with_settings("ui/pixel.png", |s: &mut ImageLoaderSettings| { s.sampler = ImageSampler::nearest(); });
```

---

## 3. bevy_ui: Node, colors, images, UI text, z-index, interaction, widgets, hierarchy

Prelude (`bevy_ui-0.19.1/src/lib.rs:56`): `geometry::*` (Val, UiRect, px(), percent(), ...), `ui_node::*` (Node, Display, FlexDirection, ..., BackgroundColor, BorderColor, BorderRadius, Outline, ZIndex, GlobalZIndex, ComputedNode, ScrollPosition, UiTargetCamera),
`widget::{Button, ImageNode, Label, NodeImageMode, ViewportNode, Text, TextShadow, TextUiReader, TextUiWriter}`, `Interaction`, `UiScale`, `UiPickingCamera/UiPickingSettings`, and re-exports `BorderRect, SliceScaleMode, SpriteImageMode, TextureSlicer`.

### Node (`bevy_ui-0.19.1/src/ui_node.rs:492`)
```rust
#[require(ComputedNode, ComputedStackIndex, ContentSize, ComputedUiTargetCamera, ComputedUiRenderTargetInfo, UiTransform,
          BackgroundColor, BorderColor, FocusPolicy, ScrollPosition, Visibility, ZIndex)]
pub struct Node {
    pub display: Display,                 // #[default] Flex, Grid, Block, None            (:1154)
    pub box_sizing: BoxSizing,            // BorderBox | ContentBox
    pub position_type: PositionType,      // Relative | Absolute                            (:1460)
    pub overflow: Overflow,               // { x: OverflowAxis, y: OverflowAxis }; Overflow::clip()/hidden()/visible()/scroll()... (:1242)
    pub scrollbar_width: f32,
    pub overflow_clip_margin: OverflowClipMargin,
    pub left: Val, pub right: Val, pub top: Val, pub bottom: Val,
    pub width: Val, pub height: Val, pub min_width: Val, pub min_height: Val, pub max_width: Val, pub max_height: Val,
    pub aspect_ratio: Option<f32>,
    pub align_items: AlignItems,          // Default, Start, End, FlexStart, FlexEnd, Center, Baseline, Stretch   (:902)
    pub justify_items: JustifyItems,
    pub align_self: AlignSelf,            // Auto, Start, End, FlexStart, FlexEnd, Center, Baseline, Stretch
    pub justify_self: JustifySelf,
    pub align_content: AlignContent,
    pub justify_content: JustifyContent,  // Default, Start, End, FlexStart, FlexEnd, Center, Stretch, SpaceBetween, SpaceEvenly, SpaceAround (:1109)
    pub direction: InlineDirection,
    pub margin: UiRect, pub padding: UiRect, pub border: UiRect,
    pub border_radius: BorderRadius,      // <-- NOW A FIELD OF Node, not a component
    pub flex_direction: FlexDirection,    // Row, Column, RowReverse, ColumnReverse        (:1213)
    pub flex_wrap: FlexWrap,              // NoWrap, Wrap, WrapReverse
    pub flex_grow: f32, pub flex_shrink: f32, pub flex_basis: Val,
    pub row_gap: Val, pub column_gap: Val,
    pub grid_auto_flow: GridAutoFlow, pub grid_template_rows: Vec<RepeatedGridTrack>, pub grid_template_columns: Vec<RepeatedGridTrack>,
    pub grid_auto_rows: Vec<GridTrack>, pub grid_auto_columns: Vec<GridTrack>, pub grid_row: GridPlacement, pub grid_column: GridPlacement,
}
```
- `pub enum Val { Auto, Px(f32), Percent(f32), Vw(f32), Vh(f32), VMin(f32), VMax(f32) }` (geometry.rs:32); `Val::ZERO = Px(0.)`, `Val::DEFAULT = Auto`.
  Free helper fns in prelude: `px(v)`, `percent(v)`, `vw(v)`, `vh(v)`, `vmin(v)`, `vmax(v)`, `auto()` (geometry.rs:536-:578). Also `Val::px(..)` style methods `.left()/.all()/.horizontal()/.vertical()` produce a `UiRect` (:186-:302).
- `pub struct UiRect { pub left: Val, pub right: Val, pub top: Val, pub bottom: Val }` (geometry.rs:632): `UiRect::all(Val)`, `::px(l,r,t,b)`, `::percent(..)`, `::horizontal(Val)`, `::vertical(Val)`, `::axes(h, v)`, `::left/right/top/bottom(Val)`, `UiRect::ZERO/AUTO`.
- `pub struct BorderRadius { pub top_left: Val, pub top_right: Val, pub bottom_right: Val, pub bottom_left: Val }` (ui_node.rs:2526, plain struct, **not a Component**): `BorderRadius::all(Val)`, `::px(tl,tr,br,bl)`, `::percent(..)`, `::ZERO`, `::MAX`, `::top_left(Val)`...
- `pub struct BackgroundColor(pub Color)` (ui_node.rs:2229; default `Color::NONE`).
- `pub struct BorderColor { pub top: Color, pub right: Color, pub bottom: Color, pub left: Color }` (ui_node.rs:2256): `BorderColor::all(color)`, `impl<T: Into<Color>> From<T>`, `.set_all(color)`. **Not a newtype any more** — write `BorderColor::all(Color::WHITE)` (or `Color::WHITE.into()`).
- `pub struct Outline { pub width: Val, pub offset: Val, pub color: Color }` (ui_node.rs:2369); `Outline::new(width, offset, color)`.
- `pub struct ZIndex(pub i32)` (:2440, local among siblings) and `pub struct GlobalZIndex(pub i32)` (:2450, global).
- `ComputedNode` (:29): `size: Vec2`, `content_size`, `inverse_scale_factor`, `.size()`, `.contains_point(UiGlobalTransform, Vec2)` (:223), `.normalize_point(..)` (:247).
- `UiTransform { translation: Val2, scale: Vec2, rotation: Rot2 }` (ui_transform.rs:130) is the UI-node transform (required by Node). `UiGlobalTransform(Affine2)`.
- `UiScale(pub f32)` resource (lib.rs:126). `UiTargetCamera(pub Entity)` (ui_node.rs:2938) to bind a UI root to a specific camera.
- `ScrollPosition(pub Vec2)` (:419) — requires an `OverflowAxis::Scroll` axis.

### Interaction / Button / FocusPolicy (`bevy_ui-0.19.1/src/focus.rs`, `widget/button.rs`)
- `pub enum Interaction { Pressed, Hovered, #[default] None }` (focus.rs:51), updated by `ui_focus_system` (focus.rs:148). Query with `Changed<Interaction>`.
- `pub enum FocusPolicy { Block, Pass }` (focus.rs:108). `RelativeCursorPosition { cursor_over: bool, normalized: Option<Vec2> }` (focus.rs:85).
- `#[require(Node, FocusPolicy::Block, Interaction)] pub struct Button;` (widget/button.rs:9) — the **bevy_ui** button (Interaction-based).
- Interaction state markers (`bevy_ui-0.19.1/src/interaction_states.rs`): `InteractionDisabled` (:23), `Pressed` (:46), `Checkable` (:51), `Checked` (:56).

### ImageNode (`bevy_ui-0.19.1/src/widget/image.rs:18`)
```rust
#[require(Node, ImageNodeSize)]
pub struct ImageNode {
    pub color: Color,                       // tint, default WHITE
    pub image: Handle<Image>,
    pub texture_atlas: Option<TextureAtlas>,
    pub flip_x: bool, pub flip_y: bool,
    pub rect: Option<Rect>,
    pub image_mode: NodeImageMode,          // #[default] Auto, Stretch, Sliced(TextureSlicer), Tiled{tile_x, tile_y, stretch_value}  (:158)
    pub visual_box: VisualBox,
}
```
`ImageNode::new(Handle<Image>)` (:74), `::solid_color(Color)` (:85), `::from_atlas_image(..)` (:99), `.with_color(..)`, `.with_flip_x()`, `.with_rect(Rect)`, `.with_mode(NodeImageMode)`.

### UI Text (`bevy_ui-0.19.1/src/widget/text.rs:111`)
```rust
#[require(Node, TextLayout, TextFont, TextColor, LineHeight, LetterSpacing, TextNodeFlags, ContentSize, FontHinting::Enabled)]
pub struct Text(pub String);      // Text::new(impl Into<String>); From<&str>, From<String>
pub struct TextShadow { pub offset: Vec2, pub color: Color }   // :146
```
Same `TextFont`/`TextColor`/`TextLayout`/`TextSpan` as section 2. Update text: `TextUiWriter` (= `TextWriter<Text>`) `writer.text(root_entity, 0)` returns `Mut<String>`, or query `&mut Text` and assign `text.0 = "..".into()`; spans are children with `TextSpan`.

### bevy_ui_widgets (`bevy_ui_widgets-0.19.1/src`, module `bevy::ui_widgets`)
- `UiWidgetsPlugins` plugin group (lib.rs:61) is already in `DefaultPlugins`; it adds `ButtonPlugin, CheckboxPlugin, ListBoxPlugin, MenuPlugin, RadioGroupPlugin, ScrollAreaPlugin, ScrollbarPlugin, SliderPlugin, EditableTextInputPlugin, PopoverPlugin`.
- Headless widget button: `#[require(AccessibilityNode(..))] pub struct Button;` (button.rs:29) — **name-clashes with `bevy::ui::Button`**; import as `bevy::ui_widgets::Button as WidgetButton` or use paths.
  Marker `ActivateOnPress` (button.rs:35). The plugin's global observers (button.rs:37-:138) listen to `On<Pointer<Press>>`, `On<Pointer<Release>>`, `On<Pointer<Click>>`, `On<Pointer<DragEnd>>`, `On<Pointer<Cancel>>` and `On<FocusedInput<KeyboardInput>>` (Enter/Space), insert/remove `bevy::ui::Pressed`, and `commands.trigger(Activate { entity })`.
- `#[derive(EntityEvent)] pub struct Activate { pub entity: Entity }` (lib.rs:82), `pub struct ValueChange<T> { #[event_target] pub source: Entity, pub value: T, pub is_final: bool }` (lib.rs:90).
- `pub fn observe<E: EntityEvent, B: Bundle, M, I: IntoObserverSystem<E, B, M>>(observer: I) -> AddObserver<..>` (observe.rs) — a **Bundle** that attaches an entity observer while spawning (usable inside `children![]`).
```rust
use bevy::ui_widgets::{observe, Activate};
commands.spawn((
    Node { width: px(200.), height: px(56.), justify_content: JustifyContent::Center, align_items: AlignItems::Center,
           border: UiRect::all(px(2.)), border_radius: BorderRadius::all(px(8.)), ..default() },
    bevy::ui::Button,                    // Interaction-based (optional)
    bevy::ui_widgets::Button,            // widget: emits Activate
    BackgroundColor(Color::srgb(0.15, 0.15, 0.2)),
    BorderColor::all(Color::WHITE),
    observe(|activate: On<Activate>, mut next: ResMut<NextState<GameState>>| { next.set(GameState::InGame); }),
    children![(Text::new("Play"), TextFont { font_size: FontSize::Px(24.), ..default() }, TextColor(Color::WHITE))],
));
```
Classic Interaction polling still works:
```rust
fn buttons(mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<bevy::ui::Button>)>) {
    for (i, mut bg) in &mut q { bg.0 = match i { Interaction::Pressed => Color::srgb(0.3,0.3,0.3), Interaction::Hovered => Color::srgb(0.25,0.25,0.3), Interaction::None => Color::srgb(0.15,0.15,0.2) }; }
}
```

### Building a UI tree
- `children![a, (b, children![c])]` macro (`bevy_ecs-0.19.1/src/hierarchy.rs:519`) is a Bundle: `commands.spawn((Node{..}, children![...]))`.
- `commands.spawn(Node{..}).with_children(|p| { p.spawn(Text::new("hi")); })` (hierarchy.rs:383), `.with_child(bundle)` (:484).
- Explicit relationship: `commands.spawn((Node{..}, ChildOf(parent_entity)))` (`pub struct ChildOf(pub Entity)` hierarchy.rs:107; `.parent() -> Entity`). `Children(Vec<Entity>)` is auto-maintained (:152). `Parent` no longer exists.
- Layout root: a `Node` with no `ChildOf` is a root; `position_type: PositionType::Absolute` + `left/top` for overlays.
```rust
commands.spawn((
    Node { width: percent(100.), height: percent(100.), flex_direction: FlexDirection::Column,
           justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::all(px(16.)), ..default() },
    GlobalZIndex(10),
    DespawnOnExit(GameState::InGame),
    children![
        (Node { position_type: PositionType::Absolute, top: px(8.), left: px(8.), ..default() }, ImageNode::new(logo)),
        (Node { width: px(300.), height: px(48.), border: UiRect::all(px(1.)), border_radius: BorderRadius::all(px(6.)), ..default() },
         BackgroundColor(Color::srgba(0., 0., 0., 0.6)), BorderColor::all(Color::srgb(0.8, 0.7, 0.2)), Outline::new(px(1.), px(0.), Color::BLACK)),
    ],
));
```

---

## 4. bevy_picking: Pointer events, Pickable, hit data, cursor -> world

Prelude (`bevy_picking-0.19.1/src/lib.rs`): `events::*` (Pointer, Over, Out, **Enter**, **Leave**, Press, Release, Click, Move, DragStart, Drag, DragEnd, DragEnter, DragOver, DragLeave, DragDrop, Cancel, Scroll),
`PointerButton`, `Pickable`, `DefaultPickingPlugins`, `PickingPlugin`, `InteractionPlugin`, `PointerInputPlugin`, mesh picking types.

### The Pointer<E> entity event (`bevy_picking-0.19.1/src/events.rs:74`)
```rust
#[derive(Message, EntityEvent, Clone, PartialEq, Debug, Reflect, Component)]
#[entity_event(propagate = PointerTraversal, auto_propagate)]   // bubbles child -> parent (ChildOf) -> window entity
pub struct Pointer<E: Debug + Clone + Reflect> {
    pub entity: Entity,                 // the hit entity (event target)
    pub pointer_id: PointerId,          // Mouse | Touch(u64) | Custom(Uuid)
    pub pointer_location: Location,     // { target: NormalizedRenderTarget, position: Vec2 }  (pointer.rs:212) — viewport/logical px
    pub event: E,
    pub(crate) propagate: bool,
}
impl<E> Deref for Pointer<E> { type Target = E; }   // so `click.hit`, `click.button` work
```
Event payloads (events.rs):
```rust
pub struct Over  { pub hit: HitData }                                              // :192  bubbles to ALL ancestors
pub struct Out   { pub hit: HitData }                                              // :242  bubbles to ALL ancestors
pub struct Enter { pub hit: HitData, pub is_in_bounds: bool }                      // :225  web `mouseenter` semantics
pub struct Leave { pub hit: HitData, pub was_in_bounds: bool }                     // :275  web `mouseleave` semantics
pub struct Press { pub button: PointerButton, pub hit: HitData, pub count: u8 }    // :288  (NOT "Pressed")
pub struct Release { pub button: PointerButton, pub hit: HitData }                // :300  (NOT "Released")
pub struct Click { pub button: PointerButton, pub hit: HitData, pub duration: Duration, pub count: u8 } // :311
pub struct Move  { pub hit: HitData, pub delta: Vec2 }                             // :325
pub struct DragStart { pub button: PointerButton, pub hit: HitData }              // :340
pub struct Drag  { pub button: PointerButton, pub distance: Vec2, pub delta: Vec2 } // :350
pub struct DragEnd { pub button: PointerButton, pub distance: Vec2 }              // :372
pub struct DragEnter { pub button, pub dragged: Entity, pub hit: HitData }         // :387
pub struct DragOver  { pub button, pub dragged: Entity, pub hit: HitData }         // :399
pub struct DragLeave { pub button, pub dragged: Entity, pub hit: HitData }         // :411
pub struct DragDrop  { pub button, pub dropped: Entity, pub hit: HitData }         // :423
pub struct Cancel { pub hit: HitData }                                             // :180
pub struct Scroll { pub unit: MouseScrollUnit, pub x: f32, pub y: f32, pub hit: HitData, pub phase: TouchPhase } // :457-470 (mouse -> always TouchPhase::Moved)
pub struct HitData { pub camera: Entity, pub depth: f32, pub position: Option<Vec3>, pub normal: Option<Vec3> } // backend.rs:135
pub enum PointerButton { Primary, Secondary, Middle }                             // pointer.rs:161
```
- `#[derive(Component)] pub struct Pickable { pub should_block_lower: bool, pub is_hoverable: bool }` (lib.rs:198); default `{true, true}`; `Pickable::IGNORE` = `{false,false}`.
  Sprites/UI nodes are pickable by default (backends don't require markers unless `require_markers = true`); add `Pickable::IGNORE` to make an entity transparent to picking, `Pickable { should_block_lower: false, .. }` to let hits pass through.
- Hover state components (hover.rs): `PickingInteraction::{Pressed, Hovered, #[default] None}` (:226), `Hovered(pub bool)` (:339, immutable component; opt-in). Resources `HoverMap(HashMap<PointerId, EntityHashMap<HitData>>)` (:60).
- `PointerInteraction` component on pointer entities: `.get_nearest_hit() -> Option<&(Entity, HitData)>` (pointer.rs:79). `PointerLocation { location: Option<Location> }` (:180) component; `.location()`.
- `PickingSettings { is_enabled, is_input_enabled, is_hover_enabled, is_window_picking_enabled, multi_click_interval: Duration }` resource (lib.rs:316).
- UI: `UiPickingSettings { require_markers: bool }`, marker `UiPickingCamera` (`bevy_ui-0.19.1/src/picking_backend.rs:42`). Sprites: see section 2 (`SpritePickingSettings.picking_mode`: `BoundingBox` or `AlphaThreshold(f32)`; default `AlphaThreshold(0.1)`).

### Observers on sprites and UI nodes
```rust
// per-entity observer (sprite or UI node — identical API)
commands.spawn((Sprite::from_image(card_img), Transform::from_xyz(0., 0., 2.)))
    .observe(|click: On<Pointer<Click>>, mut commands: Commands| {
        let hit_entity = click.entity;                        // target
        let screen_pos: Vec2 = click.pointer_location.position;
        // Sprite backend: Some(world position) (bevy_sprite-0.19.1/src/picking_backend.rs:285-290).
        // UI backend: ALWAYS Some(..), and it is NOT a world position — it is the cursor in the node's local
        // space divided by the node size (roughly 0..1 node-relative) with z = 0
        // (bevy_ui-0.19.1/src/picking_backend.rs:238-256). Never treat a UI hit.position as world coords.
        let hit_pos: Option<Vec3> = click.hit.position;
        if click.button == PointerButton::Primary { commands.entity(hit_entity).insert(Selected); }
    })
    .observe(|over: On<Pointer<Over>>, mut q: Query<&mut Sprite>| { if let Ok(mut s) = q.get_mut(over.entity) { s.color = Color::srgb(1.2, 1.2, 1.2); } })
    .observe(|out: On<Pointer<Out>>, mut q: Query<&mut Sprite>| { if let Ok(mut s) = q.get_mut(out.entity) { s.color = Color::WHITE; } })
    .observe(|drag: On<Pointer<Drag>>, mut q: Query<&mut Transform>| {
        if let Ok(mut t) = q.get_mut(drag.entity) { t.translation.x += drag.delta.x; t.translation.y -= drag.delta.y; } // screen y is down
    })
    .observe(|mut press: On<Pointer<Press>>| { press.propagate(false); });   // stop bubbling to parents/window

// global observer (all entities)
app.add_observer(|drop: On<Pointer<DragDrop>>| { info!("dropped {:?} onto {:?}", drop.dropped, drop.entity); });
```
Hover events — pick the right pair (`bevy_picking-0.19.1/src/events.rs`, registered at lib.rs:441-442):
- `Over` (:192) / `Out` (:242) bubble to **every** ancestor via `ChildOf`. On nested nodes each ancestor fires too.
- `Enter` (:225) / `Leave` (:275) bubble only to the ancestors whose hover state actually changed — web
  `mouseenter`/`mouseleave` semantics — and carry `is_in_bounds` / `was_in_bounds` telling you whether the pointer
  was directly inside this entity's own bounds (false when only a child's overflowing bounds were entered/exited).
  For card/UI hover highlighting, prefer `On<Pointer<Enter>>` / `On<Pointer<Leave>>` to avoid double-firing.

Gotchas: `Trigger<Pointer<Click>>` -> `On<Pointer<Click>>`; `.target()` -> `.entity` field (`On::original_event_target()` gives the leaf before propagation, system_param.rs:139). Names are `Press`/`Release`, not `Pressed`/`Released`. `Pointer<E>` events are also `Message`s (readable with `MessageReader<Pointer<Click>>`).

### Manual cursor -> world and pointer position
- `Window::cursor_position() -> Option<Vec2>` + `Camera::viewport_to_world_2d(&GlobalTransform, Vec2) -> Result<Vec2, ViewportConversionError>` (section 2 snippet).
- `AccumulatedMouseMotion { delta: Vec2 }` / `AccumulatedMouseScroll { unit, delta: Vec2 }` resources (`bevy_input-0.19.1/src/mouse.rs:218,:239`); messages `MouseMotion { delta }` (:107), `MouseWheel { unit, x, y, window }` (:168), `MouseButtonInput { button, state: ButtonState, window }` (:42).
- `Res<ButtonInput<MouseButton>>` / `Res<ButtonInput<KeyCode>>` (`bevy_input-0.19.1/src/button_input.rs`): `pressed(x)`, `just_pressed(x)`, `just_released(x)`, `any_pressed(iter)`, `get_pressed()`.
  `MouseButton::{Left, Right, Middle, Back, Forward, Other(u16)}` (mouse.rs:72). `KeyCode::{Escape, Space, Enter, KeyA.., Digit1.., ArrowLeft..}`. `ButtonState::{Pressed, Released}` (lib.rs:180).
- Keyboard focus: `bevy::input_focus::InputFocus` resource (`bevy_input_focus-0.19.1/src/lib.rs:103`; `.set(entity, FocusCause::{Navigated, Pressed})` (gained_and_lost.rs:16), `.clear()`), `FocusedInput<M> { focused_entity: Entity, input: M }` entity event (:192).

---

## 5. Time, Timer, Stopwatch, hand-rolled tweening, Messages, Events, Observers

### Time (`bevy_time-0.19.1/src`; prelude: `Fixed, Real, Time, Timer, TimerMode, Virtual, DelayedCommandsExt`)
- `Res<Time>` = `Time<()>` (generic clock; in `Update` it's virtual time, in `FixedUpdate` it's fixed time). `Res<Time<Virtual>>`, `Res<Time<Real>>`, `Res<Time<Fixed>>`.
- `Time<T>` (time.rs): `delta() -> Duration` (:276), `delta_secs() -> f32` (:283), `delta_secs_f64()`, `elapsed() -> Duration` (:296), `elapsed_secs() -> f32` (:306), `elapsed_secs_wrapped() -> f32` (:329), `elapsed_secs_f64()`.
- `Time<Virtual>` (virt.rs): `pause()` (:215), `unpause()` (:221), `is_paused()` (:227), `set_relative_speed(f32)` (:188), `relative_speed()`, `set_max_delta(Duration)` (:140), `effective_speed()`.
- `Time<Fixed>` (fixed.rs): `Time::<Fixed>::from_hz(f64)` (:105), `from_seconds(f64)` (:94), `timestep()`, `set_timestep(Duration)`, `overstep_fraction() -> f32` (:205). Override: `app.insert_resource(Time::<Fixed>::from_hz(60.0))`.
- `commands.delayed().secs(1.5).entity(e).despawn();` via `DelayedCommandsExt::delayed(&mut self) -> DelayedCommands` (delayed_commands.rs:107) with `.secs(f32)`/`.duration(Duration)` returning `Commands`.

### Timer / Stopwatch
```rust
pub struct Timer { .. }                      // timer.rs:34
Timer::new(Duration, TimerMode) (:46); Timer::from_seconds(f32, TimerMode) (:61)
pub enum TimerMode { #[default] Once, Repeating }   // :495
tick(&mut self, delta: Duration) -> &Self (:278); is_finished() (:93)  /* was finished() */; just_finished() (:110)
elapsed()/elapsed_secs() (:128/:135); duration(); set_duration(); remaining()/remaining_secs(); fraction() -> f32 (:408); fraction_remaining() (:427)
reset() (:391); pause()/unpause()/is_paused(); finish() (:204); times_finished_this_tick() -> u32 (:482); mode()/set_mode()
pub struct Stopwatch { .. }                  // stopwatch.rs:34 ; new(), tick(Duration), elapsed(), elapsed_secs(), set_elapsed(), pause(), unpause(), is_paused(), reset()
```
Gotcha: `Timer::finished()` was renamed to `is_finished()`.

### Hand-rolled tweening (no external crate)
Available math: `f32::lerp(self, rhs, t)` / `inverse_lerp` / `remap` from `FloatExt` (glam, re-exported in `bevy::math::prelude`),
`Vec2/Vec3::lerp`, `Quat::slerp`, `StableInterpolate::{interpolate_stable, smooth_nudge(&mut self, &target, decay_rate, delta)}` (`bevy_math-0.19.1/src/common_traits.rs:426-:467`),
`EasingCurve::new(start, end, EaseFunction)` (`bevy_math-0.19.1/src/curve/easing.rs:312`) + `Curve::sample_clamped(t) -> T` (curve/mod.rs:349); `Ease` is implemented for f32/Vec2/Vec3/Vec4/Quat/Rot2/etc. (easing.rs:98-:161).
`EaseFunction::{Linear, QuadraticIn/Out/InOut, CubicIn/Out/InOut, QuarticIn/Out/InOut, QuinticIn/Out/InOut, SmoothStepIn/Out/SmoothStep, ... }` (easing.rs:435).
Color: `Mix::mix(&self, &other, f32)` and `Alpha::{with_alpha, alpha, set_alpha}` (`bevy_color-0.19.1/src/color_ops.rs:33,:59`; `impl Alpha for Color` color.rs:519).
```rust
#[derive(Component)]
struct Tween { timer: Timer, from: Vec3, to: Vec3, ease: EaseFunction, fade_out: bool }

fn tween_system(time: Res<Time>, mut commands: Commands, mut q: Query<(Entity, &mut Tween, &mut Transform, &mut Sprite)>) {
    for (e, mut tw, mut tf, mut sprite) in &mut q {
        tw.timer.tick(time.delta());
        let t = EasingCurve::new(0.0f32, 1.0, tw.ease).sample_clamped(tw.timer.fraction());
        tf.translation = tw.from.lerp(tw.to, t);
        tf.scale = Vec3::splat(1.0 + 0.1 * (t * core::f32::consts::PI).sin());
        if tw.fade_out { sprite.color.set_alpha(1.0 - t); }
        if tw.timer.is_finished() { commands.entity(e).remove::<Tween>(); }
    }
}
// spawn: Tween { timer: Timer::from_seconds(0.35, TimerMode::Once), from, to, ease: EaseFunction::CubicOut, fade_out: false }
// frame-rate independent "chase": tf.translation.smooth_nudge(&target, 8.0, time.delta_secs());
```

### Messages (buffered, was "Events" in <=0.16) — `bevy_ecs-0.19.1/src/message/`
- `#[derive(Message)] struct CardPlayed { card: Entity }` (`pub use bevy_ecs_macros::Message`, message/mod.rs). Register: `app.add_message::<CardPlayed>()`.
- `MessageWriter<'w, M>` (message_writer.rs:62): `write(&mut self, m: M) -> MessageId<M>` (:74), `write_batch(iter)` (:85), `write_default()` (:95).
- `MessageReader<'w, 's, M>` (message_reader.rs:34): `read(&mut self) -> MessageIterator<M>` (:44), `read_with_id()`, `len()`, `is_empty()`, `clear()`. `PopulatedMessageReader<M>` skips the system when empty.
- `Messages<M>` resource, `MessageMutator<M>`, `Commands::write_message(m)`. Run condition `on_message::<M>()`.
- Window/input events are Messages: `MessageReader<CursorMoved>`, `MessageReader<KeyboardInput>`, `MessageReader<MouseWheel>`.
```rust
fn play(mut w: MessageWriter<CardPlayed>) { w.write(CardPlayed { card }); }
fn react(mut r: MessageReader<CardPlayed>) { for m in r.read() { info!("{:?}", m.card); } }
```
Renames: `Event`->`Message`, `EventWriter`->`MessageWriter` (`send`->`write`), `EventReader`->`MessageReader`, `add_event`->`add_message`, `on_event`->`on_message`, `send_event`->`write_message`.

### Events + Observers (immediate, push-based) — `bevy_ecs-0.19.1/src/event/mod.rs`, `src/observer/`
```rust
pub trait Event: Send + Sync + Sized + 'static { type Trigger<'a>: Trigger<Self>; }   // event/mod.rs:88
pub trait EntityEvent: Event { fn event_target(&self) -> Entity; }                    // :327 (fn at :329)
#[derive(Event)] struct TurnEnded;                                  // global event, GlobalTrigger
#[derive(EntityEvent)] struct Explode { entity: Entity }            // entity-targeted; field named `entity` or mark one with #[event_target]
#[derive(EntityEvent)] #[entity_event(propagate, auto_propagate)] struct Bubble { entity: Entity }  // bubbles via ChildOf
```
- Observer system param: `On<'w, 't, E: Event, B: Bundle = ()>` (observer/system_param.rs:38) — `Deref<Target = E>`; `.event() -> &E` (:72), `.event_mut()` (:77), `.trigger()` (:87), `.observer() -> Entity` (:118),
  for propagating entity events: `.propagate(bool)` (:156), `.get_propagate()`, `.original_event_target() -> Entity` (:139).
  **`Trigger<E>` was renamed to `On<E>`; `trigger.target()` -> `on.entity` (your own field) / `on.event_target()`.**
- Registering: `app.add_observer(sys)` / `world.add_observer(sys)` / `commands.add_observer(sys)` (global; `impl IntoObserver<M>`), `commands.entity(e).observe(sys)` / `EntityCommands::observe` (entity-scoped; `impl IntoEntityObserver<M>`),
  or spawn `Observer::new(sys).with_entity(e)` (`Observer::watch_entity`).
- Triggering: `commands.trigger(TurnEnded)` (commands/mod.rs:1169), `commands.trigger(Explode { entity: e })`, `commands.entity(e).trigger(Explode::from)` / `.trigger(|entity| Explode { entity })` (:2358), `world.trigger(ev)` (observer/mod.rs:63).
- Component lifecycle events for observers (lifecycle.rs), all `EntityEvent { pub entity: Entity }`, used as `On<Add, MyComponent>`:
  `Add` (:337), `Insert` (:350), **`Discard` (:367)**, `Remove` (:380), `Despawn` (:392).
  **`Discard` is the 0.19 replacement for `OnReplace`/`Replace`** (it carries `#[doc(alias = "OnDiscard")]`,
  `#[doc(alias = "OnReplace")]`, `#[doc(alias = "Replace")]`, lifecycle.rs:360-370). It fires *before* a component
  value is overwritten or removed, so `On<Discard, T>` is the only hook that can still read the old value.
```rust
#[derive(EntityEvent)] struct Damage { entity: Entity, amount: i32 }
app.add_observer(|dmg: On<Damage>, mut q: Query<&mut Hp>| { if let Ok(mut hp) = q.get_mut(dmg.entity) { hp.0 -= dmg.amount; } });
app.add_observer(|add: On<Add, Hp>| { info!("hp added to {:?}", add.entity); });
commands.trigger(Damage { entity: target, amount: 3 });
commands.entity(target).observe(|d: On<Damage>| info!("{} on {:?}", d.amount, d.entity));
```

---

## 6. Custom 2D materials/shaders, Mesh2d, ColorMaterial, Gizmos, colors, image sampling

### Material2d (`bevy_sprite_render-0.19.1/src/mesh2d/material.rs`; module `bevy::sprite_render`)
```rust
pub trait Material2d: AsBindGroup + Asset + Clone + Sized {          // :136
    fn vertex_shader() -> ShaderRef { ShaderRef::Default }
    fn fragment_shader() -> ShaderRef { ShaderRef::Default }
    fn depth_bias(&self) -> f32 { 0.0 }
    fn alpha_mode(&self) -> AlphaMode2d { AlphaMode2d::Opaque }
    fn specialize(descriptor: &mut RenderPipelineDescriptor, layout: &MeshVertexBufferLayoutRef, key: Material2dKey<Self>) -> Result<(), SpecializedMeshPipelineError> { Ok(()) }
}
pub enum AlphaMode2d { #[default] Opaque, Mask(f32), Blend }          // :247
pub struct MeshMaterial2d<M: Material2d>(pub Handle<M>);              // :204  (prelude)
pub struct Material2dPlugin<M: Material2d>(PhantomData<M>);           // :268 ; Material2dPlugin::<M>::default()
pub struct Mesh2d(pub Handle<Mesh>);  #[require(Transform)]          // bevy_mesh-0.19.1/src/components.rs:45 (prelude via bevy::mesh)
```
- `ShaderRef::{#[default] Default, Handle(Handle<Shader>), Path(AssetPath<'static>)}` (`bevy_shader-0.19.1/src/shader.rs:406`); `From<&'static str>` (:428) and `From<AssetPath<'static>>` (:422) — `"shaders/sea.wgsl".into()` works.
- `AsBindGroup` derive and `ShaderType` derive: `bevy::render::render_resource::{AsBindGroup, ShaderType}` — the path compiles, but the two names come from different places:
  `ShaderType` is re-exported at `bevy_render-0.19.1/src/render_resource/mod.rs:73` (`pub use self::encase::{ShaderSize, ShaderType};`),
  while `AsBindGroup` does **not** appear in mod.rs at all — it arrives through the glob `pub use bind_group::*;` (mod.rs:20) and is defined in
  `render_resource/bind_group.rs:10` (`pub use bevy_render_macros::AsBindGroup;`, the derive) and `bind_group.rs:500` (`pub trait AsBindGroup`).
  Field attrs (bind_group.rs docs): `#[uniform(N)]` (ShaderType value -> uniform buffer; same N on several fields merges them into one struct), `#[texture(N)]`, `#[sampler(N)]`, `#[storage(N, read_only)]`, `#[storage_texture(N)]`.
  Struct attr: `#[uniform(N, ConvertedType)]` with `AsBindGroupShaderType`, `#[bind_group_data(T)]`.
  **WGSL group index is `@group(#{MATERIAL_BIND_GROUP})`** (a shader def injected by Material2dPlugin, material.rs:470) — do not hardcode `@group(2)`.
- Shader imports available to 2D materials: `#import bevy_sprite::mesh2d_vertex_output::VertexOutput` (fields `position, world_position: vec4, world_normal: vec3, uv: vec2`), `#import bevy_sprite::mesh2d_view_bindings::{view, globals}`
  with `globals: Globals { time: f32 (seconds, wraps at 1 h), delta_time: f32, frame_count: u32 }` at `@group(0) @binding(1)` (`mesh2d_view_bindings.wgsl`, `bevy_render-0.19.1/src/globals.wgsl:3`). So a time uniform is free; a custom one is optional.
- Reference: `ColorMaterial` (`color_material.rs:39`):
```rust
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[uniform(0, ColorMaterialUniform)]
pub struct ColorMaterial { pub color: Color, pub alpha_mode: AlphaMode2d, pub uv_transform: Affine2, #[texture(1)] #[sampler(2)] pub texture: Option<Handle<Image>> }
// ColorMaterial::from_color(impl Into<Color>) (:50); From<Color> (:67; alpha<1 -> Blend); From<Handle<Image>> (:81); Default = WHITE, Blend
#[derive(Clone, Default, ShaderType)] pub struct ColorMaterialUniform { pub color: Vec4, pub uv_transform: Mat3, pub flags: u32, pub alpha_cutoff: f32 }
```
Animated sea material:
```rust
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d};

#[derive(Clone, Copy, Default, ShaderType, Debug)]
struct SeaParams { deep: LinearRgba, shallow: LinearRgba, speed: f32, scale: f32, _pad: Vec2 }

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
struct SeaMaterial {
    #[uniform(0)] params: SeaParams,
    #[texture(1)] #[sampler(2)] noise: Option<Handle<Image>>,
}
impl Material2d for SeaMaterial {
    fn fragment_shader() -> ShaderRef { "shaders/sea.wgsl".into() }
    fn alpha_mode(&self) -> AlphaMode2d { AlphaMode2d::Blend }
}

fn plugin(app: &mut App) { app.add_plugins(Material2dPlugin::<SeaMaterial>::default()); }

fn spawn_sea(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<SeaMaterial>>, assets: Res<AssetServer>) {
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1600., 900.))),
        MeshMaterial2d(mats.add(SeaMaterial {
            params: SeaParams { deep: LinearRgba::new(0.02, 0.15, 0.35, 1.), shallow: LinearRgba::new(0.1, 0.5, 0.7, 1.), speed: 0.4, scale: 6., _pad: Vec2::ZERO },
            noise: Some(assets.load("textures/noise.png")),
        })),
        Transform::from_xyz(0., 0., -10.),
    ));
}
// optional CPU-side animation: for (_, m) in mats.iter_mut() { m.params.speed = ...; }  (Assets<M>::iter_mut / get_mut)
```
`assets/shaders/sea.wgsl`:
```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

struct SeaParams { deep: vec4<f32>, shallow: vec4<f32>, speed: f32, scale: f32, _pad: vec2<f32> };
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SeaParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var noise_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var noise_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = globals.time * params.speed;
    let uv = in.uv * params.scale;
    let n = textureSample(noise_tex, noise_sampler, uv + vec2(t * 0.05, t * 0.03)).r;
    let wave = 0.5 + 0.5 * sin(uv.x * 3.0 + t + n * 4.0) * cos(uv.y * 2.0 - t * 0.7);
    return mix(params.deep, params.shallow, wave * n);
}
```
Gotchas: `Material2d` trait comes from `bevy::sprite_render` (not `bevy::sprite`); `Mesh2d` from `bevy::mesh` (prelude); `AsBindGroup` from `bevy::render::render_resource`; `ShaderRef` from `bevy::shader`. Uniform structs must satisfy WGSL alignment (16-byte for vec4; use `ShaderType` derive and pad). `#import bevy_sprite::...` paths are unchanged.

### Mesh2d primitives (`bevy_mesh-0.19.1/src/primitives/dim2.rs`, `bevy_math-0.19.1/src/primitives/dim2.rs`)
- `Rectangle { half_size: Vec2 }`: `Rectangle::new(width, height)` (:1828), `from_size(Vec2)`, `from_corners(a, b)`, `from_length(f32)`; `impl From<Rectangle> for Mesh` (bevy_mesh dim2.rs:1114), `Meshable::mesh() -> RectangleMeshBuilder`.
- `Circle { radius }`: `Circle::new(r)` (:53); `impl From<Circle> for Mesh` (bevy_mesh dim2.rs:91). Also `Ellipse, Annulus, RegularPolygon, Capsule2d, Segment2d, Polyline2d, ConvexPolygon, CircularSector/Segment`.
- `Assets<Mesh>::add(impl Into<Mesh>)` accepts primitives directly: `meshes.add(Circle::new(20.))`.
```rust
commands.spawn((Mesh2d(meshes.add(Rectangle::new(200., 120.))), MeshMaterial2d(color_mats.add(Color::srgb(0.9, 0.2, 0.2))), Transform::from_xyz(0., 0., 1.)));
```
(`Assets<ColorMaterial>::add` accepts `Color` via `From<Color> for ColorMaterial`.)

### Gizmos (`bevy_gizmos-0.19.1/src`; `Gizmos` system param in prelude; enabled by `bevy_gizmos_render` from `2d_bevy_render`)
`pub struct Gizmos<'w, 's, Config = DefaultGizmoConfigGroup, Clear = ()>` (gizmos.rs:143). 2D methods:
- `line_2d(&mut self, start: Vec2, end: Vec2, color: impl Into<Color>)` (gizmos.rs:726), `linestrip_2d(positions: impl IntoIterator<Item = Vec2>, color)` (:772), `ray_2d(start, vector, color)` (:851), `line_gradient_2d(..)` (:746)
- `rect_2d(&mut self, isometry: impl Into<Isometry2d>, size: Vec2, color)` (:902) — pass `Vec2` position (`Isometry2d: From<Vec2>`) or `Isometry2d::new(pos, Rot2::radians(a))`
- `circle_2d(&mut self, isometry: impl Into<Isometry2d>, radius: f32, color) -> Ellipse2dBuilder` (circles.rs:167; `.resolution(n)`), `ellipse_2d` (:89), `arc_2d` (arcs.rs:46), `arrow_2d(start, end, color)` (arrows.rs:150), `rounded_rect_2d` (rounded_box.rs:318), `grid_2d` (grid.rs:319), `cross_2d`.
- Config: `ResMut<GizmoConfigStore>` -> `.config_mut::<DefaultGizmoConfigGroup>() -> (&mut GizmoConfig, &mut T)` (config.rs:157); `GizmoConfig { enabled: bool, line: GizmoLineConfig { width: f32, perspective: bool, style: GizmoLineStyle, joints }, depth_bias: f32, render_layers }` (config.rs:208,:248).
- Retained gizmos: `Gizmo` component + `GizmoAsset` (prelude). Immediate-mode gizmos are cleared each frame; draw every `Update`.
```rust
fn debug_draw(mut gizmos: Gizmos, q: Query<&GlobalTransform, With<Sprite>>) {
    for gt in &q { gizmos.rect_2d(gt.translation().truncate(), Vec2::new(120., 168.), Color::srgb(0., 1., 0.)); }
    gizmos.circle_2d(Vec2::ZERO, 8., bevy::color::palettes::css::GOLD);
    gizmos.line_2d(Vec2::new(-100., 0.), Vec2::new(100., 0.), Color::WHITE);
}
```

### Color API (`bevy_color-0.19.1/src`; prelude exports `Color`, all color spaces, and the ops traits `Alpha, Mix, Luminance, Hue, Saturation, Gray, ...`)
```rust
pub enum Color { Srgba(Srgba), LinearRgba(LinearRgba), Hsla(Hsla), Hsva(Hsva), Hwba(Hwba), Laba(Laba), Lcha(Lcha), Oklaba(Oklaba), Oklcha(Oklcha), Xyza(Xyza) } // color.rs:56
```
- Constructors (color.rs): `Color::srgb(r,g,b)` (:116), `srgba(r,g,b,a)` (:100), `srgb_u8(r,g,b)` (:162), `srgba_u8(..)` (:146), `srgb_from_array([f32;3])`, `linear_rgb(..)` (:234), `linear_rgba(..)` (:218), `hsl(h,s,l)` (:267), `hsla`, `hsv`, `oklch`, `oklab`... ; consts `Color::WHITE`, `Color::BLACK`, `Color::NONE` (:503-:509).
- Conversions: `to_srgba() -> Srgba` (:88), `to_linear() -> LinearRgba` (:83); `From<Srgba>/From<LinearRgba>/...` for `Color` and back.
- `pub struct Srgba { pub red, pub green, pub blue, pub alpha: f32 }` (srgba.rs:28): `Srgba::new(r,g,b,a)`, `Srgba::rgb(r,g,b)`, `Srgba::hex("#RRGGBB") -> Result<Srgba, HexColorError>` (:127), `rgb_u8`, `rgba_u8`, `to_hex()`.
- `pub struct LinearRgba { pub red, pub green, pub blue, pub alpha: f32 }` (linear_rgba.rs:27): `new`, `rgb`, `as_u32()`; implements `ShaderType` (use in uniforms).
- Traits (color_ops.rs): `Alpha { with_alpha(&self, f32) -> Self; alpha(&self) -> f32; set_alpha(&mut self, f32); is_fully_transparent(); is_fully_opaque() }` (:59), `Mix { mix(&self, &Self, f32) -> Self; mix_assign }` (:33), `Luminance { luminance(); darker(f32); lighter(f32) }`, `Hue { with_hue; hue; rotate_hue }`.
- Palettes: `bevy::color::palettes::css::{GOLD, SKY_BLUE, DARK_GRAY, ...}`, `palettes::basic::{RED, BLUE, WHITE, NAVY, LIME,...}`, `palettes::tailwind::{SKY_400, SLATE_900, ...}` — these are `Srgba` consts; use `.into()` or `Color::from(GOLD)` where a `Color` is required (`impl Into<Color>` params accept them directly).
```rust
let tint = Color::srgb(1.0, 0.8, 0.2).with_alpha(0.5);
let faded = Color::WHITE.mix(&Color::NONE, 0.3);
sprite.color = bevy::color::palettes::tailwind::SKY_400.into();
```

### Image sampling / nearest filtering (`bevy_image-0.19.1/src/image.rs`; `bevy::image::{Image, ImagePlugin}` in prelude)
- `pub struct ImagePlugin { pub default_sampler: ImageSamplerDescriptor }` (:180); `ImagePlugin::default_nearest()` (:200), `ImagePlugin::default_linear()` (:193, the default). Use `DefaultPlugins.set(ImagePlugin::default_nearest())` for pixel art.
- `pub enum ImageSampler { #[default] Default, Descriptor(ImageSamplerDescriptor) }` (:676); `ImageSampler::nearest()` (:693), `ImageSampler::linear()` (:687).
- `ImageSamplerDescriptor { label, address_mode_u/v/w: ImageAddressMode /*ClampToEdge|Repeat|MirrorRepeat|ClampToBorder*/, mag_filter/min_filter/mipmap_filter: ImageFilterMode /*Nearest|Linear*/, lod_min_clamp, lod_max_clamp, compare, anisotropy_clamp, border_color }` (:833); `ImageSamplerDescriptor::nearest()` (:893), `::linear()` (:882).
- `Image { pub data: Option<Vec<u8>>, pub data_order, pub texture_descriptor, pub sampler: ImageSampler, pub texture_view_descriptor, pub asset_usage, .. }` (:613) — set `image.sampler = ImageSampler::nearest()` on a loaded asset via `Assets<Image>::get_mut`, or per-load via `ImageLoaderSettings.sampler` (section 2). Use `ImageAddressMode::Repeat` for tiling noise textures in shaders.

---

## Quick rename table (older Bevy -> 0.19.1)
| Old | 0.19.1 | Source |
|---|---|---|
| `Camera2dBundle` / `SpriteBundle` / `Text2dBundle` / `NodeBundle` / `TextBundle` / `ButtonBundle` / `ImageBundle` | plain components `Camera2d`, `Sprite`, `Text2d`, `Node`, `Text`, `Button`, `ImageNode` with `#[require]` | bevy_camera components.rs:16, bevy_sprite sprite.rs:19, bevy_ui ui_node.rs:492 |
| `Style` | `Node` (layout fields live on `Node`) | bevy_ui ui_node.rs:492 |
| `BorderRadius` component | `Node::border_radius` field | bevy_ui ui_node.rs:738 |
| `BorderColor(Color)` newtype | `BorderColor { top, right, bottom, left }` / `BorderColor::all(c)` | bevy_ui ui_node.rs:2256 |
| `UiImage` | `ImageNode` | bevy_ui widget/image.rs:18 |
| `Text::new("..")` with sections | `Text(String)` + child `TextSpan`s | bevy_ui widget/text.rs:111 |
| `TextFont { font: Handle<Font>, font_size: f32 }` | `font: FontSource` (`handle.into()`), `font_size: FontSize::Px(f32)` | bevy_text text.rs:376,:487 |
| `JustifyText` | `Justify` | bevy_text text.rs:233 |
| `Sprite::anchor` field | `Anchor` component (`bevy::sprite::Anchor`) | bevy_sprite sprite.rs:16,:257 |
| `Timer::finished()` | `Timer::is_finished()` | bevy_time timer.rs:93 |
| `Event`/`EventReader`/`EventWriter`/`add_event`/`send` | `Message`/`MessageReader`/`MessageWriter`/`add_message`/`write` | bevy_ecs message/ |
| `Trigger<E>` / `trigger.target()` / `trigger.entity()` | `On<E>` / `.entity` field / `event_target()` | bevy_ecs observer/system_param.rs:38 |
| `Trigger<OnAdd, T>` | `On<Add, T>` (also `Insert`, `Discard`, `Remove`, `Despawn`) | bevy_ecs lifecycle.rs:337,:350,:367,:380,:392 |
| `Trigger<OnReplace, T>` / `Replace` | `On<Discard, T>` (fires before the old value is overwritten/removed) | bevy_ecs lifecycle.rs:367 |
| `Pointer<Down>` / `Pointer<Up>` (0.15) | `Pointer<Press>` / `Pointer<Release>` | bevy_picking events.rs:288,:300 |
| `StateScoped(S)` | `DespawnOnExit(S)` | bevy_state state_scoped.rs:149 |
| `Parent` / `BuildChildren` | `ChildOf(Entity)` / `children![]` / `with_children` | bevy_ecs hierarchy.rs:107,:519 |
| `despawn_recursive()` | `despawn()` (recursive by default) | bevy_ecs commands/mod.rs:1906 |
| `Query::get_single()` | `Query::single() -> Result` / `Single<..>` param | bevy_ecs system/query.rs:2097 |
| `WindowResolution::new(f32, f32)` | `WindowResolution::new(u32, u32)` (physical px) | bevy_window window.rs:923 |
| `bevy::render::texture::ImagePlugin` | `bevy::image::ImagePlugin` | bevy_image image.rs:180 |
| `bevy::sprite::{Material2d, MeshMaterial2d, ColorMaterial}` | `bevy::sprite_render::{..}` (Mesh2d in `bevy::mesh`) | bevy_sprite_render lib.rs:24 |
| `@group(2)` in material WGSL | `@group(#{MATERIAL_BIND_GROUP})` | bevy_sprite_render mesh2d/material.rs:470 |
| `Handle::Weak` | removed; `Handle::{Strong, Uuid}` | bevy_asset handle.rs:134 |
| `Time::delta_seconds()` | `delta_secs()` / `elapsed_secs()` | bevy_time time.rs:283 |
