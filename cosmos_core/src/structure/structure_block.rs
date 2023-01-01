use bevy_inspector_egui::Inspectable;

use crate::{block::Block, registry::Registry};

use super::{chunk::CHUNK_DIMENSIONS, Structure};

#[derive(Clone, Debug, Inspectable, Copy, PartialEq, Eq, Default)]
pub struct StructureBlock {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl StructureBlock {
    #[inline]
    pub fn x(&self) -> usize {
        self.x
    }
    #[inline]
    pub fn y(&self) -> usize {
        self.y
    }
    #[inline]
    pub fn z(&self) -> usize {
        self.z
    }

    pub fn new(x: usize, y: usize, z: usize) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn block_id(&self, structure: &Structure) -> u16 {
        structure.block_id_at(self.x, self.y, self.z)
    }

    #[inline]
    pub fn block<'a>(&self, structure: &Structure, blocks: &'a Registry<Block>) -> &'a Block {
        blocks.from_numeric_id(self.block_id(structure))
    }

    #[inline]
    pub fn chunk_coord_x(&self) -> usize {
        self.x / CHUNK_DIMENSIONS
    }

    #[inline]
    pub fn chunk_coord_y(&self) -> usize {
        self.y / CHUNK_DIMENSIONS
    }

    #[inline]
    pub fn chunk_coord_z(&self) -> usize {
        self.z / CHUNK_DIMENSIONS
    }
}
