//! Used to iterate over the blocks or chunks of a structure.

use bevy::utils::hashbrown::hash_map;

use super::{
    chunk::{Chunk, CHUNK_DIMENSIONS},
    coordinates::{
        BlockCoordinate, ChunkBlockCoordinate, ChunkCoordinate, Coordinate, UnboundBlockCoordinate, UnboundChunkCoordinate,
        UnboundCoordinateType,
    },
    structure_block::StructureBlock,
    Structure,
};

#[derive(Debug, Clone)]
struct Body<'a, T: Coordinate> {
    start: T,
    end: T,
    at: T,

    structure: &'a Structure,
}

#[derive(Debug, Clone)]
struct EmptyBody<'a, T: Coordinate> {
    chunk_itr: ChunkIterator<'a>,
    cur_chunk: &'a Chunk,

    body: Body<'a, T>,
}

#[derive(Debug, Clone)]
enum BlockItrState<'a, T: Coordinate> {
    ExcludeEmpty(EmptyBody<'a, T>),
    IncludeEmpty(Body<'a, T>),
    Invalid,
}

#[derive(Debug, Clone)]
struct ExcludeEmptyBody {
    start: ChunkCoordinate,
    end: ChunkCoordinate,
}

#[derive(Debug, Clone)]
enum ChunkItrState<'a, T: Coordinate> {
    ExcludeEmpty((ExcludeEmptyBody, hash_map::Iter<'a, usize, Chunk>)),
    IncludeEmpty(Body<'a, T>),
    Invalid,
}

/// Iterates over the blocks of a structure
#[derive(Clone, Debug)]
pub struct BlockIterator<'a> {
    state: BlockItrState<'a, BlockCoordinate>,
}

impl<'a> BlockIterator<'a> {
    /// ALL Coordinates are inclusive!
    ///
    /// * `include_empty` - If this is true, air blocks will be included. If false, air blocks will be excluded so some optimizations can be used.
    pub fn new(mut start: UnboundBlockCoordinate, mut end: UnboundBlockCoordinate, include_empty: bool, structure: &'a Structure) -> Self {
        end.x = end.x.min(structure.blocks_width() as UnboundCoordinateType - 1);
        end.y = end.y.min(structure.blocks_height() as UnboundCoordinateType - 1);
        end.z = end.z.min(structure.blocks_length() as UnboundCoordinateType - 1);

        let Ok(end) = BlockCoordinate::try_from(end) else {
            return Self {
                state: BlockItrState::Invalid
            };
        };

        start.x = start.x.max(0);
        start.y = start.y.max(0);
        start.z = start.z.max(0);

        let Ok(start) = BlockCoordinate::try_from(start) else {
            return Self {
                state: BlockItrState::Invalid
            };
        };

        if !structure.is_within_blocks(start) {
            Self {
                state: BlockItrState::Invalid,
            }
        } else {
            let body = Body {
                start,
                end,
                at: start,
                structure,
            };

            Self {
                state: if include_empty {
                    BlockItrState::IncludeEmpty(body)
                } else {
                    let mut chunk_itr = structure.chunk_iter(
                        UnboundChunkCoordinate::for_unbound_block_coordinate(start.into()),
                        UnboundChunkCoordinate::for_unbound_block_coordinate(end.into()),
                        false,
                    );
                    let cur_chunk = chunk_itr.next();

                    if let Some(ChunkIteratorResult::FilledChunk { position: _, chunk }) = cur_chunk {
                        BlockItrState::ExcludeEmpty(EmptyBody {
                            chunk_itr,
                            cur_chunk: chunk,
                            body,
                        })
                    } else {
                        BlockItrState::Invalid
                    }
                },
            }
        }
    }

    /// Returns true if there are no blocks to iterate through with respect to the `include_empty`, false if not.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of blocks left to iterate through, with respect to the `include_empty` flag.
    pub fn len(&self) -> usize {
        match &self.state {
            BlockItrState::IncludeEmpty(body) => {
                ((body.end.x - body.start.x) * (body.end.y - body.start.y) * (body.end.z - body.start.z)) as usize
            }
            BlockItrState::ExcludeEmpty(_) => self.clone().count(),
            BlockItrState::Invalid => 0,
        }
    }
}

impl<'a> Iterator for BlockIterator<'a> {
    type Item = StructureBlock;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            BlockItrState::Invalid => None,
            BlockItrState::IncludeEmpty(body) => {
                if body.at.z > body.end.z {
                    return None;
                }

                let position = body.at;

                body.at.x += 1;

                if body.at.x > body.end.x {
                    body.at.x = body.start.x;

                    body.at.y += 1;

                    if body.at.y > body.end.y {
                        body.at.y = body.start.y;

                        body.at.z += 1;
                    }
                }

                Some(StructureBlock::new(position))
            }
            BlockItrState::ExcludeEmpty(body) => {
                let first_block_coordinate = body.cur_chunk.chunk_coordinates().first_structure_block();

                let structure_x = body.body.at.x + first_block_coordinate.x;
                let structure_y = body.body.at.y + first_block_coordinate.y;
                let structure_z = body.body.at.z + first_block_coordinate.z;

                if structure_x < body.body.start.x {
                    body.body.at.x = structure_x - first_block_coordinate.x;
                }
                if structure_y < body.body.start.y {
                    body.body.at.y = structure_y - first_block_coordinate.y;
                }
                if structure_z < body.body.start.z {
                    body.body.at.z = structure_z - first_block_coordinate.z;
                }

                if body.body.at.x >= CHUNK_DIMENSIONS || body.body.at.y >= CHUNK_DIMENSIONS || body.body.at.z >= CHUNK_DIMENSIONS {
                    if let Some(chunk) = body.chunk_itr.next() {
                        if let ChunkIteratorResult::FilledChunk { position: _, chunk } = chunk {
                            body.cur_chunk = chunk;
                            body.body.at.x = 0;
                            body.body.at.y = 0;
                            body.body.at.z = 0;
                        } else {
                            panic!("This should never happen.");
                        }
                    } else {
                        self.state = BlockItrState::Invalid;
                        return None;
                    }
                }

                while !body
                    .cur_chunk
                    .has_block_at(ChunkBlockCoordinate::new(body.body.at.x, body.body.at.y, body.body.at.z))
                {
                    if advance_body(body) {
                        self.state = BlockItrState::Invalid;
                        return None;
                    }
                }

                let to_return = Some(StructureBlock::new(BlockCoordinate::new(
                    body.body.at.x + body.cur_chunk.structure_x() * CHUNK_DIMENSIONS,
                    body.body.at.y + body.cur_chunk.structure_y() * CHUNK_DIMENSIONS,
                    body.body.at.z + body.cur_chunk.structure_z() * CHUNK_DIMENSIONS,
                )));

                if advance_body(body) {
                    self.state = BlockItrState::Invalid;
                }

                to_return
            }
        }
    }
}

