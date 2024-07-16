use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use bevy_renet2::renet2::RenetServer;
use cosmos_core::{
    netty::{
        cosmos_encoder,
        netty_rigidbody::{NettyRigidBody, NettyRigidBodyLocation},
        server_reliable_messages::ServerReliableMessages,
        sync::server_entity_syncing::RequestedEntityEvent,
        system_sets::NetworkingSystemsSet,
        NettyChannelServer,
    },
    physics::location::Location,
    structure::{ship::Ship, Structure},
};

use crate::state::GameState;

fn on_request_ship(
    mut event_reader: EventReader<RequestedEntityEvent>,
    query: Query<(&Structure, &Transform, &Location, &Velocity), With<Ship>>,
    mut server: ResMut<RenetServer>,
) {
    for ev in event_reader.read() {
        if let Ok((structure, transform, location, velocity)) = query.get(ev.entity) {
            // server.send_message(
            //     ev.client_id,
            //     NettyChannelServer::Reliable,
            //     cosmos_encoder::serialize(&ServerReliableMessages::NumberOfChunks {
            //         entity: ev.entity,
            //         chunks_needed: ChunksNeedLoaded {
            //             amount_needed: structure.all_chunks_iter(false).len(),
            //         },
            //     }),
            // );

            server.send_message(
                ev.client_id,
                NettyChannelServer::Reliable,
                cosmos_encoder::serialize(&ServerReliableMessages::Ship {
                    entity: ev.entity,
                    body: NettyRigidBody::new(Some(*velocity), transform.rotation, NettyRigidBodyLocation::Absolute(*location)),
                    dimensions: structure.chunk_dimensions(),
                }),
            );
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        on_request_ship
            .in_set(NetworkingSystemsSet::Between)
            .run_if(in_state(GameState::Playing)),
    );
}
