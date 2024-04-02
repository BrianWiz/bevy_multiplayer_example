// file from https://bevy-cheatbook.github.io/cookbook/print-framerate.html

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;

use crate::Character;
use crate::LocalPlayer;

pub struct FpsCounterPlugin;
impl Plugin for FpsCounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_fps_counter);
        app.add_systems(Update, (fps_text_update_system, speed_text_update_system));
    }
}

#[derive(Component)]
struct FpsText;

#[derive(Component)]
struct SpeedText;

fn setup_fps_counter(mut commands: Commands) {
    commands.spawn((
        FpsText,
        TextBundle {
            text: Text::from_sections([
                TextSection {
                    value: "FPS: ".into(),
                    style: TextStyle {
                        font_size: 16.0,
                        color: Color::WHITE,
                        ..default()
                    },
                },
                TextSection {
                    value: " N/A".into(),
                    style: TextStyle {
                        font_size: 16.0,
                        color: Color::WHITE,
                        ..default()
                    },
                },
            ]),
            ..Default::default()
        },
    ));

    commands.spawn((
        SpeedText,
        TextBundle {
            text: Text::from_sections([
                TextSection {
                    value: "Speed: ".into(),
                    style: TextStyle {
                        font_size: 16.0,
                        color: Color::WHITE,
                        ..default()
                    },
                },
                TextSection {
                    value: " N/A".into(),
                    style: TextStyle {
                        font_size: 16.0,
                        color: Color::WHITE,
                        ..default()
                    },
                },
            ]),
            ..Default::default()
        },
    ));
}

fn fps_text_update_system(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    for mut text in &mut query {
        // try to get a "smoothed" FPS value from Bevy
        if let Some(value) = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|fps| fps.smoothed())
        {
            // Format the number as to leave space for 4 digits, just in case,
            // right-aligned and rounded. This helps readability when the
            // number changes rapidly.
            text.sections[1].value = format!("{value:>4.0}");

            // Let's make it extra fancy by changing the color of the
            // text according to the FPS value:
            text.sections[1].style.color = if value >= 120.0 {
                // Above 120 FPS, use green color
                Color::rgb(0.0, 1.0, 0.0)
            } else if value >= 60.0 {
                // Between 60-120 FPS, gradually transition from yellow to green
                Color::rgb((1.0 - (value - 60.0) / (120.0 - 60.0)) as f32, 1.0, 0.0)
            } else if value >= 30.0 {
                // Between 30-60 FPS, gradually transition from red to yellow
                Color::rgb(1.0, ((value - 30.0) / (60.0 - 30.0)) as f32, 0.0)
            } else {
                // Below 30 FPS, use red color
                Color::rgb(1.0, 0.0, 0.0)
            }
        } else {
            // display "N/A" if we can't get a FPS measurement
            // add an extra space to preserve alignment
            text.sections[1].value = " N/A".into();
            text.sections[1].style.color = Color::WHITE;
        }
    }
}

fn speed_text_update_system(
    local_player: Res<LocalPlayer>,
    characters: Query<&Character, Without<SpeedText>>,
    mut query: Query<&mut Text, With<SpeedText>>,
) {
    for mut text in &mut query {
        for character in characters.iter() {
            if character.owner_client_id == local_player.client_id {
                text.sections[1].value = format!("{:.2}", character.velocity.length());
                return;
            }
        }
    }
}
