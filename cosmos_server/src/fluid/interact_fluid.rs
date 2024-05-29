use bevy::{
    app::{App, Update},
    ecs::{
        entity::Entity,
        event::EventReader,
        query::{With, Without},
        schedule::{IntoSystemConfigs, OnEnter},
        system::{Commands, Query, Res, ResMut},
    },
    log::{error, info},
};
use cosmos_core::{
    block::{block_events::BlockInteractEvent, data::BlockData, Block},
    fluid::{
        data::{FluidHolder, FluidItemData, StoredBlockFluid},
        registry::Fluid,
    },
    inventory::{
        held_item_slot::HeldItemSlot,
        itemstack::{ItemShouldHaveData, ItemStackData, ItemStackNeedsDataCreated, ItemStackSystemSet},
        Inventory,
    },
    item::Item,
    registry::{create_registry, identifiable::Identifiable, Registry},
    structure::Structure,
};

use crate::state::GameState;

const FLUID_PER_BLOCK: u32 = 1000;

fn on_interact_with_fluid(
    mut ev_reader: EventReader<BlockInteractEvent>,
    q_structure: Query<&Structure>,
    blocks: Res<Registry<Block>>,
    mut q_held_item: Query<(&HeldItemSlot, &mut Inventory)>,
    items: Res<Registry<Item>>,
    fluid_holders: Res<Registry<FluidHolder>>,
    mut q_fluid_data: Query<&mut FluidItemData>,
    fluid_registry: Res<Registry<Fluid>>,
    mut commands: Commands,
) {
    for ev in ev_reader.read() {
        let s_block = ev.block_including_fluids;

        let Ok(structure) = q_structure.get(s_block.structure_entity) else {
            continue;
        };

        let block = structure.block_at(s_block.structure_block.coords(), &blocks);

        // if !block.is_fluid() {
        //     continue;
        // }

        let Some(fluid) = fluid_registry.from_id(block.unlocalized_name()) else {
            continue;
        };

        let Ok((held_item, mut inventory)) = q_held_item.get_mut(ev.interactor) else {
            continue;
        };

        let slot = held_item.slot() as usize;

        let Some(is) = inventory.itemstack_at(slot) else {
            continue;
        };

        let Some(fluid_holder) = fluid_holders.from_id(items.from_numeric_id(is.item_id()).unlocalized_name()) else {
            continue;
        };

        if fluid_holder.convert_to_item_id() != is.item_id() {
            if inventory.decrease_quantity_at(slot, 1, &mut commands) != 0 {
                continue;
            }

            let item = items.from_numeric_id(fluid_holder.convert_to_item_id());
            let fluid_data = FluidItemData::Filled {
                fluid_id: fluid.id(),
                fluid_stored: FLUID_PER_BLOCK.min(fluid_holder.max_capacity()),
            };

            // Attempt to insert item into its original spot, if that fails try to insert it anywhere
            if inventory.insert_item_with_data_at(slot, item, 1, &mut commands, fluid_data) != 0 {
                if inventory.insert_item_with_data(item, 1, &mut commands, fluid_data).1.is_none() {
                    info!("TODO: Throw item because it doesn't fit in inventory");
                }
            }
        } else {
            let Some(mut data) = is.data_entity().map(|x| q_fluid_data.get_mut(x).ok()).flatten() else {
                continue;
            };

            match *data {
                FluidItemData::Empty => {
                    *data = FluidItemData::Filled {
                        fluid_id: fluid.id(),
                        fluid_stored: FLUID_PER_BLOCK.min(fluid_holder.max_capacity()),
                    }
                }
                FluidItemData::Filled { fluid_id, fluid_stored } => {
                    if fluid_id != fluid.id() {
                        continue;
                    }

                    *data = FluidItemData::Filled {
                        fluid_id: fluid.id(),
                        fluid_stored: (fluid_stored + FLUID_PER_BLOCK).min(fluid_holder.max_capacity()),
                    }
                }
            }
        };
    }
}

#[derive(Clone)]
/// This block is a fluid tank, and can store fluid
pub struct FluidTankBlock {
    id: u16,
    unlocalized_name: String,
    max_capacity: u32,
}

impl FluidTankBlock {
    /// Indicates that this block can store fluids
    pub fn new(block: &Block, max_capacity: u32) -> Self {
        Self {
            id: 0,
            max_capacity,
            unlocalized_name: block.unlocalized_name().to_owned(),
        }
    }

    /// The maximimum capacity that this block can store of fluids.
    pub fn max_capacity(&self) -> u32 {
        self.max_capacity
    }
}

impl Identifiable for FluidTankBlock {
    fn id(&self) -> u16 {
        self.id
    }

    fn set_numeric_id(&mut self, id: u16) {
        self.id = id;
    }

    fn unlocalized_name(&self) -> &str {
        &self.unlocalized_name
    }
}

