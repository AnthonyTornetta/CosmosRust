//! Responsible for the default generation of biospheres.

// Fix corners.

use std::{marker::PhantomData, mem::swap};

use bevy::{
    prelude::{Component, Entity, EventReader, EventWriter, Query, Res, ResMut, Resource},
    tasks::AsyncComputeTaskPool,
};
use cosmos_core::{
    block::{Block, BlockFace},
    physics::location::Location,
    registry::Registry,
    structure::{
        chunk::{Chunk, CHUNK_DIMENSIONS},
        planet::{ChunkFaces, Planet},
        Structure,
    },
    utils::{array_utils::flatten_2d, resource_wrapper::ResourceWrapper, timer::UtilsTimer},
};
use futures_lite::future;
use noise::NoiseFn;

use super::{GeneratingChunk, GeneratingChunks, TGenerateChunkEvent};

/// Tells the chunk to generate its features.
pub struct GenerateChunkFeaturesEvent<T: Component> {
    _phantom: PhantomData<T>,
    /// cx, cy, cz.
    pub chunk_coords: (usize, usize, usize),
    /// The structure entity that contains this chunk.
    pub structure_entity: Entity,
}

/// Gets the "y" value of a block on the planet. This "y" value is relative to the face the block is on.
///
/// * `noise_generator` Used to generate noise values. Seeded for this world seed.
/// * `(x, y, z)` Block x/y/z in the structure
/// * `(structure_x, structure_y, structure_z)` Where the structure is in the universe - used to offset the noise values so no two structures are the same.
/// * `(middle_air_start)` The midpoint of the extremes of heights. Aka if noise generates 0, then this should return middle_air_start.
/// * `amplitude` Value passed in by the `GenerationParemeters`. Represents how tall the terrain will be
/// * `delta` Value passed in by the `GenerationParemeters`. Represents how much each change in x/y/z will effect the terrain. Small values = lesser effect
/// * `iterations` Value passed in by the `GenerationParemeters`. Represents how many times the noise function will be run
fn get_block_height(
    noise_generator: &noise::OpenSimplex,
    (bx, by, bz): (usize, usize, usize),
    (structure_x, structure_y, structure_z): (f64, f64, f64),
    middle: usize,
    amplitude: f64,
    delta: f64,
    iterations: usize,
) -> f64 {
    let mut depth: f64 = 0.0;
    for iteration in 1..=iterations {
        let iteration = iteration as f64;
        depth += noise_generator.get([
            (bx as f64 + structure_x) * (delta / iteration),
            (by as f64 + structure_y) * (delta / iteration),
            (bz as f64 + structure_z) * (delta / iteration),
        ]) * amplitude
            * iteration;
    }

    middle as f64 + depth
}

/// Sends a ChunkInitEvent for every chunk that's done generating, monitors when chunks are finished generating.
pub fn notify_when_done_generating_terrain<T: Component>(
    mut generating: ResMut<GeneratingChunks<T>>,
    mut event_writer: EventWriter<GenerateChunkFeaturesEvent<T>>,
    mut structure_query: Query<&mut Structure>,
) {
    let mut still_todo = Vec::with_capacity(generating.generating.len());

    swap(&mut generating.generating, &mut still_todo);

    for mut generating_chunk in still_todo {
        if let Some(chunks) = future::block_on(future::poll_once(&mut generating_chunk.task)) {
            let (chunk, structure_entity) = chunks;

            if let Ok(mut structure) = structure_query.get_mut(structure_entity) {
                let (x, y, z) = (chunk.structure_x(), chunk.structure_y(), chunk.structure_z());

                structure.set_chunk(chunk);

                event_writer.send(GenerateChunkFeaturesEvent::<T> {
                    _phantom: PhantomData,
                    structure_entity,
                    chunk_coords: (x, y, z),
                });
            }
        } else {
            generating.generating.push(generating_chunk);
        }
    }
}

