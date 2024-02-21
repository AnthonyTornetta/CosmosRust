//! Event & its processing for when a player wants to create a ship

use bevy::{
    ecs::{query::Without, system::Res},
    log::info,
    prelude::{in_state, App, Event, EventReader, EventWriter, IntoSystemConfigs, Query, ResMut, Update, With},
};
use bevy_renet::renet::RenetClient;
use cosmos_core::{
    inventory::Inventory,
    item::Item,
    netty::{client_reliable_messages::ClientReliableMessages, cosmos_encoder, NettyChannelClient},
    registry::Registry,
    structure::shared::build_mode::BuildMode,
};

use crate::{
    input::inputs::{CosmosInputs, InputChecker, InputHandler},
    netty::flags::LocalPlayer,
    state::game_state::GameState,
};

#[derive(Debug, Event)]
/// Sent when the client wants the server to create a ship
pub struct CreateShipEvent {
    name: String,
}

fn listener(
    q_inventory: Query<&Inventory, (With<LocalPlayer>, Without<BuildMode>)>,
    items: Res<Registry<Item>>,
    input_handler: InputChecker,
    mut event_writer: EventWriter<CreateShipEvent>,
) {
    // Don't create ships while in build mode
    let Ok(inventory) = q_inventory.get_single() else {
        return;
    };

    if input_handler.check_just_pressed(CosmosInputs::CreateShip) {
        let Some(ship_core) = items.from_id("cosmos:ship_core") else {
            info!("Ship core not registered");
            return;
        };

        if inventory.can_take_item(ship_core, 1) {
            event_writer.send(CreateShipEvent { name: "Cool name".into() });
        } else {
            info!("Does not have ship core");
        }
    }
}

fn event_handler(mut event_reader: EventReader<CreateShipEvent>, mut client: ResMut<RenetClient>) {
    for ev in event_reader.read() {
        client.send_message(
            NettyChannelClient::Reliable,
            cosmos_encoder::serialize(&ClientReliableMessages::CreateShip { name: ev.name.clone() }),
        );
    }
}

pub(super) fn register(app: &mut App) {
    app.add_event::<CreateShipEvent>()
        .add_systems(Update, (listener, event_handler).chain().run_if(in_state(GameState::Playing)));
}
