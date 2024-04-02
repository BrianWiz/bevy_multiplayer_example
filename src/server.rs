use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::SystemTime,
};

use crate::core::*;
use bevy::{prelude::*, utils::HashMap};
use bevy_renet::renet::{
    transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig},
    ClientId, ConnectionConfig, DefaultChannel, RenetServer, ServerEvent,
};

pub struct ServerPlugin;
impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_server_system);
        app.add_systems(FixedPreUpdate, handle_connection_events_system);
        app.add_systems(FixedPreUpdate, receive_inputs_system);
        app.add_systems(FixedUpdate, input_processing_system);
        app.add_systems(FixedPostUpdate, snapshot_send_system);
        app.init_resource::<SnapshotHistory>();
        app.init_resource::<PlayerInputCache>();
    }
}

#[derive(Resource, Default)]
struct PlayerInputCache {
    inputs: HashMap<ClientId, PlayerInputCacheEntry>,
}

#[derive(Resource, Default)]
struct PlayerInputCacheEntry {
    input_groups: Vec<Vec<PlayerInput>>,
    latest_processed_input: Option<PlayerInput>,
    client_latest_processed_snapshot_id: Option<u32>,
}

fn start_server_system(mut commands: Commands, server_settings: Res<ServerSettings>) {
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), server_settings.port);
    if let Ok(socket) = UdpSocket::bind(server_addr) {
        let server_config = ServerConfig {
            current_time: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
            max_clients: 64,
            protocol_id: 0,
            public_addresses: vec![server_addr],
            authentication: ServerAuthentication::Unsecure,
        };

        if let Ok(transport) = NetcodeServerTransport::new(server_config, socket) {
            commands.insert_resource(LocalPlayer {
                client_id: ClientId::from_raw(0),
            });
            commands.insert_resource(RenetServer::new(ConnectionConfig::default()));
            commands.insert_resource(transport);
        }
    }
}

fn handle_connection_events_system(
    characters: Query<(&Character, &Transform)>,
    mut spawn_visuals: EventWriter<SpawnCharacterVisualsEvent>,
    mut commands: Commands,
    mut server_events: EventReader<ServerEvent>,
    mut input_buffer: ResMut<PlayerInputCache>,
    mut server: ResMut<RenetServer>,
) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                let start_position = Vec3::new(0.0, 0.0, 0.0);
                let start_velocity = Vec3::ZERO;

                crate::spawn_character(
                    *client_id,
                    &mut spawn_visuals,
                    &mut commands,
                    start_position,
                    start_velocity,
                );

                // tell them to spawn it
                if let Ok(message) = bincode::serialize(&ReliableServerMessage::SpawnCharacter(
                    client_id.raw(),
                    start_position,
                    start_velocity,
                )) {
                    server.send_message(*client_id, DefaultChannel::ReliableUnordered, message);
                }

                // tell them to spawn all existing characters
                for (character, transform) in characters.iter() {
                    if let Ok(message) = bincode::serialize(&ReliableServerMessage::SpawnCharacter(
                        character.owner_client_id.raw(),
                        transform.translation,
                        character.velocity,
                    )) {
                        server.send_message(*client_id, DefaultChannel::ReliableUnordered, message);
                    }
                }
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                println!("Client disconnected: {:?} ({:?})", client_id, reason);
                input_buffer.inputs.remove(client_id);
            }
        }
    }
}

fn receive_inputs_system(
    mut input_buffer: ResMut<PlayerInputCache>,
    mut server: ResMut<RenetServer>,
) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, DefaultChannel::Unreliable) {
            if let Ok(message) = bincode::deserialize::<UnreliableClientMessage>(&message) {
                match message {
                    UnreliableClientMessage::PlayerInputMessage(message) => {
                        let player_inputs =
                            input_buffer.inputs.entry(client_id).or_insert_with(|| {
                                PlayerInputCacheEntry {
                                    input_groups: Vec::new(),
                                    latest_processed_input: None,
                                    client_latest_processed_snapshot_id: None,
                                }
                            });
                        player_inputs.client_latest_processed_snapshot_id =
                            message.latest_processed_snapshot_id;
                        player_inputs.input_groups.push(message.inputs);
                    }
                }
            }
        }
    }
}