#[inline]
fn generate_face_chunk<S: BiosphereGenerationStrategy, T: Component + Clone + Default>(
    (sx, sy, sz): (usize, usize, usize),
    structure_coords: (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    block_ranges: &BlockLayers<T>,
    chunk: &mut Chunk,
    up: BlockFace,
) {
    for i in 0..CHUNK_DIMENSIONS {
        for j in 0..CHUNK_DIMENSIONS {
            let seed_coords = match up {
                BlockFace::Top => (sx + i, s_dimensions, sz + j),
                BlockFace::Bottom => (sx + i, 0, sz + j),
                BlockFace::Front => (sx + i, sy + j, s_dimensions),
                BlockFace::Back => (sx + i, sy + j, 0),
                BlockFace::Right => (s_dimensions, sy + i, sz + j),
                BlockFace::Left => (0, sy + i, sz + j),
            };

            let mut height = s_dimensions;
            let mut concrete_ranges = Vec::new();
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    up,
                    seed_coords,
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - level.middle_depth,
                    level.amplitude,
                    level.delta,
                    level.iterations,
                );
                concrete_ranges.push((block, level_top));
                height = level_top;
            }

            for chunk_height in 0..CHUNK_DIMENSIONS {
                let (x, y, z, height) = match up {
                    BlockFace::Front => (i, j, chunk_height, sz + chunk_height),
                    BlockFace::Back => (i, j, chunk_height, s_dimensions - (sz + chunk_height)),
                    BlockFace::Top => (i, chunk_height, j, sy + chunk_height),
                    BlockFace::Bottom => (i, chunk_height, j, s_dimensions - (sy + chunk_height)),
                    BlockFace::Right => (chunk_height, i, j, sx + chunk_height),
                    BlockFace::Left => (chunk_height, i, j, s_dimensions - (sx + chunk_height)),
                };

                let block = block_ranges.face_block(height, &concrete_ranges, block_ranges.sea_level, block_ranges.sea_block());
                if let Some(block) = block {
                    chunk.set_block_at(x, y, z, block, up);
                }
            }
        }
    }
}

