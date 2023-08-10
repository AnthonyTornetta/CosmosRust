//! Contains all the functionality & information related to structures that are fully loaded at all times.
//!
//! This means that all chunks this structure needs are loaded as long as the structure exists.

use bevy::{
    prelude::{Commands, Entity, EventWriter, GlobalTransform, Vec3},
    reflect::Reflect,
    utils::{hashbrown::HashSet, HashMap},
};
use serde::{Deserialize, Serialize};

use crate::{
    block::{blocks::AIR_BLOCK_ID, hardness::BlockHardness, Block, BlockFace},
    ecs::NeedsDespawned,
    events::block_events::BlockChangedEvent,
    physics::location::Location,
    registry::{identifiable::Identifiable, Registry},
};

use super::{
    base_structure::BaseStructure,
    block_health::block_destroyed_event::BlockDestroyedEvent,
    chunk::{Chunk, ChunkUnloadEvent, CHUNK_DIMENSIONS},
    coordinates::{
        BlockCoordinate, ChunkBlockCoordinate, ChunkCoordinate, Coordinate, CoordinateType, UnboundBlockCoordinate, UnboundChunkCoordinate,
    },
    structure_block::StructureBlock,
    structure_iterator::{BlockIterator, ChunkIterator},
    ChunkState, Structure,
};

#[derive(Serialize, Deserialize, Reflect, Debug)]
/// Contains all the functionality & information related to structures that are fully loaded at all times.
///
/// This means that all chunks this structure needs are loaded as long as the structure exists.
pub struct FullStructure {
    base_structure: BaseStructure,
    #[serde(skip)]
    loaded: bool,
}

impl FullStructure {
    pub fn new(dimensions: ChunkCoordinate) -> Self {
        Self {
            base_structure: BaseStructure::new(dimensions),
            loaded: false,
        }
    }

    /// A static version of [`Self::block_relative_position`]. This is useful if you know
    /// the dimensions of the structure, but don't have access to the structure instance.
    ///
    /// Gets the block's relative position to any structure's transform.
    ///
    /// The width, height, and length should be that structure's width, height, and length.
    pub fn block_relative_position_static(
        coords: BlockCoordinate,
        structure_blocks_width: CoordinateType,
        structure_blocks_height: CoordinateType,
        structure_blocks_length: CoordinateType,
    ) -> Vec3 {
        let xoff = structure_blocks_width as f32 / 2.0;
        let yoff = structure_blocks_height as f32 / 2.0;
        let zoff = structure_blocks_length as f32 / 2.0;

        let xx = coords.x as f32 - xoff;
        let yy = coords.y as f32 - yoff;
        let zz = coords.z as f32 - zoff;

        Vec3::new(xx + 0.5, yy + 0.5, zz + 0.5)
    }

    /// Gets the block's relative position to this structure's transform.
    pub fn block_relative_position(&self, coords: BlockCoordinate) -> Vec3 {
        Self::block_relative_position_static(coords, self.blocks_width(), self.blocks_height(), self.blocks_length())
    }

    /// Sets the block at the given block coordinates.
    ///
    /// * `event_writer` If this is `None`, no event will be generated. A valid usecase for this being `None` is when you are initially loading/generating everything and you don't want a billion events being generated.
    pub fn set_block_at(
        &mut self,
        coords: BlockCoordinate,
        block: &Block,
        block_up: BlockFace,
        blocks: &Registry<Block>,
        event_writer: Option<&mut EventWriter<BlockChangedEvent>>,
    ) {
        self.base_structure.debug_assert_block_coords_within(coords);

        let old_block = self.block_id_at(coords);
        if blocks.from_numeric_id(old_block) == block {
            return;
        }

        let chunk_coords = ChunkCoordinate::for_block_coordinate(coords);
        let chunk_block_coords = ChunkBlockCoordinate::for_block_coordinate(coords);
        let mut send_event = false;

        if let Some(chunk) = self.mut_chunk_from_chunk_coordinates(chunk_coords) {
            chunk.set_block_at(chunk_block_coords, block, block_up);

            if chunk.is_empty() {
                self.base_structure.unload_chunk(chunk_coords);
            }

            send_event = true;
        } else if block.id() != AIR_BLOCK_ID {
            let mut chunk = Chunk::new(chunk_coords);
            chunk.set_block_at(chunk_block_coords, block, block_up);
            self.base_structure.chunks.insert(self.base_structure.flatten(chunk_coords), chunk);
            send_event = true;
        }

        if !send_event {
            return;
        }
        let Some(self_entity) = self.base_structure.self_entity else {
            return;
        };
        let Some(event_writer) = event_writer else {
            return;
        };

        event_writer.send(BlockChangedEvent {
            new_block: block.id(),
            old_block,
            structure_entity: self_entity,
            block: StructureBlock::new(coords),
            old_block_up: self.block_rotation(coords),
            new_block_up: block_up,
        });
    }