fn on_interact_with_tank(
    mut ev_reader: EventReader<BlockInteractEvent>,
    mut q_structure: Query<&mut Structure>,
    blocks: Res<Registry<Block>>,
    mut q_held_item: Query<(&HeldItemSlot, &mut Inventory)>,
    items: Res<Registry<Item>>,
    fluid_holders: Res<Registry<FluidHolder>>,
    mut q_fluid_data_is: Query<&mut FluidItemData>,
    tank_registry: Res<Registry<FluidTankBlock>>,
    mut commands: Commands,
    mut q_stored_fluid_block: Query<&mut StoredBlockFluid>,
    mut q_block_data: Query<&mut BlockData>,
    q_has_stored_fluid: Query<(), With<StoredBlockFluid>>,
    needs_data: Res<ItemShouldHaveData>,
) {
    for ev in ev_reader.read() {
        let Some(s_block) = ev.block else {
            continue;
        };

        let Ok(mut structure) = q_structure.get_mut(s_block.structure_entity) else {
            continue;
        };

        let coords = s_block.structure_block.coords();

        let block = structure.block_at(coords, &blocks);

        let Some(tank_block) = tank_registry.from_id(block.unlocalized_name()) else {
            continue;
        };

        let Ok((held_item, mut inventory)) = q_held_item.get_mut(ev.interactor) else {
            continue;
        };

        let slot = held_item.slot() as usize;

        let Some(is) = inventory.itemstack_at(slot) else {
            continue;
        };

        let Some(fluid_holder) = fluid_holders.from_id(items.from_numeric_id(is.item_id()).unlocalized_name()) else {
            continue;
        };

        let Some(mut stored_fluid_item) = is.query_itemstack_data_mut(&mut q_fluid_data_is) else {
            println!("Stored fluid block");
            let Some(mut stored_fluid_block) = structure.query_block_data_mut(coords, &mut q_stored_fluid_block) else {
                continue;
            };

            println!("Fluid holder not same!");
            if fluid_holder.convert_to_item_id() == is.item_id() {
                continue;
            }

            println!("Decreased qty");
            if inventory.decrease_quantity_at(slot, 1, &mut commands) != 0 {
                continue;
            }

            let item = items.from_numeric_id(fluid_holder.convert_to_item_id());

            let fluid_data = if stored_fluid_block.fluid_stored <= fluid_holder.max_capacity() {
                println!("Filled to not max cap");
                let block_data = *stored_fluid_block;

                structure.remove_block_data::<StoredBlockFluid>(coords, &mut commands, &mut q_block_data, &q_has_stored_fluid);

                FluidItemData::Filled {
                    fluid_id: block_data.fluid_id,
                    fluid_stored: block_data.fluid_stored,
                }
            } else {
                println!("Filled to max cap");
                stored_fluid_block.fluid_stored -= fluid_holder.max_capacity();

                FluidItemData::Filled {
                    fluid_id: stored_fluid_block.fluid_id,
                    fluid_stored: fluid_holder.max_capacity(),
                }
            };

            // Attempt to insert item into its original spot, if that fails try to insert it anywhere
            if inventory.insert_item_with_data_at(slot, item, 1, &mut commands, fluid_data) != 0 {
                if inventory.insert_item_with_data(item, 1, &mut commands, fluid_data).1.is_none() {
                    info!("TODO: Throw item because it doesn't fit in inventory");
                }
            }

            continue;
        };

        match *stored_fluid_item {
            FluidItemData::Empty => {
                if let Some(mut stored_fluid_block) = structure.query_block_data_mut(coords, &mut q_stored_fluid_block) {
                    if stored_fluid_block.fluid_stored <= fluid_holder.max_capacity() {
                        *stored_fluid_item = FluidItemData::Filled {
                            fluid_id: stored_fluid_block.fluid_id,
                            fluid_stored: stored_fluid_block.fluid_stored,
                        };

                        structure.remove_block_data::<StoredBlockFluid>(coords, &mut commands, &mut q_block_data, &q_has_stored_fluid);
                    } else {
                        *stored_fluid_item = FluidItemData::Filled {
                            fluid_id: stored_fluid_block.fluid_id,
                            fluid_stored: fluid_holder.max_capacity(),
                        };

                        stored_fluid_block.fluid_stored -= fluid_holder.max_capacity();
                    }
                }
            }
            FluidItemData::Filled { fluid_id, fluid_stored } => {
                if !ev.alternate {
                    let cur_fluid = structure.query_block_data(coords, &q_stored_fluid_block);

                    // Insert fluid into tank
                    let (data, left_over) = if let Some(cur_fluid) = cur_fluid {
                        if fluid_id != cur_fluid.fluid_id {
                            continue;
                        }

                        let prev_amount = cur_fluid.fluid_stored;

                        let data = StoredBlockFluid {
                            fluid_stored: tank_block.max_capacity().min(fluid_stored + cur_fluid.fluid_stored),
                            fluid_id,
                        };

                        (data, fluid_stored - (data.fluid_stored - prev_amount))
                    } else {
                        let data = StoredBlockFluid {
                            fluid_stored: tank_block.max_capacity().min(fluid_stored),
                            fluid_id,
                        };
                        (data, fluid_stored - data.fluid_stored)
                    };

                    if left_over > 0 {
                        *stored_fluid_item = FluidItemData::Filled {
                            fluid_id,
                            fluid_stored: left_over,
                        };
                    } else {
                        *stored_fluid_item = FluidItemData::Empty;
                    }

                    structure.insert_block_data(coords, data, &mut commands, &mut q_block_data, &q_has_stored_fluid);

                    if matches!(*stored_fluid_item, FluidItemData::Empty) && fluid_holder.convert_from_item_id() != is.item_id() {
                        if inventory.decrease_quantity_at(slot, 1, &mut commands) != 0 {
                            error!("Items with data stacked?");
                            continue;
                        }

                        let item = items.from_numeric_id(fluid_holder.convert_from_item_id());

                        // Attempt to insert item into its original spot, if that fails try to insert it anywhere
                        if inventory.insert_item_at(slot, item, 1, &mut commands, &needs_data) != 0 {
                            if inventory.insert_item(item, 1, &mut commands, &needs_data).1.is_none() {
                                info!("TODO: Throw item because it doesn't fit in inventory");
                            }
                        }
                    }
                } else if let Some(mut stored_fluid_block) = structure.query_block_data_mut(coords, &mut q_stored_fluid_block) {
                    // Put fluid into item
                    if stored_fluid_block.fluid_id != fluid_id {
                        continue;
                    }

                    if stored_fluid_block.fluid_stored <= fluid_holder.max_capacity() - fluid_stored {
                        *stored_fluid_item = FluidItemData::Filled {
                            fluid_id,
                            fluid_stored: fluid_stored + stored_fluid_block.fluid_stored,
                        };

                        structure.remove_block_data::<StoredBlockFluid>(coords, &mut commands, &mut q_block_data, &q_has_stored_fluid);
                    } else {
                        let delta = fluid_holder.max_capacity() - fluid_stored;

                        // Avoid change detection if not needed
                        if delta != 0 {
                            *stored_fluid_item = FluidItemData::Filled {
                                fluid_id,
                                fluid_stored: fluid_holder.max_capacity(),
                            };

                            stored_fluid_block.fluid_stored -= delta;
                        }
                    }
                }
            }
        }
    }
}