fn generate_edge_chunk<S: BiosphereGenerationStrategy, T: Component + Clone + Default>(
    (sx, sy, sz): (usize, usize, usize),
    structure_coords: (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    block_ranges: &BlockLayers<T>,
    chunk: &mut Chunk,
    j_up: BlockFace,
    k_up: BlockFace,
) {
    for i in 0..CHUNK_DIMENSIONS {
        let mut j_layers: Vec<Vec<(&Block, usize)>> = vec![vec![]; CHUNK_DIMENSIONS];
        for (j, vec) in j_layers.iter_mut().enumerate() {
            // Seed coordinates and j-direction noise functions.
            let (mut x, mut y, mut z) = (sx + i, sy + i, sz + i);
            match j_up {
                BlockFace::Front => z = s_dimensions,
                BlockFace::Back => z = 0,
                BlockFace::Top => y = s_dimensions,
                BlockFace::Bottom => y = 0,
                BlockFace::Right => x = s_dimensions,
                BlockFace::Left => x = 0,
            };
            match k_up {
                BlockFace::Front | BlockFace::Back => z = sz + j,
                BlockFace::Top | BlockFace::Bottom => y = sy + j,
                BlockFace::Right | BlockFace::Left => x = sx + j,
            };
            let mut height = s_dimensions;
            for (block, layer) in block_ranges.ranges.iter() {
                let layer_top = S::get_top_height(
                    j_up,
                    (x, y, z),
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - layer.middle_depth,
                    layer.amplitude,
                    layer.delta,
                    layer.iterations,
                );
                vec.push((block, layer_top));
                height = layer_top;
            }
        }

        // The minimum (j, j) on the 45 where the two top heights intersect.
        let mut first_both_45 = s_dimensions;
        for j in 0..CHUNK_DIMENSIONS {
            // Seed coordinates and k-direction noise functions.
            let (mut x, mut y, mut z) = (sx + i, sy + i, sz + i);
            match k_up {
                BlockFace::Front => z = s_dimensions,
                BlockFace::Back => z = 0,
                BlockFace::Top => y = s_dimensions,
                BlockFace::Bottom => y = 0,
                BlockFace::Right => x = s_dimensions,
                BlockFace::Left => x = 0,
            };
            match j_up {
                BlockFace::Front | BlockFace::Back => z = sz + j,
                BlockFace::Top | BlockFace::Bottom => y = sy + j,
                BlockFace::Right | BlockFace::Left => x = sx + j,
            };
            let j_height = match j_up {
                BlockFace::Front => z,
                BlockFace::Back => s_dimensions - z,
                BlockFace::Top => y,
                BlockFace::Bottom => s_dimensions - y,
                BlockFace::Right => x,
                BlockFace::Left => s_dimensions - x,
            };

            let mut height = s_dimensions;
            let mut k_layers: Vec<(&Block, usize)> = vec![];
            for (block, layer) in block_ranges.ranges.iter() {
                let layer_top = S::get_top_height(
                    k_up,
                    (x, y, z),
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - layer.middle_depth,
                    layer.amplitude,
                    layer.delta,
                    layer.iterations,
                );
                k_layers.push((block, layer_top));
                height = layer_top;
            }

            if j_layers[j][0].1 == j_height && k_layers[0].1 == j_height && first_both_45 == s_dimensions {
                first_both_45 = j_height;
            }

            for (k, vec) in j_layers.iter().enumerate() {
                let (mut x, mut y, mut z) = (i, i, i);
                match j_up {
                    BlockFace::Front | BlockFace::Back => z = j,
                    BlockFace::Top | BlockFace::Bottom => y = j,
                    BlockFace::Right | BlockFace::Left => x = j,
                };
                match k_up {
                    BlockFace::Front | BlockFace::Back => z = k,
                    BlockFace::Top | BlockFace::Bottom => y = k,
                    BlockFace::Right | BlockFace::Left => x = k,
                };

                let k_height = match k_up {
                    BlockFace::Front => sz + z,
                    BlockFace::Back => s_dimensions - (sz + z),
                    BlockFace::Top => sy + y,
                    BlockFace::Bottom => s_dimensions - (sy + y),
                    BlockFace::Right => sx + x,
                    BlockFace::Left => s_dimensions - (sx + x),
                };

                if j_height < first_both_45 || k_height < first_both_45 {
                    // The top block needs different "top" to look good, the block can't tell which "up" looks good.
                    let block_up = Planet::get_planet_face_without_structure(sx + x, sy + y, sz + z, s_dimensions);
                    let block = block_ranges.edge_block(
                        j_height,
                        k_height,
                        Some(vec),
                        Some(&k_layers),
                        block_ranges.sea_level,
                        block_ranges.sea_block(),
                    );
                    if let Some(block) = block {
                        chunk.set_block_at(x, y, z, block, block_up);
                    }
                }
            }
        }
    }
}

fn generate_corner_chunk<S: BiosphereGenerationStrategy, T: Component + Clone + Default>(
    (sx, sy, sz): (usize, usize, usize),
    structure_coords: (f64, f64, f64),
    s_dimensions: usize,
    noise_generator: &noise::OpenSimplex,
    block_ranges: &BlockLayers<T>,
    chunk: &mut Chunk,
    x_up: BlockFace,
    y_up: BlockFace,
    z_up: BlockFace,
) {
    // x top height cache.
    let mut x_layers: Vec<Vec<(&Block, usize)>> = vec![vec![]; CHUNK_DIMENSIONS * CHUNK_DIMENSIONS];
    for j in 0..CHUNK_DIMENSIONS {
        for k in 0..CHUNK_DIMENSIONS {
            let index = flatten_2d(j, k, CHUNK_DIMENSIONS);

            // Seed coordinates for the noise function.
            let (x, y, z) = match x_up {
                BlockFace::Right => (s_dimensions, sy + j, sz + k),
                _ => (0, sy + j, sz + k),
            };

            // Unmodified top height.
            let mut height = s_dimensions;
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    x_up,
                    (x, y, z),
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - level.middle_depth,
                    level.amplitude,
                    level.delta,
                    level.iterations,
                );
                x_layers[index].push((block, level_top));
                height = level_top;
            }
        }
    }

    // y top height cache.
    let mut y_layers: Vec<Vec<(&Block, usize)>> = vec![vec![]; CHUNK_DIMENSIONS * CHUNK_DIMENSIONS];
    for i in 0..CHUNK_DIMENSIONS {
        for k in 0..CHUNK_DIMENSIONS {
            let index = flatten_2d(i, k, CHUNK_DIMENSIONS);

            // Seed coordinates for the noise function. Which loop variable goes to which xyz must agree everywhere.
            let (x, y, z) = match y_up {
                BlockFace::Top => (sx + i, s_dimensions, sz + k),
                _ => (sx + i, 0, sz + k),
            };

            // Unmodified top height.
            let mut height = s_dimensions;
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    y_up,
                    (x, y, z),
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - level.middle_depth,
                    level.amplitude,
                    level.delta,
                    level.iterations,
                );
                y_layers[index].push((block, level_top));
                height = level_top;
            }
        }
    }

    for i in 0..CHUNK_DIMENSIONS {
        // The minimum (j, j, j) on the 45 where the three top heights intersect.
        // let mut first_all_45 = s_dimensions;
        for j in 0..CHUNK_DIMENSIONS {
            // Seed coordinates for the noise function.
            let (x, y, z) = match z_up {
                BlockFace::Front => (sx + i, sy + j, s_dimensions),
                _ => (sx + i, sy + j, 0),
            };

            // Unmodified top height.
            let mut height = s_dimensions;
            let mut z_layers = vec![];
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    z_up,
                    (x, y, z),
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - level.middle_depth,
                    level.amplitude,
                    level.delta,
                    level.iterations,
                );
                z_layers.push((block, level_top));
                height = level_top;
            }
            // z_layers[0].1 = z_layers[0].1.min(first_all_45);
            // let z_top = z_layers[0].1;

            // Get smallest top height that's on the 45 for x, y, and z.
            // let index = flatten_2d(i, j, CHUNK_DIMENSIONS);
            // if x_layers[index][0].1 == j && y_layers[index][0].1 == j && z_top == j && first_all_45 == s_dimensions {
            //     first_all_45 = z_top;
            // };

            for k in 0..CHUNK_DIMENSIONS {
                // Don't let the top rise "above" the first shared 45.
                // let x_top = x_layers[flatten_2d(j, k, CHUNK_DIMENSIONS)][0].1.min(first_all_45);
                // let y_top = y_layers[flatten_2d(i, k, CHUNK_DIMENSIONS)][0].1.min(first_all_45);

                let z_height = match z_up {
                    BlockFace::Front => sz + k,
                    _ => s_dimensions - (sz + k),
                };
                let y_height = match y_up {
                    BlockFace::Top => sy + j,
                    _ => s_dimensions - (sy + j),
                };
                let x_height = match x_up {
                    BlockFace::Right => sx + i,
                    _ => s_dimensions - (sx + i),
                };

                // Stops stairways to heaven.
                // let num_top: usize = (x_height == x_top) as usize + (y_height == y_top) as usize + (z_height == z_top) as usize;
                // if num_top <= 1 {
                let block_up = Planet::get_planet_face_without_structure(sx + i, sy + j, sz + k, s_dimensions);
                let block = block_ranges.corner_block(
                    x_height,
                    y_height,
                    z_height,
                    &x_layers[flatten_2d(j, k, CHUNK_DIMENSIONS)],
                    &y_layers[flatten_2d(i, k, CHUNK_DIMENSIONS)],
                    &z_layers,
                    block_ranges.sea_level,
                    block_ranges.sea_block(),
                );
                if let Some(block) = block {
                    chunk.set_block_at(i, j, k, block, block_up);
                }
                // }
            }
        }
    }
}