    /// Removes the block at the given coordinates
    ///
    /// * `event_writer` If this is None, no event will be generated.
    pub fn remove_block_at(
        &mut self,
        coords: BlockCoordinate,
        blocks: &Registry<Block>,
        event_writer: Option<&mut EventWriter<BlockChangedEvent>>,
    ) {
        self.set_block_at(coords, blocks.from_numeric_id(AIR_BLOCK_ID), BlockFace::Top, blocks, event_writer);
    }

    /// Marks this structure as being completely loaded
    pub fn set_loaded(&mut self) {
        self.loaded = true;
    }

    /// Returns the chunk's state
    pub fn get_chunk_state(&self, coords: ChunkCoordinate) -> ChunkState {
        if !self.is_within_chunks(coords) {
            ChunkState::Invalid
        } else if self.loaded {
            ChunkState::Loaded
        } else {
            ChunkState::Loading
        }
    }

    fn is_within_chunks(&self, coords: ChunkCoordinate) -> bool {
        let (w, h, l) = self.block_dimensions().into();

        coords.x < w && coords.y < h && coords.z < l
    }

    /// Returns if the chunk at these chunk coordinates is fully loaded & empty.
    pub fn has_empty_chunk_at(&self, coords: ChunkCoordinate) -> bool {
        self.get_chunk_state(coords) == ChunkState::Loaded
            && self.chunk_from_chunk_coordinates(coords).map(|c| c.is_empty()).unwrap_or(true)
    }

    pub fn chunk_from_entity(&self, entity: &Entity) -> Option<&Chunk> {
        self.base_structure.chunk_from_entity(entity)
    }

    pub fn set_entity(&mut self, entity: bevy::prelude::Entity) {
        self.base_structure.set_entity(entity)
    }

    pub fn get_entity(&self) -> Option<Entity> {
        self.base_structure.get_entity()
    }

    pub fn chunk_from_chunk_coordinates(&self, coords: ChunkCoordinate) -> Option<&Chunk> {
        self.base_structure.chunk_from_chunk_coordinates(coords)
    }

    pub fn chunk_from_chunk_coordinates_unbound(&self, unbound_coords: UnboundChunkCoordinate) -> Option<&Chunk> {
        self.base_structure.chunk_from_chunk_coordinates_unbound(unbound_coords)
    }

    pub fn mut_chunk_from_chunk_coordinates(&mut self, coords: ChunkCoordinate) -> Option<&mut Chunk> {
        self.base_structure.mut_chunk_from_chunk_coordinates(coords)
    }

    pub fn chunk_at_block_coordinates(&self, coords: BlockCoordinate) -> Option<&Chunk> {
        self.base_structure.chunk_at_block_coordinates(coords)
    }

    pub fn is_within_blocks(&self, coords: BlockCoordinate) -> bool {
        self.base_structure.is_within_blocks(coords)
    }

    pub fn has_block_at(&self, coords: BlockCoordinate) -> bool {
        self.base_structure.has_block_at(coords)
    }

    pub fn relative_coords_to_local_coords_checked(&self, x: f32, y: f32, z: f32) -> Result<BlockCoordinate, bool> {
        self.base_structure.relative_coords_to_local_coords_checked(x, y, z)
    }

    pub fn relative_coords_to_local_coords(&self, x: f32, y: f32, z: f32) -> UnboundBlockCoordinate {
        self.base_structure.relative_coords_to_local_coords(x, y, z)
    }

    pub fn block_rotation(&self, coords: BlockCoordinate) -> BlockFace {
        self.base_structure.block_rotation(coords)
    }

    pub fn block_id_at(&self, coords: BlockCoordinate) -> u16 {
        self.base_structure.block_id_at(coords)
    }

    pub fn block_at<'a>(&'a self, coords: BlockCoordinate, blocks: &'a Registry<Block>) -> &'a Block {
        self.base_structure.block_at(coords, blocks)
    }

    pub fn chunks(&self) -> &bevy::utils::hashbrown::HashMap<usize, Chunk> {
        self.base_structure.chunks()
    }

    pub fn chunk_relative_position(&self, coords: ChunkCoordinate) -> Vec3 {
        self.base_structure.chunk_relative_position(coords)
    }

    pub fn block_world_location(&self, coords: BlockCoordinate, body_position: &GlobalTransform, this_location: &Location) -> Location {
        self.base_structure.block_world_location(coords, body_position, this_location)
    }

    pub fn take_chunk(&mut self, coords: ChunkCoordinate) -> Option<Chunk> {
        self.base_structure.take_chunk(coords)
    }

