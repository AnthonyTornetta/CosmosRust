//! Handles the backbone for blocks that store their own data, such as containers

use bevy::{
    app::{App, PostUpdate, Update},
    core::Name,
    ecs::{
        component::Component,
        entity::Entity,
        query::{Changed, Without},
        system::{Commands, Query},
    },
    reflect::Reflect,
};
use serde::{Deserialize, Serialize};

use crate::{
    ecs::NeedsDespawned,
    structure::{coordinates::ChunkBlockCoordinate, structure_block::StructureBlock, Structure},
};

#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize, Reflect)]
/// This component indicates an entity that is storing data for a specific block
pub struct BlockData {
    /// The block this data is for
    pub block: StructureBlock,
    /// The structure this block is a part of
    pub structure_entity: Entity,
    /// The number of important pieces of data. Used to determine when to remove this data
    ///
    /// Use `self::increment` and `self::decrement` to manage this. Make sure to call these when you add/remove data appropriately!
    pub data_count: usize,
}

impl BlockData {
    /// Do this whenever you add a piece of data from this block. Removing the data entity will be automatically handled for you if you do this right.
    pub fn increment(&mut self) {
        self.data_count += 1;
    }

    /// Do this whenever you remove a piece of data from this block. Removing the data entity will be automatically handled for you if you do this right.
    pub fn decrement(&mut self) {
        assert_ne!(self.data_count, 0);
        self.data_count -= 1;
    }
}

fn despawn_dead_data(
    mut commands: Commands,
    mut q_structure: Query<&mut Structure>,
    query: Query<(Entity, &BlockData), Changed<BlockData>>,
) {
    query.for_each(|(ent, block_data)| {
        if block_data.data_count == 0 {
            if let Ok(mut structure) = q_structure.get_mut(block_data.structure_entity) {
                structure.remove_block_data(block_data.block.coords());
            }

            commands.entity(ent).insert(NeedsDespawned);
        }
    });
}

fn name_block_data(query: Query<(Entity, &BlockData), Without<Name>>, mut commands: Commands) {
    for (ent, data) in query.iter() {
        commands.entity(ent).insert(Name::new(format!(
            "BlockData for Block @ {}",
            ChunkBlockCoordinate::for_block_coordinate(data.block.coords())
        )));
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(PostUpdate, despawn_dead_data)
        .add_systems(Update, name_block_data)
        .register_type::<BlockData>();
}