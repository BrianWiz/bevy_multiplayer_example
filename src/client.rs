use crate::core::*;
use bevy::prelude::*;
use bevy_renet::renet::transport::ClientAuthentication;
use bevy_renet::renet::transport::NetcodeClientTransport;
use bevy_renet::renet::ClientId;
use bevy_renet::renet::ConnectionConfig;
use bevy_renet::renet::DefaultChannel;
use bevy_renet::renet::RenetClient;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::net::UdpSocket;
use std::time::SystemTime;

pub struct ClientPlugin;
impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_client);
        app.add_systems(FixedPostUpdate, send_inputs_system);
        app.add_systems(FixedPreUpdate, receive_snapshot_system);
    }
}

fn start_client(mut commands: Commands, client_settings: Res<ClientSettings>) {
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let client_id = ClientId::from_raw(current_time.as_secs());
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).unwrap();
    if let Ok(transport) = NetcodeClientTransport::new(
        current_time,
        ClientAuthentication::Unsecure {
            server_addr: SocketAddr::new(client_settings.address, client_settings.port),
            client_id: client_id.raw(),
            user_data: None,
            protocol_id: 0,
        },
        socket,
    ) {
        commands.insert_resource(LocalPlayer { client_id });
        commands.insert_resource(RenetClient::new(ConnectionConfig::default()));
        commands.insert_resource(transport);
    }
}

fn send_inputs_system(history: Res<InputHistory>, mut client: ResMut<RenetClient>) {
    if let Ok(encoded) = bincode::serialize(&UnreliableClientMessage::PlayerInputMessage(
        PlayerInputMessage {
            latest_processed_snapshot_id: history.latest_processed_snapshot_id,
            inputs: history.inputs_for_next_send.clone(),
        },
    )) {
        client.send_message(DefaultChannel::Unreliable, encoded);
    }
}

fn receive_snapshot_system(
    fixed_time: Res<Time<Fixed>>,
    local_player: Res<LocalPlayer>,
    mut spawn_visuals: EventWriter<SpawnCharacterVisualsEvent>,
    mut commands: Commands,
    mut characters: Query<(&mut Character, &mut Transform), Without<CharacterVisuals>>,
    mut input_history: ResMut<InputHistory>,
    mut client: ResMut<RenetClient>,
) {
    while let Some(message) = client.receive_message(DefaultChannel::ReliableUnordered) {
        if let Ok(message) = bincode::deserialize::<ReliableServerMessage>(&message) {
            match message {
                ReliableServerMessage::SpawnCharacter(client_id, translation, velocity) => {
                    crate::spawn_character(
                        ClientId::from_raw(client_id),
                        &mut spawn_visuals,
                        &mut commands,
                        translation,
                        velocity,
                    );
                }
            }
        }
    }
    while let Some(message) = client.receive_message(DefaultChannel::Unreliable) {
        if let Ok(message) = bincode::deserialize::<UnreliableServerMessage>(&message) {
            match message {
                UnreliableServerMessage::Snapshot(snapshot) => {
                    let should_process = if let Some(latest_processed_snapshot_id) =
                        input_history.latest_processed_snapshot_id
                    {
                        snapshot.id > latest_processed_snapshot_id
                    } else {
                        true
                    };

                    if !should_process {
                        continue;
                    }

                    input_history.latest_processed_snapshot_id = Some(snapshot.id);

                    for character_snapshot in snapshot.character_snapshots {
                        let client_id = ClientId::from_raw(character_snapshot.client_id);
                        if let Some((mut character, mut character_transform)) = characters
                            .iter_mut()
                            .find(|(character, _)| character.owner_client_id == client_id)
                        {
                            if client_id == local_player.client_id {
                                if character_snapshot.translation.is_some() {
                                    if let Some(latest_processed_input_id) =
                                        snapshot.latest_processed_input_id
                                    {
                                        if let Some(latest_processed_input) = input_history
                                            .input_groups
                                            .iter()
                                            .flat_map(|inputs| inputs.iter())
                                            .find(|input| input.id == latest_processed_input_id)
                                        {
                                            let dist_diff = character_snapshot
                                                .translation
                                                .unwrap()
                                                .distance_squared(
                                                    latest_processed_input.final_translation,
                                                );

                                            if dist_diff > 0.0001 {
                                                let pitch = character.pitch;
                                                let yaw = character.yaw;
                                                // correct the character's position
                                                character_snapshot.apply(
                                                    &mut character,
                                                    &mut character_transform,
                                                );
                                                // replay all input groups since the last processed input
                                                for input_group in
                                                    input_history.input_groups.iter_mut()
                                                {
                                                    let chopped_delta = fixed_time.delta_seconds()
                                                        / input_group.len() as f32;
                                                    for mut input in input_group.iter_mut() {
                                                        if input.id > latest_processed_input_id {
                                                            character.process_input(
                                                                &mut input,
                                                                &mut character_transform,
                                                                chopped_delta,
                                                            );
                                                        }
                                                    }
                                                }

                                                character.pitch = pitch;
                                                character.yaw = yaw;
                                            }
                                        }
                                    }
                                }
                            } else {
                                character_snapshot.apply(&mut character, &mut character_transform);
                            }
                        }
                    }
                }
            }
        }
    }
}