const MIRROR_MIN: usize = 100;
const MIRROR_MAX: usize = 0;
/// Used to change the algorithm used for base terrain generation.
///
/// Try tweaking the values of GenerationParemeters first before making your own custom generation function.
///
/// For most cases, the `DefaultBiosphereGenerationStrategy` strategy will work.
pub trait BiosphereGenerationStrategy {
    /// Gets the "y" value of a block on the planet. This "y" value is relative to the face the block is on.
    ///
    /// * `noise_generator` Used to generate noise values. Seeded for this world seed.
    /// * `(x, y, z)` Block x/y/z in the structure
    /// * `(structure_x, structure_y, structure_z)` Where the structure is in the universe - used to offset the noise values so no two structures are the same.
    /// * `(middle_air_start)` The midpoint of the extremes of heights. Aka if noise generates 0, then this should return middle_air_start.
    /// * `amplitude` Value passed in by the `GenerationParemeters`. Represents how tall the terrain will be
    /// * `delta` Value passed in by the `GenerationParemeters`. Represents how much each change in x/y/z will effect the terrain. Small values = lesser effect
    /// * `iterations` Value passed in by the `GenerationParemeters`. Represents how many times the noise function will be run
    fn get_block_height(
        noise_generator: &noise::OpenSimplex,
        block_coords: (usize, usize, usize),
        structure_coords: (f64, f64, f64),
        middle_air_start: usize,
        amplitude: f64,
        delta: f64,
        iterations: usize,
    ) -> f64 {
        get_block_height(
            noise_generator,
            block_coords,
            structure_coords,
            middle_air_start,
            amplitude,
            delta,
            iterations,
        )
    }

