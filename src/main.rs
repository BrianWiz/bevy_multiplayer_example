use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::{prelude::*, winit::WinitSettings};
use bevy_renet::{
    renet::ClientId,
    transport::{NetcodeClientPlugin, NetcodeServerPlugin},
    RenetClientPlugin, RenetServerPlugin,
};
use clap::Parser;
use std::net::{IpAddr, Ipv4Addr};

mod client;
mod core;
mod input;
mod server;
mod stats;

use core::*;
use std::time::Instant;

const MOUSE_SENSITIVITY: f32 = 0.01;

const CHARACTER_HEIGHT: f32 = 0.7;
const CHARACTER_SPEED: f32 = 5.0;
const CHARACTER_ACCEL: f32 = 8.0;
const CHARACTER_FRICTION: f32 = 8.0;

const SMOOTH_CORRECTION_DISTANCE_THRESHOLD: f32 = 0.001;
const SMOOTH_CORRECTION_STEP_MIN: f32 = 0.25;
const SMOOTH_CORRECTION_STEP_MAX: f32 = 0.75;
const DEFAULT_PORT: u16 = 7777;

#[derive(Parser, PartialEq, Resource, Clone)]
pub enum Cli {
    SinglePlayer,
    DedicatedServer {
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    ListenServer {
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    Client {
        #[arg(short, long, default_value_t = Ipv4Addr::LOCALHOST.into())]
        ip: IpAddr,

        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
}

fn main() {
    let mut app = App::new();

    match Cli::try_parse() {
        Ok(Cli::SinglePlayer) => {
            println!("Starting single player game");
            app.add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    //present_mode: PresentMode::Immediate,
                    ..default()
                }),
                ..default()
            }));
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
            app.add_plugins(stats::FpsCounterPlugin);
            app.add_plugins(input::InputPlugin);
            app.add_systems(Startup, spawn_authority_character_system);
            app.insert_resource(LocalPlayer {
                client_id: ClientId::from_raw(0),
            });
            app.add_systems(Update, spawn_character_visuals_system);
            app.add_systems(
                Update,
                (extrapolate_player_visuals_system, camera_system).chain(),
            );
            app.add_systems(FixedPostUpdate, post_fixed_player_visuals_system);
        }

        Ok(Cli::DedicatedServer { port }) => {}

        Ok(Cli::ListenServer { port }) => {
            app.insert_resource(ServerSettings { port });
            app.add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    //present_mode: PresentMode::Immediate,
                    ..default()
                }),
                ..default()
            }));
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
            app.add_plugins(stats::FpsCounterPlugin);
            app.add_plugins(input::InputPlugin);
            app.add_plugins(server::ServerPlugin);
            app.add_plugins(RenetServerPlugin);
            app.add_plugins(NetcodeServerPlugin);
            app.add_systems(Startup, spawn_authority_character_system);
            app.add_systems(Update, spawn_character_visuals_system);
            app.add_systems(
                Update,
                (extrapolate_player_visuals_system, camera_system).chain(),
            );
            app.add_systems(FixedPostUpdate, post_fixed_player_visuals_system);
        }

        Ok(Cli::Client { ip, port }) => {
            app.insert_resource(ClientSettings { address: ip, port });
            app.add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    //present_mode: PresentMode::Immediate,
                    ..default()
                }),
                ..default()
            }));
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
            app.add_plugins(stats::FpsCounterPlugin);
            app.add_plugins(input::InputPlugin);
            app.add_plugins(client::ClientPlugin);
            app.add_plugins(RenetClientPlugin);
            app.add_plugins(NetcodeClientPlugin);
            app.add_systems(Update, spawn_character_visuals_system);
            app.add_systems(
                Update,
                (extrapolate_player_visuals_system, camera_system).chain(),
            );
            app.add_systems(FixedPostUpdate, post_fixed_player_visuals_system);
        }

        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    app.add_systems(Startup, setup_level);
    app.insert_resource(WinitSettings {
        focused_mode: bevy::winit::UpdateMode::Continuous,
        unfocused_mode: bevy::winit::UpdateMode::Continuous,
    });
    app.insert_resource(Time::<Fixed>::from_hz(64.0));
    app.insert_resource(LastPhysicsUpdate {
        time: std::time::Instant::now(),
    });
    app.add_event::<SpawnCharacterVisualsEvent>();
    app.run();
}

fn setup_level(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    println!("Setting up level");

    let ground_texture = asset_server.load("texture_04.png");

    // floor
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(20.0, 0.1, 20.0)),
        material: materials.add(StandardMaterial {
            base_color_texture: Some(ground_texture.clone()),
            ..default()
        }),
        transform: Transform::from_xyz(0.0, -0.5, 0.0),
        ..default()
    });

    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });

    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-2.5, 4.5, 9.0),
        ..default()
    });
}

