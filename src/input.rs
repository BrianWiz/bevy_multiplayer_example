use crate::{core::*, MOUSE_SENSITIVITY};
use bevy::{input::mouse::MouseMotion, prelude::*};
use core::f32::consts::FRAC_PI_2;
use std::{
    f32::consts::{PI, TAU},
    time::{Instant, SystemTime},
};

const ANGLE_EPSILON: f32 = 0.001953125;
const SMOOTHING_FACTOR: f32 = 0.1;

pub struct InputPlugin;
impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, capture_inputs_system);
        app.add_systems(FixedUpdate, apply_inputs_system);
        app.init_resource::<InputHistory>();
    }
}

fn capture_inputs_system(
    local_player: Res<LocalPlayer>,
    mut mouse_motion_events: EventReader<MouseMotion>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut history: ResMut<InputHistory>,
    mut characters: Query<&mut Character>,
) {
    for mut character in characters.iter_mut() {
        if character.owner_client_id != local_player.client_id {
            return;
        }

        let mut input = PlayerInput {
            id: history.next_id,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            forward: keyboard.pressed(KeyCode::KeyW),
            backward: keyboard.pressed(KeyCode::KeyS),
            left: keyboard.pressed(KeyCode::KeyA),
            right: keyboard.pressed(KeyCode::KeyD),
            jump: keyboard.pressed(KeyCode::Space),
            final_translation: Vec3::ZERO,
            pitch: character.pitch,
            yaw: character.yaw,
        };

        // Calculate the total mouse delta as before but apply smoothing
        let mut total_mouse_delta = Vec2::ZERO;
        for mouse_event in mouse_motion_events.read() {
            total_mouse_delta += mouse_event.delta;
        }
        total_mouse_delta *= MOUSE_SENSITIVITY;

        // Smoothly interpolate the mouse delta using a smoothing factor
        let smoothed_mouse_delta =
            total_mouse_delta * SMOOTHING_FACTOR + (1.0 - SMOOTHING_FACTOR) * Vec2::ZERO; // Vec2::ZERO can be replaced with the previous frame's delta if available

        // Update pitch and yaw with the smoothed deltas
        input.pitch = (input.pitch - smoothed_mouse_delta.y)
            .clamp(-FRAC_PI_2 + ANGLE_EPSILON, FRAC_PI_2 - ANGLE_EPSILON);
        input.yaw -= smoothed_mouse_delta.x;

        // Normalize yaw to prevent large values and potential precision issues
        if input.yaw.abs() > PI {
            input.yaw = input.yaw.rem_euclid(TAU);
        }

        character.pitch = input.pitch;
        character.yaw = input.yaw;

        history.input_group_for_next_fixed_tick.push(input);
        history.next_id += 1;

        // only keep inputs up to a second ago
        history.input_groups = history
            .input_groups
            .iter()
            .filter(|inputs| {
                if let Some(input) = inputs.last() {
                    let age = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        - input.timestamp;
                    age < 1000
                } else {
                    false
                }
            })
            .cloned()
            .collect();
    }
}

fn apply_inputs_system(
    local_player: Res<LocalPlayer>,
    fixed_time: Res<Time<Fixed>>,
    mut last_physics_update: ResMut<LastPhysicsUpdate>,
    mut history: ResMut<InputHistory>,
    mut characters: Query<(&mut Character, &mut Transform), Without<CharacterVisuals>>,
) {
    last_physics_update.time = Instant::now();

    for (mut character, mut transform) in characters.iter_mut() {
        if character.owner_client_id != local_player.client_id {
            continue;
        }

        let mut latest_processed_input_id = history.latest_processed_input_id;

        if history.input_group_for_next_fixed_tick.is_empty() {
            return;
        }

        let chopped_delta =
            fixed_time.delta_seconds() / history.input_group_for_next_fixed_tick.len() as f32;

        for mut input in history.input_group_for_next_fixed_tick.iter_mut() {
            if input.id > latest_processed_input_id {
                character.process_input(&mut input, &mut transform, chopped_delta);
                latest_processed_input_id = input.id;
            }
        }

        history.latest_processed_input_id = latest_processed_input_id;

        let input_group = history.input_group_for_next_fixed_tick.clone();
        history.input_groups.push(input_group);
        history.inputs_for_next_send = history.input_group_for_next_fixed_tick.clone();
        history.input_group_for_next_fixed_tick.clear();

        return;
    }
}