    /// Returns how much the edge height should be averaged in from the other side it's approaching.
    ///
    /// Don't touch this unless you're doing something extremely crazy
    ///
    /// - `a` x, y, or z but generalized
    /// - `intersection` is where the two edges are projected to meet, which is used as the limit to your height
    /// - `s_dimensions` structure width/height/length
    fn get_mirror_coefficient(a: usize, intersection: usize, s_dimensions: usize) -> f64 {
        let max = intersection - MIRROR_MAX;
        let min = intersection - MIRROR_MIN;
        if a > max || a < s_dimensions - max {
            1.0
        } else if a > min {
            1.0 - (max - a) as f64 / (max - min) as f64
        } else if a < s_dimensions - min {
            1.0 - ((a - (s_dimensions - max)) as f64 / (max - min) as f64)
        } else {
            0.0
        }
    }

    /// "Where the math happens" - Dan
    ///
    /// Combining two linear gradients so that they have the same end behaviors is "a little difficult". Thus the max functions.
    ///
    /// No touchy
    ///
    /// - `height` If you were at the center of the face of a planet - that's how tall this column would be
    /// - `c1` The first edge coefficient (from `get_mirror_coefficient`)
    /// - `c1_height` The height on c1's edge
    /// - `c2` The second edge coefficient (from `get_mirror_coefficient`)
    /// - `c2_height` The height on c2's edge
    fn merge(height: f64, c1: f64, c1_height: f64, c2: f64, c2_height: f64) -> usize {
        let c = if c1 + c2 == 0.0 { 0.0 } else { c1.max(c2) / (c1 + c2) };
        (height * (1.0 - c * (c1 + c2)) + c * (c1 * c1_height + c2 * c2_height)) as usize
    }

    /// brownie points to whoever documents this
    fn mirror(
        noise_generator: &noise::OpenSimplex,
        block_up: BlockFace,
        (bx, by, bz): (usize, usize, usize),
        structure_coords: (f64, f64, f64),
        middle_air_start: usize,
        amplitude: f64,
        delta: f64,
        iterations: usize,
        s_dimensions: usize,
    ) -> usize {
        let top = middle_air_start - (amplitude * iterations as f64) as usize;
        let bottom = s_dimensions - top;
        let min = top - MIRROR_MIN;

        // X.
        let mut x_coefficient = 0.0;
        let mut x_height = 0.0;
        if bx > min || bx < s_dimensions - min {
            let x_coord = if bx > s_dimensions / 2 { top } else { bottom };
            let x_seed = match block_up {
                BlockFace::Front => (x_coord, by.clamp(bottom, top), top),
                BlockFace::Back => (x_coord, by.clamp(bottom, top), bottom),
                BlockFace::Top => (x_coord, top, bz.clamp(bottom, top)),
                BlockFace::Bottom => (x_coord, bottom, bz.clamp(bottom, top)),
                BlockFace::Right => (x_coord, by, bz),
                BlockFace::Left => (x_coord, by, bz),
            };
            x_height = self::get_block_height(
                noise_generator,
                x_seed,
                structure_coords,
                middle_air_start,
                amplitude,
                delta,
                iterations,
            );
            x_coefficient = Self::get_mirror_coefficient(bx, x_height as usize, s_dimensions);
        }

        // Y.
        let mut y_coefficient = 0.0;
        let mut y_height = 0.0;
        if by > min || by < s_dimensions - min {
            let y_coord = if by > s_dimensions / 2 { top } else { bottom };
            let y_seed = match block_up {
                BlockFace::Front => (bx.clamp(bottom, top), y_coord, top),
                BlockFace::Back => (bx.clamp(bottom, top), y_coord, bottom),
                BlockFace::Top => (bx, y_coord, bz),
                BlockFace::Bottom => (bx, y_coord, bz),
                BlockFace::Right => (top, y_coord, bz.clamp(bottom, top)),
                BlockFace::Left => (bottom, y_coord, bz.clamp(bottom, top)),
            };
            y_height = self::get_block_height(
                noise_generator,
                y_seed,
                structure_coords,
                middle_air_start,
                amplitude,
                delta,
                iterations,
            );
            y_coefficient = Self::get_mirror_coefficient(by, y_height as usize, s_dimensions);
        }

        // Z.
        let mut z_coefficient = 0.0;
        let mut z_height = 0.0;
        if bz > min || bz < s_dimensions - min {
            let z_coord = if bz > s_dimensions / 2 { top } else { bottom };
            let z_seed = match block_up {
                BlockFace::Front => (bx, by, z_coord),
                BlockFace::Back => (bx, by, z_coord),
                BlockFace::Top => (bx.clamp(bottom, top), top, z_coord),
                BlockFace::Bottom => (bx.clamp(bottom, top), bottom, z_coord),
                BlockFace::Right => (top, by.clamp(bottom, top), z_coord),
                BlockFace::Left => (bottom, by.clamp(bottom, top), z_coord),
            };
            z_height = self::get_block_height(
                noise_generator,
                z_seed,
                structure_coords,
                middle_air_start,
                amplitude,
                delta,
                iterations,
            );
            z_coefficient = Self::get_mirror_coefficient(bz, z_height as usize, s_dimensions);
        }

        let height = match block_up {
            BlockFace::Front | BlockFace::Back => Self::merge(z_height, x_coefficient, x_height, y_coefficient, y_height),
            BlockFace::Top | BlockFace::Bottom => Self::merge(y_height, x_coefficient, x_height, z_coefficient, z_height),
            BlockFace::Right | BlockFace::Left => Self::merge(x_height, y_coefficient, y_height, z_coefficient, z_height),
        };
        if height < 100 {
            panic!("Low height ({height}) for coordinates ({bx}, {by}, {bz}). cx = {x_coefficient}, hx = {x_height}. cy = {y_coefficient}, hy = {y_height}. cz = {z_coefficient}, hz = {z_height}.");
        } else {
            height
        }
    }

