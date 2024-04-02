use std::{net::IpAddr, time::Instant};

use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct PlayerInput {
    // networked
    pub id: u32,
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub pitch: f32,
    pub yaw: f32,

    // not networked
    #[serde(skip)]
    pub final_translation: Vec3,

    #[serde(skip)]
    pub timestamp: u128,
}

impl PlayerInput {
    pub fn compute_move_direction(&self, rotation: Quat) -> Vec3 {
        let mut direction = Vec3::ZERO;
        if self.forward {
            direction -= rotation.mul_vec3(Vec3::Z);
        }
        if self.backward {
            direction += rotation.mul_vec3(Vec3::Z);
        }
        if self.left {
            direction -= rotation.mul_vec3(Vec3::X);
        }
        if self.right {
            direction += rotation.mul_vec3(Vec3::X);
        }
        if direction.length() > 0.0 {
            direction = direction.normalize();
        }
        direction
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PlayerInputOutcome {
    pub translation: Vec3,
    pub velocity: Vec3,
}

#[derive(Serialize, Deserialize)]
/// what we send to the server
pub struct PlayerInputMessage {
    pub latest_processed_snapshot_id: Option<u32>,
    pub inputs: Vec<PlayerInput>,
}

#[derive(Resource, Default)]
pub struct InputHistory {
    pub next_id: u32,
    pub input_group_for_next_fixed_tick: Vec<PlayerInput>,
    pub inputs_for_next_send: Vec<PlayerInput>,
    pub input_groups: Vec<Vec<PlayerInput>>,
    pub latest_processed_input_id: u32,
    pub latest_processed_snapshot_id: Option<u32>,
}

#[derive(Resource, Default)]
pub struct SnapshotHistory {
    pub snapshots: Vec<Snapshot>,
    pub next_id: u32,
}

#[derive(Resource)]
pub struct LocalPlayer {
    pub client_id: ClientId,
}

impl LocalPlayer {
    pub fn is_authority(&self) -> bool {
        self.client_id.raw() == 0
    }
}

#[derive(Resource)]
pub struct ClientSettings {
    pub address: IpAddr,
    pub port: u16,
}

#[derive(Resource)]
pub struct ServerSettings {
    pub port: u16,
}

#[derive(Component)]
pub struct Character {
    pub owner_client_id: ClientId,
    pub move_accel: f32,
    pub move_speed: f32,
    pub move_friction: f32,
    pub velocity: Vec3,
    pub pitch: f32,
    pub yaw: f32,
}

impl Character {
    pub fn process_input(
        &mut self,
        input: &mut PlayerInput,
        transform: &mut Transform,
        delta_seconds: f32,
    ) {
        self.pitch = input.pitch;
        self.yaw = input.yaw;

        let rotation = Quat::from_rotation_y(self.yaw);
        let wish_direction = input.compute_move_direction(rotation);

        // todo, refactor (cleanup)
        self.velocity = Self::deccelerate(
            self.velocity,
            self.velocity.length(),
            self.move_friction,
            delta_seconds,
        );

        self.velocity += Self::accelerate(
            wish_direction,
            self.move_speed,
            self.velocity.length(),
            self.move_accel,
            delta_seconds,
        );

        transform.translation += self.velocity * delta_seconds;
        input.final_translation = transform.translation;
    }

    pub fn accelerate(
        wish_direction: Vec3,
        wish_speed: f32,
        current_speed: f32,
        accel: f32,
        delta_seconds: f32,
    ) -> Vec3 {
        let add_speed = wish_speed - current_speed;

        if add_speed <= 0.0 {
            return Vec3::ZERO;
        }

        let mut accel_speed = accel * delta_seconds * wish_speed;
        if accel_speed > add_speed {
            accel_speed = add_speed;
        }

        wish_direction * accel_speed
    }

    pub fn deccelerate(
        velocity: Vec3,
        current_speed: f32,
        friction: f32,
        delta_seconds: f32,
    ) -> Vec3 {
        let mut new_speed;
        let mut drop = 0.0;

        drop += current_speed * friction * delta_seconds;

        new_speed = current_speed - drop;
        if new_speed < 0.0 {
            new_speed = 0.0;
        }

        if new_speed != 0.0 {
            new_speed /= current_speed;
        }

        velocity * new_speed
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Snapshot {
    // networked
    pub id: u32,
    pub latest_processed_input_id: Option<u32>,
    pub character_snapshots: Vec<CharacterSnapshot>,

    // not networked
    #[serde(skip)]
    pub timestamp: u128,
}

impl Snapshot {
    pub fn diff(&self, old: &Self) -> Snapshot {
        Snapshot {
            id: self.id,
            timestamp: self.timestamp,
            latest_processed_input_id: self.latest_processed_input_id,
            character_snapshots: {
                let mut diffs = Vec::new();
                for snapshot in &self.character_snapshots {
                    if let Some(old_snapshot) = old
                        .character_snapshots
                        .iter()
                        .find(|old_snapshot| old_snapshot.client_id == snapshot.client_id)
                    {
                        let diff = snapshot.diff(old_snapshot);
                        if !diff.is_empty() {
                            diffs.push(diff);
                        }
                    } else {
                        diffs.push(snapshot.clone());
                    }
                }
                diffs
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CharacterSnapshot {
    pub client_id: u64,
    pub translation: Option<Vec3>,
    pub velocity: Option<Vec3>,
}

impl CharacterSnapshot {
    pub fn from_character(character: &Character, transform: &Transform) -> Self {
        Self {
            client_id: character.owner_client_id.raw(),
            translation: Some(transform.translation),
            velocity: Some(character.velocity),
        }
    }

    pub fn apply(&self, character: &mut Character, transform: &mut Transform) {
        if let Some(translation) = self.translation {
            transform.translation = translation;
        }
        if let Some(velocity) = self.velocity {
            character.velocity = velocity;
        }
    }

    pub fn diff(&self, old: &Self) -> Self {
        Self {
            client_id: self.client_id,
            translation: {
                if let (Some(new), Some(old)) = (self.translation, old.translation) {
                    if new != old {
                        Some(new)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            velocity: {
                if let (Some(new), Some(old)) = (self.velocity, old.velocity) {
                    if new != old {
                        Some(new)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.translation.is_none() && self.velocity.is_none()
    }
}

#[derive(Serialize, Deserialize)]
pub enum ReliableServerMessage {
    SpawnCharacter(u64, Vec3, Vec3),
}

#[derive(Serialize, Deserialize)]
pub enum UnreliableServerMessage {
    Snapshot(Snapshot),
}

#[derive(Serialize, Deserialize)]
pub enum UnreliableClientMessage {
    PlayerInputMessage(PlayerInputMessage),
}

#[derive(Event)]
pub struct SpawnCharacterVisualsEvent {
    pub entity: Entity,
    pub owner_client_id: ClientId,
    pub translation: Vec3,
}

#[derive(Component)]
pub struct CharacterVisuals {
    pub owner_client_id: ClientId,
    pub character_entity: Entity,
    pub last_physics_translation: Vec3,
}

#[derive(Resource)]
pub struct LastPhysicsUpdate {
    pub time: Instant,
}