fn spawn_character(
    owner_client_id: ClientId,
    event: &mut EventWriter<SpawnCharacterVisualsEvent>,
    commands: &mut Commands,
    translation: Vec3,
    velocity: Vec3,
) {
    let entity = commands
        .spawn((
            Character {
                owner_client_id,
                move_friction: CHARACTER_FRICTION,
                move_speed: CHARACTER_SPEED,
                move_accel: CHARACTER_ACCEL,
                velocity: velocity,
                pitch: 0.0,
                yaw: 0.0,
            },
            TransformBundle {
                global: GlobalTransform::from_translation(translation),
                ..default()
            },
        ))
        .id();

    event.send(SpawnCharacterVisualsEvent {
        translation,
        entity,
        owner_client_id,
    });
}

fn spawn_authority_character_system(
    mut spawn_visuals: EventWriter<SpawnCharacterVisualsEvent>,
    mut commands: Commands,
) {
    spawn_character(
        ClientId::from_raw(0),
        &mut spawn_visuals,
        &mut commands,
        Vec3::ZERO,
        Vec3::ZERO,
    );
}

fn spawn_character_visuals_system(
    mut spawn_visuals: EventReader<SpawnCharacterVisualsEvent>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for event in spawn_visuals.read() {
        if commands.get_entity(event.entity).is_some() {
            commands.spawn((
                CharacterVisuals {
                    owner_client_id: event.owner_client_id,
                    character_entity: event.entity,
                    last_physics_translation: event.translation,
                },
                PbrBundle {
                    mesh: meshes.add(Cuboid::new(0.465, CHARACTER_HEIGHT * 2.0, 0.465)),
                    material: materials.add(Color::rgb(0.0, 0.0, 0.5)),
                    transform: Transform::from_translation(event.translation),
                    ..default()
                },
            ));
        }
    }
}

fn compute_physics_interpolation_fraction(
    fixed_time: &Res<Time<Fixed>>,
    last_physics_update: Instant,
) -> f32 {
    let time_since_last_physics_update = Instant::now() - last_physics_update;
    ((time_since_last_physics_update.as_secs_f64() / fixed_time.delta_seconds_f64()) as f32)
        .clamp(0.0, 1.0)
}

fn extrapolate_player_visuals_system(
    fixed_time: Res<Time<Fixed>>,
    last_physics_update: Res<LastPhysicsUpdate>,
    mut visuals: Query<(&CharacterVisuals, &mut Transform)>,
    characters: Query<&Character>,
) {
    for (visuals, mut visuals_transform) in visuals.iter_mut() {
        if let Ok(character) = characters.get(visuals.character_entity) {
            let fraction =
                compute_physics_interpolation_fraction(&fixed_time, last_physics_update.time);
            if character.velocity.is_finite() {
                visuals_transform.translation = visuals.last_physics_translation
                    + character.velocity * fixed_time.delta_seconds() * fraction;
            }
        }
    }
}

fn post_fixed_player_visuals_system(
    local_player: Res<LocalPlayer>,
    mut last_physics_update: ResMut<LastPhysicsUpdate>,
    characters: Query<(&Character, &Transform)>,
    mut visuals: Query<(&mut CharacterVisuals, &Transform), Without<Character>>,
) {
    for (mut visuals, visuals_transform) in visuals.iter_mut() {
        if let Ok((character, character_transform)) = characters.get(visuals.character_entity) {
            // simulated characters ("we" aren't controlling these, just observing)
            if character.owner_client_id != local_player.client_id {
                visuals.last_physics_translation = character_transform.translation;
            }
            // owned characters ("we" are controlling these)
            else {
                // if we're the server player, we can just use the physics translation
                if local_player.is_authority() {
                    visuals.last_physics_translation = character_transform.translation;
                }
                // if we're a client and this is our character
                else {
                    let diff = visuals_transform
                        .translation
                        .distance(character_transform.translation);
                    if diff > SMOOTH_CORRECTION_DISTANCE_THRESHOLD {
                        let step_scale = (SMOOTH_CORRECTION_DISTANCE_THRESHOLD - diff).max(0.0)
                            / SMOOTH_CORRECTION_DISTANCE_THRESHOLD;
                        let dynamic_step = SMOOTH_CORRECTION_STEP_MIN
                            + (SMOOTH_CORRECTION_STEP_MAX - SMOOTH_CORRECTION_STEP_MIN)
                                * step_scale;
                        visuals.last_physics_translation = visuals_transform
                            .translation
                            .lerp(character_transform.translation, dynamic_step);
                    } else {
                        visuals.last_physics_translation = character_transform.translation;
                    }
                }
            }
        }
    }
    last_physics_update.time = Instant::now();
}

fn camera_system(
    local_player: Res<LocalPlayer>,
    characters: Query<&Character>,
    visuals: Query<(&CharacterVisuals, &Transform)>,
    mut camera: Query<&mut Transform, (With<Camera>, Without<CharacterVisuals>)>,
) {
    for mut camera_transform in camera.iter_mut() {
        for (visuals, visuals_transform) in visuals.iter() {
            if visuals.owner_client_id == local_player.client_id {
                if let Ok(character) = characters.get(visuals.character_entity) {
                    camera_transform.rotation =
                        Quat::from_euler(EulerRot::YXZ, character.yaw, character.pitch, 0.0);
                    camera_transform.translation =
                        visuals_transform.translation + Vec3::new(0.0, CHARACTER_HEIGHT, 0.0);
                }
            }
        }
    }
}