    /// Gets the top block's height
    ///
    /// * `(x, y, z)` Block x/y/z in the structure
    /// * `(structure_x, structure_y, structure_z)` Where the structure is in the universe - used to offset the noise values so no two structures are the same.
    /// * `(s_dimensions)` The width/height/length of the structure this is on.
    /// * `noise_generator` Used to generate noise values. Seeded for this world seed.
    /// * `(middle_air_start)` The midpoint of the extremes of heights. Aka if noise generates 0, then this should return middle_air_start.
    /// * `amplitude` Value passed in by the `GenerationParemeters`. Represents how tall the terrain will be
    /// * `delta` Value passed in by the `GenerationParemeters`. Represents how much each change in x/y/z will effect the terrain. Small values = lesser effect
    /// * `iterations` Value passed in by the `GenerationParemeters`. Represents how many times the noise function will be run
    fn get_top_height(
        block_up: BlockFace,
        (x, y, z): (usize, usize, usize),
        (structure_x, structure_y, structure_z): (f64, f64, f64),
        s_dimensions: usize,
        noise_generator: &noise::OpenSimplex,
        middle_air_start: usize,
        amplitude: f64,
        delta: f64,
        iterations: usize,
    ) -> usize {
        Self::mirror(
            noise_generator,
            block_up,
            (x, y, z),
            (structure_x, structure_y, structure_z),
            middle_air_start,
            amplitude,
            delta,
            iterations,
            s_dimensions,
        )
    }
}

/// The default implementation for the `BiosphereGenerationStrategy` that will work for most biospheres.
pub struct DefaultBiosphereGenerationStrategy;

impl BiosphereGenerationStrategy for DefaultBiosphereGenerationStrategy {}

/// Stores which blocks make up each biosphere, and how far below the top solid block each block generates.
/// Blocks in ascending order ("stone" = 5 first, "grass" = 0 last).
#[derive(Resource, Clone, Default, Debug)]
pub struct BlockLayers<T: Component + Clone + Default> {
    _phantom: PhantomData<T>,
    ranges: Vec<(Block, BlockLayer)>,
    sea_block: Option<Block>,
    sea_level: Option<usize>,
}

/// Stores the blocks and all the noise information for creating the top of their layer.
/// For example, the "stone" BlockLevel has the noise paramters that create the boundry between dirt and stone.
#[derive(Clone, Debug)]
pub struct BlockLayer {
    middle_depth: usize,
    delta: f64,
    amplitude: f64,
    iterations: usize,
}

impl BlockLayer {
    /// This layer doesn't use a noise function to generate its span, and is thus fixed at a certain depth.
    pub fn fixed_layer(middle_depth: usize) -> Self {
        Self {
            middle_depth,
            delta: 0.0,
            amplitude: 0.0,
            iterations: 0,
        }
    }

    /// This layer is based off a noise function and will appear at a varying depth based on the parameters
    pub fn noise_layer(middle_depth: usize, delta: f64, amplitude: f64, iterations: usize) -> Self {
        Self {
            middle_depth,
            delta,
            amplitude,
            iterations,
        }
    }
}

#[derive(Debug)]
/// Errors generated when initally setting up the block ranges
pub enum BlockRangeError<T: Component + Clone + Default> {
    /// This means the block id provided was not found in the block registry
    MissingBlock(BlockLayers<T>),
}

