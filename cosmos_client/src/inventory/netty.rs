//! Syncs the inventories with the server-provided inventories

use bevy::{
    ecs::query::With,
    log::warn,
    prelude::{in_state, App, Commands, Entity, IntoSystemConfigs, Query, Res, ResMut, Update},
};
use bevy_renet::renet::RenetClient;
use cosmos_core::{
    block::data::BlockData,
    ecs::NeedsDespawned,
    inventory::{
        netty::{InventoryIdentifier, ServerInventoryMessages},
        Inventory,
    },
    netty::{client::LocalPlayer, cosmos_encoder, sync::mapping::NetworkMapping, NettyChannelServer},
    structure::Structure,
};

use crate::state::game_state::GameState;

use super::{HeldItemStack, InventorySide, NeedsDisplayed};

fn sync(
    mut client: ResMut<RenetClient>,
    network_mapping: Res<NetworkMapping>,
    mut commands: Commands,
    mut held_item_query: Query<(Entity, &mut HeldItemStack)>,
    mut structure_query: Query<&mut Structure>,
    local_player: Query<Entity, With<LocalPlayer>>,
    q_check_inventory: Query<(), With<Inventory>>,
    mut q_block_data: Query<&mut BlockData>,
) {
    while let Some(message) = client.receive_message(NettyChannelServer::Inventory) {
        let msg: ServerInventoryMessages = cosmos_encoder::deserialize(&message).expect("Failed to deserialize server inventory message!");

        match msg {
            ServerInventoryMessages::UpdateInventory { mut inventory, owner } => {
                for is in inventory.iter_mut().flat_map(|x| x) {
                    let Some(de) = is.data_entity() else {
                        continue;
                    };

                    let Some(mapped_entity) = network_mapping.client_from_server(&de) else {
                        warn!("Missing data entity for is!");
                        is.set_data_entity(None);
                        continue;
                    };

                    is.set_data_entity(Some(mapped_entity));
                }

                match owner {
                    InventoryIdentifier::Entity(owner) => {
                        if let Some(client_entity) = network_mapping.client_from_server(&owner) {
                            inventory.set_self_entity(client_entity, &mut commands);

                            if let Some(mut ecmds) = commands.get_entity(client_entity) {
                                ecmds.insert(inventory);
                            }
                        } else {
                            warn!("Error: unrecognized entity {owner:?} received from server when trying to sync up inventories!");
                        }
                    }
                    InventoryIdentifier::BlockData(block_data) => {
                        let Some(client_entity) = network_mapping.client_from_server(&block_data.structure_entity) else {
                            warn!(
                                "Error: unrecognized entity {:?} received from server when trying to sync up inventories!",
                                block_data.structure_entity
                            );
                            continue;
                        };

                        inventory.set_self_entity(client_entity, &mut commands);

                        let Ok(mut structure) = structure_query.get_mut(client_entity) else {
                            continue;
                        };

                        let coords = block_data.block.coords();

                        structure.insert_block_data(coords, inventory, &mut commands, &mut q_block_data, &q_check_inventory);
                    }
                }
            }
            ServerInventoryMessages::HeldItemstack { itemstack } => {
                if let Ok((entity, mut holding_itemstack)) = held_item_query.get_single_mut() {
                    if let Some(mut is) = itemstack {
                        // Don't trigger change detection unless it actually changed
                        if is.quantity() != holding_itemstack.quantity() || is.item_id() != holding_itemstack.item_id() {
                            if let Some(de) = is.data_entity() {
                                if let Some(de) = network_mapping.client_from_server(&de) {
                                    is.set_data_entity(Some(de));
                                } else {
                                    warn!("Missing data entity for is!");
                                    is.set_data_entity(None);
                                }
                            }

                            *holding_itemstack = is;
                        }
                    } else {
                        commands.entity(entity).insert(NeedsDespawned);
                    }
                }
            }
            ServerInventoryMessages::OpenInventory { owner } => {
                match owner {
                    InventoryIdentifier::Entity(owner) => {
                        if let Some(client_entity) = network_mapping.client_from_server(&owner) {
                            if let Some(mut ecmds) = commands.get_entity(client_entity) {
                                ecmds.insert(NeedsDisplayed::default());
                            }
                        } else {
                            warn!("Error: unrecognized entity {owner:?} received from server when trying to sync up inventories!");
                        }
                    }
                    InventoryIdentifier::BlockData(block_data) => {
                        let Some(client_entity) = network_mapping.client_from_server(&block_data.structure_entity) else {
                            warn!(
                                "Error: unrecognized entity {:?} received from server when trying to sync up inventories!",
                                block_data.structure_entity
                            );
                            continue;
                        };

                        let Ok(structure) = structure_query.get(client_entity) else {
                            warn!("Tried to open inventory of unknown structure");
                            continue;
                        };

                        let coords = block_data.block.coords();

                        let Some(data_entity) = structure.block_data(coords) else {
                            warn!("Tried to open inventory of block without any client-side block data.");
                            continue;
                        };

                        if !q_check_inventory.contains(data_entity) {
                            warn!("Tried to open inventory of block with block data but without an inventory component!");
                            continue;
                        }

                        commands.entity(data_entity).insert(NeedsDisplayed::default());
                    }
                }

                commands.entity(local_player.single()).insert(NeedsDisplayed(InventorySide::Left));
            }
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(Update, sync.run_if(in_state(GameState::Playing)));
}