fn add_item_fluid_data(
    q_needs_data: Query<(Entity, &ItemStackData), (Without<FluidItemData>, With<ItemStackNeedsDataCreated>)>,
    mut commands: Commands,
    items: Res<Registry<Item>>,
    fluid_holders: Res<Registry<FluidHolder>>,
) {
    for (ent, is_data) in q_needs_data.iter() {
        let item = items.from_numeric_id(is_data.item_id);

        if !fluid_holders.contains(item.unlocalized_name()) {
            continue;
        };

        commands.entity(ent).insert(FluidItemData::Empty);
    }
}

fn register_fluid_holder_items(
    items: Res<Registry<Item>>,
    mut needs_data: ResMut<ItemShouldHaveData>,
    mut fluid_holders: ResMut<Registry<FluidHolder>>,
) {
    if let Some(fluid_cell_filled) = items.from_id("cosmos:fluid_cell_filled") {
        if let Some(fluid_cell) = items.from_id("cosmos:fluid_cell") {
            fluid_holders.register(FluidHolder::new(fluid_cell_filled, fluid_cell_filled, fluid_cell, 10_000));
            needs_data.add_item(fluid_cell_filled);

            fluid_holders.register(FluidHolder::new(fluid_cell, fluid_cell_filled, fluid_cell, 10_000));
        }
    }
}

fn fill_tank_registry(mut tank_reg: ResMut<Registry<FluidTankBlock>>, blocks: Res<Registry<Block>>) {
    if let Some(tank) = blocks.from_id("cosmos:tank") {
        tank_reg.register(FluidTankBlock::new(tank, 10_000));
    }
}

pub(super) fn register(app: &mut App) {
    create_registry::<FluidTankBlock>(app, "cosmos:tank_block");

    app.add_systems(OnEnter(GameState::PostLoading), (register_fluid_holder_items, fill_tank_registry))
        .add_systems(Update, on_interact_with_tank.before(ItemStackSystemSet::CreateDataEntity))
        .add_systems(Update, add_item_fluid_data.in_set(ItemStackSystemSet::FillDataEntity))
        .add_systems(Update, on_interact_with_fluid.after(ItemStackSystemSet::FillDataEntity));
}