impl<T: Component + Clone + Default> BlockLayers<T> {
    /// Creates a new block range, for each planet type to specify its blocks.
    pub fn new() -> Self {
        Self::default()
    }

    /// Does what `add_fixed_layer` does, but makes the layer depth vary based off the noise parameters.
    pub fn add_noise_layer(
        mut self,
        block_id: &str,
        block_registry: &Registry<Block>,
        middle_depth: usize,
        delta: f64,
        amplitude: f64,
        iterations: usize,
    ) -> Result<Self, BlockRangeError<T>> {
        let Some(block) = block_registry.from_id(block_id) else {
            return Err(BlockRangeError::MissingBlock(self));
        };
        let layer = BlockLayer::noise_layer(middle_depth, delta, amplitude, iterations);
        self.ranges.push((block.clone(), layer));
        Ok(self)
    }

    /// Use this to construct the various ranges of the blocks.
    ///
    /// The order you add the ranges in DOES matter.
    ///
    /// middle_depth represents how many blocks from the previous layer this block will appear.
    /// For example, If grass was 100, dirt was 1, and stone was 4, it would generate as:
    /// - 100 blocks of air
    /// - Grass
    /// - Dirt
    /// - Dirt
    /// - Dirt
    /// - Dirt
    /// - Stone
    /// - Stone
    /// - Stone
    /// - ... stone down to the bottom
    pub fn add_fixed_layer(
        mut self,
        block_id: &str,
        block_registry: &Registry<Block>,
        middle_depth: usize,
    ) -> Result<Self, BlockRangeError<T>> {
        let Some(block) = block_registry.from_id(block_id) else {
            return Err(BlockRangeError::MissingBlock(self));
        };
        let layer = BlockLayer::fixed_layer(middle_depth);
        self.ranges.push((block.clone(), layer));
        Ok(self)
    }

    /// Sets the sea level and the block that goes along with it
    pub fn with_sea_level_block(
        mut self,
        block_id: &str,
        block_registry: &Registry<Block>,
        sea_level: usize,
    ) -> Result<Self, BlockRangeError<T>> {
        let Some(block) = block_registry.from_id(block_id).cloned() else {
            return Err(BlockRangeError::MissingBlock(self));
        };
        self.sea_block = Some(block);
        self.sea_level = Some(sea_level);
        Ok(self)
    }

    #[inline]
    fn sea_block(&self) -> Option<&Block> {
        self.sea_block.as_ref()
    }

    fn face_block<'a>(
        &self,
        height: usize,
        block_layers: &[(&'a Block, usize)],
        sea_level: Option<usize>,
        sea_block: Option<&'a Block>,
    ) -> Option<&'a Block> {
        for (block, level_top) in block_layers.iter().rev() {
            if height <= *level_top {
                return Some(*block);
            }
        }
        // No land blocks, must be sea or air.
        if sea_level.map(|sea_level| height <= sea_level).unwrap_or(false) {
            Some(sea_block.expect("Set sea level without setting a sea block."))
        } else {
            None
        }
    }