/// Returns true if there are no available chunks left
fn advance_body(body: &mut EmptyBody<BlockCoordinate>) -> bool {
    body.body.at.x += 1;
    if body.body.at.x >= CHUNK_DIMENSIONS {
        body.body.at.x = 0;

        body.body.at.y += 1;
        if body.body.at.y >= CHUNK_DIMENSIONS {
            body.body.at.y = 0;

            body.body.at.z += 1;
            if body.body.at.z >= CHUNK_DIMENSIONS {
                body.body.at.z = 0;

                if let Some(chunk) = body.chunk_itr.next() {
                    if let ChunkIteratorResult::FilledChunk { position: _, chunk } = chunk {
                        body.cur_chunk = chunk;

                        let (cx, cy, cz) = (
                            body.cur_chunk.structure_x() * CHUNK_DIMENSIONS,
                            body.cur_chunk.structure_y() * CHUNK_DIMENSIONS,
                            body.cur_chunk.structure_z() * CHUNK_DIMENSIONS,
                        );

                        let structure_x = body.body.at.x + cx;
                        let structure_y = body.body.at.y + cy;
                        let structure_z = body.body.at.z + cz;

                        if structure_x < body.body.start.x {
                            body.body.at.x = structure_x - cx;
                        }
                        if structure_y < body.body.start.y {
                            body.body.at.y = structure_y - cy;
                        }
                        if structure_z < body.body.start.z {
                            body.body.at.z = structure_z - cz;
                        }
                    } else {
                        panic!("This should never happen.");
                    }
                } else {
                    return true;
                }
            }
        }
    }

    false
}

/// Chunk Iterator

/// Iterates over the chunks of a structure
///
/// * `include_empty` - If enabled, the value iterated over may be None OR Some(chunk). Otherwise, the value iterated over may ONLY BE Some(chunk).
#[derive(Debug, Clone)]
pub struct ChunkIterator<'a> {
    state: ChunkItrState<'a, ChunkCoordinate>,
}