fn snapshot_send_system(
    input_buffer: Res<PlayerInputCache>,
    characters: Query<(&Character, &Transform)>,
    mut server: ResMut<RenetServer>,
    mut snapshot_history: ResMut<SnapshotHistory>,
) {
    let mut snapshot = Snapshot {
        id: snapshot_history.next_id,
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        latest_processed_input_id: None,
        character_snapshots: characters
            .iter()
            .map(|(character, transform)| CharacterSnapshot::from_character(character, transform))
            .collect(),
    };

    // retain snapshots up to a second ago
    snapshot_history.snapshots.retain(|snapshot| {
        snapshot.timestamp
            > SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                - 1000
    });

    for client_id in server.clients_id() {
        if let Some(player_inputs) = input_buffer.inputs.get(&client_id) {
            snapshot.latest_processed_input_id =
                if let Some(latest_processed_input) = &player_inputs.latest_processed_input {
                    Some(latest_processed_input.id)
                } else {
                    None
                };

            if let Some(client_latest_processed_snapshot_id) =
                player_inputs.client_latest_processed_snapshot_id
            {
                // make a diff snapshot
                if let Some(old_snapshot) = snapshot_history
                    .snapshots
                    .iter()
                    .find(|old_snapshot| old_snapshot.id == client_latest_processed_snapshot_id)
                {
                    let diff = snapshot.diff(old_snapshot);
                    if let Ok(message) =
                        bincode::serialize(&UnreliableServerMessage::Snapshot(diff))
                    {
                        server.send_message(client_id, DefaultChannel::Unreliable, message);
                    }
                }
                // can't make a diff, latest acked snapshot is too old, send the full latest snapshot
                else {
                    if let Ok(message) =
                        bincode::serialize(&UnreliableServerMessage::Snapshot(snapshot.clone()))
                    {
                        server.send_message(client_id, DefaultChannel::Unreliable, message);
                    }
                }
            // can't make a diff, client never acked a snapshot, send the full latest snapshot
            } else {
                if let Ok(message) =
                    bincode::serialize(&UnreliableServerMessage::Snapshot(snapshot.clone()))
                {
                    server.send_message(client_id, DefaultChannel::Unreliable, message);
                }
            }
        }
    }

    snapshot.latest_processed_input_id = None;
    snapshot_history.snapshots.push(snapshot.clone());
    snapshot_history.next_id += 1;
}

fn input_processing_system(
    fixed_time: Res<Time<Fixed>>,
    mut input_buffer: ResMut<PlayerInputCache>,
    mut characters: Query<(&mut Character, &mut Transform)>,
) {
    for (mut character, mut transform) in characters.iter_mut() {
        if let Some(cache_entry) = input_buffer.inputs.get_mut(&character.owner_client_id) {
            if cache_entry.input_groups.is_empty() {
                for input in cache_entry.latest_processed_input.iter_mut() {
                    character.process_input(input, &mut transform, fixed_time.delta_seconds());
                }
                continue;
            }

            let chopped_delta = fixed_time.delta_seconds() / cache_entry.input_groups.len() as f32;
            for input_group in cache_entry.input_groups.iter_mut() {
                if input_group.is_empty() {
                    continue;
                }
                let even_more_chopped_delta = chopped_delta / input_group.len() as f32;
                for mut input in input_group.iter_mut() {
                    character.process_input(&mut input, &mut transform, even_more_chopped_delta);
                    cache_entry.latest_processed_input = Some(input.clone());
                }
            }
            cache_entry.input_groups.clear();
        }
    }
}