    fn edge_block<'a>(
        &self,
        j_height: usize,
        k_height: usize,
        j_layers: Option<&[(&'a Block, usize)]>,
        k_layers: Option<&[(&'a Block, usize)]>,
        sea_level: Option<usize>,
        sea_block: Option<&'a Block>,
    ) -> Option<&'a Block> {
        match (j_layers, k_layers) {
            (Some(j_layers), Some(k_layers)) => {
                for (index, (block, j_layer_top)) in j_layers.iter().enumerate().rev() {
                    if j_height <= *j_layer_top && k_height <= k_layers[index].1 {
                        return Some(*block);
                    }
                }
            }
            (Some(j_layers), None) => {
                for (block, j_layer_top) in j_layers.iter().rev() {
                    if j_height <= *j_layer_top {
                        return Some(*block);
                    }
                }
            }
            (None, Some(k_layers)) => {
                for (block, k_layer_top) in k_layers.iter().rev() {
                    if k_height <= *k_layer_top {
                        return Some(*block);
                    }
                }
            }
            (None, None) => {}
        }

        // No land blocks, must be sea or air.
        if sea_level.map(|sea_level| j_height.max(k_height) <= sea_level).unwrap_or(false) {
            Some(sea_block.expect("Set sea level without setting a sea block."))
        } else {
            None
        }
    }

    fn corner_block<'a>(
        &self,
        x_height: usize,
        y_height: usize,
        z_height: usize,
        x_layers: &[(&'a Block, usize)],
        y_layers: &[(&'a Block, usize)],
        z_layers: &[(&'a Block, usize)],
        sea_level: Option<usize>,
        sea_block: Option<&'a Block>,
    ) -> Option<&'a Block> {
        for (index, (block, x_layer_top)) in x_layers.iter().enumerate().rev() {
            if x_height <= *x_layer_top && y_height <= y_layers[index].1 && z_height <= z_layers[index].1 {
                return Some(*block);
            }
        }
        // No land blocks, must be sea or air.
        if sea_level
            .map(|sea_level| x_height.max(y_height).max(z_height) <= sea_level)
            .unwrap_or(false)
        {
            Some(sea_block.expect("Set sea level without setting a sea block."))
        } else {
            None
        }
    }
}

/// Calls generate_face_chunk, generate_edge_chunk, and generate_corner_chunk to generate the chunks of a planet.
pub fn generate_planet<T: Component + Clone + Default, E: TGenerateChunkEvent + Send + Sync + 'static, S: BiosphereGenerationStrategy>(
    mut query: Query<(&mut Structure, &Location)>,
    mut generating: ResMut<GeneratingChunks<T>>,
    mut events: EventReader<E>,
    noise_generator: Res<ResourceWrapper<noise::OpenSimplex>>,
    block_ranges: Res<BlockLayers<T>>,
) {
    let chunks = events
        .iter()
        .filter_map(|ev| {
            let structure_entity = ev.get_structure_entity();
            let (x, y, z) = ev.get_chunk_coordinates();
            if let Ok((mut structure, _)) = query.get_mut(structure_entity) {
                Some((structure_entity, structure.take_or_create_chunk_for_loading(x, y, z)))
            } else {
                None
            }
        })
        .collect::<Vec<(Entity, Chunk)>>();

    let thread_pool = AsyncComputeTaskPool::get();

    let chunks = chunks
        .into_iter()
        .flat_map(|(structure_entity, chunk)| {
            let Ok((structure, location)) = query.get(structure_entity) else {
                return None;
            };

            let s_dimensions = structure.blocks_length();
            let location = *location;

            Some((chunk, s_dimensions, location, structure_entity))
        })
        .collect::<Vec<(Chunk, usize, Location, Entity)>>();

    if !chunks.is_empty() {
        println!("Doing {} chunks!", chunks.len());

        for (mut chunk, s_dimensions, location, structure_entity) in chunks {
            let block_ranges = block_ranges.clone();
            let noise_generator = **noise_generator;

            let task = thread_pool.spawn(async move {
                let timer = UtilsTimer::start();

                let actual_pos = location.absolute_coords_f64();

                let structure_z = actual_pos.z;
                let structure_y = actual_pos.y;
                let structure_x = actual_pos.x;

                // To save multiplication operations later.
                let sz = chunk.structure_z() * CHUNK_DIMENSIONS;
                let sy = chunk.structure_y() * CHUNK_DIMENSIONS;
                let sx = chunk.structure_x() * CHUNK_DIMENSIONS;

                // Get all possible planet faces from the chunk corners.
                let chunk_faces = Planet::chunk_planet_faces((sx, sy, sz), s_dimensions);
                match chunk_faces {
                    ChunkFaces::Face(up) => {
                        generate_face_chunk::<S, T>(
                            (sx, sy, sz),
                            (structure_x, structure_y, structure_z),
                            s_dimensions,
                            &noise_generator,
                            &block_ranges,
                            &mut chunk,
                            up,
                        );
                    }
                    ChunkFaces::Edge(j_up, k_up) => {
                        generate_edge_chunk::<S, T>(
                            (sx, sy, sz),
                            (structure_x, structure_y, structure_z),
                            s_dimensions,
                            &noise_generator,
                            &block_ranges,
                            &mut chunk,
                            j_up,
                            k_up,
                        );
                    }
                    ChunkFaces::Corner(x_up, y_up, z_up) => {
                        generate_corner_chunk::<S, T>(
                            (sx, sy, sz),
                            (structure_x, structure_y, structure_z),
                            s_dimensions,
                            &noise_generator,
                            &block_ranges,
                            &mut chunk,
                            x_up,
                            y_up,
                            z_up,
                        );
                    }
                }
                timer.log_duration("Chunk: ");
                (chunk, structure_entity)
            });

            generating.generating.push(GeneratingChunk::new(task));
        }
    }
}