    pub fn all_chunks_iter<'a>(&'a self, structure: &'a Structure, include_empty: bool) -> ChunkIterator {
        self.base_structure.all_chunks_iter(structure, include_empty)
    }

    pub fn chunk_iter<'a>(
        &'a self,
        structure: &'a Structure,
        start: UnboundChunkCoordinate,
        end: UnboundChunkCoordinate,
        include_empty: bool,
    ) -> ChunkIterator {
        self.base_structure.chunk_iter(structure, start, end, include_empty)
    }

    pub fn block_iter_for_chunk<'a>(&'a self, structure: &'a Structure, coords: ChunkCoordinate, include_air: bool) -> BlockIterator {
        self.base_structure.block_iter_for_chunk(structure, coords, include_air)
    }

    pub fn all_blocks_iter<'a>(&'a self, structure: &'a Structure, include_air: bool) -> BlockIterator {
        self.base_structure.all_blocks_iter(structure, include_air)
    }

    pub fn block_iter<'a>(
        &'a self,
        structure: &'a Structure,
        start: UnboundBlockCoordinate,
        end: UnboundBlockCoordinate,
        include_air: bool,
    ) -> BlockIterator {
        self.base_structure.block_iter(structure, start, end, include_air)
    }

    pub fn get_block_health(&self, coords: BlockCoordinate, block_hardness: &crate::block::hardness::BlockHardness) -> f32 {
        self.base_structure.get_block_health(coords, block_hardness)
    }

    pub fn block_take_damage(
        &mut self,
        coords: BlockCoordinate,
        block_hardness: &BlockHardness,
        amount: f32,
        event_writer: Option<&mut EventWriter<BlockDestroyedEvent>>,
    ) -> bool {
        self.base_structure.block_take_damage(coords, block_hardness, amount, event_writer)
    }

    pub fn remove_chunk_entity(&mut self, coords: ChunkCoordinate) {
        self.base_structure.remove_chunk_entity(coords)
    }

    pub fn block_dimensions(&self) -> BlockCoordinate {
        self.base_structure.block_dimensions()
    }

    pub fn chunk_dimensions(&self) -> ChunkCoordinate {
        self.base_structure.chunk_dimensions()
    }

    #[inline(always)]
    /// The number of chunks in the x direction
    pub fn chunks_width(&self) -> CoordinateType {
        self.base_structure.chunks_width()
    }

    #[inline(always)]
    /// The number of chunks in the y direction
    pub fn chunks_height(&self) -> CoordinateType {
        self.base_structure.chunks_height()
    }

    #[inline(always)]
    /// The number of chunks in the z direction
    pub fn chunks_length(&self) -> CoordinateType {
        self.base_structure.chunks_length()
    }

    #[inline(always)]
    /// The number of blocks in the x direction
    pub fn blocks_width(&self) -> CoordinateType {
        self.base_structure.blocks_width()
    }

    #[inline(always)]
    /// The number of blocks in the y direction
    pub fn blocks_height(&self) -> CoordinateType {
        self.base_structure.blocks_height()
    }

    #[inline(always)]
    /// The number of blocks in the z direction
    pub fn blocks_length(&self) -> CoordinateType {
        self.base_structure.blocks_length()
    }

    /// Returns the entity for this chunk -- an empty chunk WILL NOT have an entity.
    ///
    /// If this returns none, that means the chunk entity was not set before being used.
    #[inline(always)]
    pub fn chunk_entity(&self, coords: ChunkCoordinate) -> Option<Entity> {
        self.base_structure.chunk_entity(coords)
    }

    /// Sets the entity for the chunk at those chunk coordinates.
    ///
    /// This should be handled automatically, so you shouldn't have to call this unless
    /// you're doing some crazy stuff.
    pub fn set_chunk_entity(&mut self, coords: ChunkCoordinate, entity: Entity) {
        self.base_structure.set_chunk_entity(coords, entity)
    }

    /// Sets the chunk, overwriting what may have been there before.
    ///
    /// Used generally when loading stuff on client from server.
    ///
    /// This does not trigger any events, so make sure to handle that properly.
    pub fn set_chunk(&mut self, chunk: Chunk) {
        self.base_structure.set_chunk(chunk)
    }

    /// Sets the chunk at this chunk location to be empty (all air).
    ///
    /// Used generally when loading stuff on client from server.
    ///
    /// This does not trigger any events, so make sure to handle those properly.
    pub fn set_to_empty_chunk(&mut self, coords: ChunkCoordinate) {
        self.base_structure.set_to_empty_chunk(coords)
    }

    /// Returns true if these chunk coordinates are within the structure
    pub fn chunk_coords_within(&self, coords: ChunkCoordinate) -> bool {
        self.base_structure.chunk_coords_within(coords)
    }
}