impl<'a> ChunkIterator<'a> {
    /// Iterates over the chunks of a structure
    /// Coordinates are invlusive!
    ///
    /// * `include_empty` - If enabled, the value iterated over may be `ChunkIteratorResult::EmptyChunk` OR `ChunkIteratorResult::FilledChunk`. Otherwise, the value iterated over may ONLY BE `ChunkIteratorResult::LoadedChunk`.
    pub fn new(mut start: UnboundChunkCoordinate, mut end: UnboundChunkCoordinate, structure: &'a Structure, include_empty: bool) -> Self {
        end.x = end.x.min(structure.chunks_width() as UnboundCoordinateType - 1);
        end.y = end.y.min(structure.chunks_length() as UnboundCoordinateType - 1);
        end.z = end.z.min(structure.chunks_height() as UnboundCoordinateType - 1);

        let Ok(end) = ChunkCoordinate::try_from(end) else {
            return Self {
                state: ChunkItrState::Invalid
            };
        };

        start.x = start.x.max(0);
        start.y = start.y.max(0);
        start.z = start.z.max(0);

        let Ok(start) = ChunkCoordinate::try_from(start) else {
            return Self {
                state: ChunkItrState::Invalid
            };
        };

        if !structure.chunk_coords_within(start) {
            Self {
                state: ChunkItrState::Invalid,
            }
        } else {
            Self {
                state: if include_empty {
                    ChunkItrState::IncludeEmpty(Body {
                        start,
                        end,
                        at: start,
                        structure,
                    })
                } else {
                    ChunkItrState::ExcludeEmpty((ExcludeEmptyBody { start, end }, structure.chunks().iter()))
                },
            }
        }
    }

    /// Returns true if there are no chunks to iterate through with respect to the `include_empty`, false if not.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of chunks left to iterate through, with respect to the `include_empty` flag.
    pub fn len(&self) -> usize {
        match &self.state {
            ChunkItrState::IncludeEmpty(body) => {
                ((body.end.x - body.start.x + 1) * (body.end.y - body.start.y + 1) * (body.end.z - body.start.z + 1)) as usize
            }
            ChunkItrState::ExcludeEmpty((_, itr)) => itr.len(),
            ChunkItrState::Invalid => 0,
        }
    }
}

/// The result of the chunk iterator
///
/// If `include_empty` is true, this may return either variant (`ChunkIteratorResult::FilledChunk` or `ChunkIteratorResult::EmptyChunk`).
/// If this is false, it will only return `ChunkIteratorResult::FilledChunk`.
pub enum ChunkIteratorResult<'a> {
    /// This represents a chunk that has no blocks in it, and is thus unloaded.
    EmptyChunk {
        /// That chunk's position in the structure, can be used in `Structure::chunk_from_chunk_coordinates` once it is loaded.
        position: ChunkCoordinate,
    },
    /// This represents a chunk that does have blocks in it, and is loaded.
    FilledChunk {
        /// That chunk's position in the structure, can be used in `Structure::chunk_from_chunk_coordinates`.
        position: ChunkCoordinate,
        /// The loaded chunk.
        chunk: &'a Chunk,
    },
}

impl<'a> Iterator for ChunkIterator<'a> {
    type Item = ChunkIteratorResult<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            ChunkItrState::Invalid => None,
            ChunkItrState::IncludeEmpty(body) => {
                if body.at.z > body.end.z {
                    self.state = ChunkItrState::Invalid;
                    return None;
                }

                let position = body.at;

                body.at.x += 1;

                if body.at.x > body.end.x {
                    body.at.x = body.start.x;

                    body.at.y += 1;

                    if body.at.y > body.end.y {
                        body.at.y = body.start.y;

                        body.at.z += 1;
                    }
                }

                if let Some(chunk) = body.structure.chunk_from_chunk_coordinates(position) {
                    Some(ChunkIteratorResult::FilledChunk { position, chunk })
                } else {
                    Some(ChunkIteratorResult::EmptyChunk { position })
                }
            }
            ChunkItrState::ExcludeEmpty((body, itr)) => {
                for (_, chunk) in itr.by_ref() {
                    let position = chunk.chunk_coordinates();

                    if position.x >= body.start.x
                        && position.x <= body.end.x
                        && position.y >= body.start.y
                        && position.y <= body.end.y
                        && position.z >= body.start.z
                        && position.z <= body.end.z
                    {
                        return Some(ChunkIteratorResult::FilledChunk { position, chunk });
                    }
                }

                self.state = ChunkItrState::Invalid;

                None
            }
        }
    }
}
