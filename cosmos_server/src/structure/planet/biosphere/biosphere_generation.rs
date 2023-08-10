//! Responsible for the default generation of biospheres.

use std::{marker::PhantomData, mem::swap};

use bevy::{
    prelude::{Component, Entity, Event, EventReader, EventWriter, Query, Res, ResMut, Resource},
    tasks::AsyncComputeTaskPool,
};
use cosmos_core::{
    block::{Block, BlockFace},
    physics::location::Location,
    registry::Registry,
    structure::{
        chunk::{Chunk, CHUNK_DIMENSIONS},
        coordinates::{BlockCoordinate, ChunkBlockCoordinate, ChunkCoordinate, CoordinateType},
        planet::{ChunkFaces, Planet},
        Structure,
    },
    utils::{array_utils::flatten_2d, resource_wrapper::ResourceWrapper, timer::UtilsTimer},
};
use futures_lite::future;
use noise::NoiseFn;

use super::{GeneratingChunk, GeneratingChunks, TGenerateChunkEvent};

/// Tells the chunk to generate its features.
#[derive(Debug, Event)]
pub struct GenerateChunkFeaturesEvent<T: Component> {
    _phantom: PhantomData<T>,
    /// cx, cy, cz.
    pub chunk_coords: ChunkCoordinate,
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
    block_coords: BlockCoordinate,
    (structure_x, structure_y, structure_z): (f64, f64, f64),
    middle: CoordinateType,
    amplitude: f64,
    delta: f64,
    iterations: usize,
) -> f64 {
    let mut depth: f64 = 0.0;
    for iteration in 1..=iterations {
        let iteration = iteration as f64;
        depth += noise_generator.get([
            (block_coords.x as f64 + structure_x) * (delta / iteration),
            (block_coords.y as f64 + structure_y) * (delta / iteration),
            (block_coords.z as f64 + structure_z) * (delta / iteration),
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
                let chunk_coords = chunk.chunk_coordinates();

                structure.set_chunk(chunk);

                event_writer.send(GenerateChunkFeaturesEvent::<T> {
                    _phantom: PhantomData,
                    structure_entity,
                    chunk_coords,
                });
            }
        } else {
            generating.generating.push(generating_chunk);
        }
    }
}

#[inline]
fn generate_face_chunk<S: BiosphereGenerationStrategy, T: Component + Clone + Default>(
    block_coords: BlockCoordinate,
    structure_coords: (f64, f64, f64),
    s_dimensions: CoordinateType,
    noise_generator: &noise::OpenSimplex,
    block_ranges: &BlockLayers<T>,
    chunk: &mut Chunk,
    up: BlockFace,
) {
    let (sx, sy, sz) = (block_coords.x, block_coords.y, block_coords.z);

    for i in 0..CHUNK_DIMENSIONS {
        for j in 0..CHUNK_DIMENSIONS {
            let seed_coords: BlockCoordinate = match up {
                BlockFace::Top => (sx + i, s_dimensions, sz + j),
                BlockFace::Bottom => (sx + i, 0, sz + j),
                BlockFace::Front => (sx + i, sy + j, s_dimensions),
                BlockFace::Back => (sx + i, sy + j, 0),
                BlockFace::Right => (s_dimensions, sy + i, sz + j),
                BlockFace::Left => (0, sy + i, sz + j),
            }
            .into();

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
                let coords: ChunkBlockCoordinate = match up {
                    BlockFace::Front => (i, j, chunk_height),
                    BlockFace::Back => (i, j, chunk_height),
                    BlockFace::Top => (i, chunk_height, j),
                    BlockFace::Bottom => (i, chunk_height, j),
                    BlockFace::Right => (chunk_height, i, j),
                    BlockFace::Left => (chunk_height, i, j),
                }
                .into();

                let height = match up {
                    BlockFace::Front => sz + chunk_height,
                    BlockFace::Back => s_dimensions - (sz + chunk_height),
                    BlockFace::Top => sy + chunk_height,
                    BlockFace::Bottom => s_dimensions - (sy + chunk_height),
                    BlockFace::Right => sx + chunk_height,
                    BlockFace::Left => s_dimensions - (sx + chunk_height),
                };

                let block = block_ranges.face_block(height, &concrete_ranges, block_ranges.sea_level, block_ranges.sea_block());
                if let Some(block) = block {
                    chunk.set_block_at(coords, block, up);
                }
            }
        }
    }
}

fn generate_edge_chunk<S: BiosphereGenerationStrategy, T: Component + Clone + Default>(
    block_coords: BlockCoordinate,
    structure_coords: (f64, f64, f64),
    s_dimensions: CoordinateType,
    noise_generator: &noise::OpenSimplex,
    block_ranges: &BlockLayers<T>,
    chunk: &mut Chunk,
    j_up: BlockFace,
    k_up: BlockFace,
) {
    for i in 0..CHUNK_DIMENSIONS {
        let mut j_layers_cache: Vec<Vec<(&Block, CoordinateType)>> = vec![vec![]; CHUNK_DIMENSIONS as usize];
        for (j, j_layers) in j_layers_cache.iter_mut().enumerate() {
            // Seed coordinates and j-direction noise functions.
            let (mut x, mut y, mut z) = (block_coords.x + i, block_coords.y + i, block_coords.z + i);
            match j_up {
                BlockFace::Front => z = s_dimensions,
                BlockFace::Back => z = 0,
                BlockFace::Top => y = s_dimensions,
                BlockFace::Bottom => y = 0,
                BlockFace::Right => x = s_dimensions,
                BlockFace::Left => x = 0,
            };
            match k_up {
                BlockFace::Front | BlockFace::Back => z = block_coords.z + j as CoordinateType,
                BlockFace::Top | BlockFace::Bottom => y = block_coords.y + j as CoordinateType,
                BlockFace::Right | BlockFace::Left => x = block_coords.x + j as CoordinateType,
            };
            let mut height = s_dimensions;
            for (block, layer) in block_ranges.ranges.iter() {
                let layer_top = S::get_top_height(
                    j_up,
                    BlockCoordinate::new(x, y, z),
                    structure_coords,
                    s_dimensions,
                    noise_generator,
                    height - layer.middle_depth,
                    layer.amplitude,
                    layer.delta,
                    layer.iterations,
                );
                j_layers.push((block, layer_top));
                height = layer_top;
            }
        }

        // The minimum (j, j) on the 45 where the two top heights intersect.
        let mut first_both_45 = s_dimensions;
        for j in 0..CHUNK_DIMENSIONS {
            // Seed coordinates and k-direction noise functions.
            let (mut x, mut y, mut z) = (block_coords.x + i, block_coords.y + i, block_coords.z + i);
            match k_up {
                BlockFace::Front => z = s_dimensions,
                BlockFace::Back => z = 0,
                BlockFace::Top => y = s_dimensions,
                BlockFace::Bottom => y = 0,
                BlockFace::Right => x = s_dimensions,
                BlockFace::Left => x = 0,
            };
            match j_up {
                BlockFace::Front | BlockFace::Back => z = block_coords.z + j,
                BlockFace::Top | BlockFace::Bottom => y = block_coords.y + j,
                BlockFace::Right | BlockFace::Left => x = block_coords.x + j,
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
            let mut k_layers: Vec<(&Block, CoordinateType)> = vec![];
            for (block, layer) in block_ranges.ranges.iter() {
                let layer_top = S::get_top_height(
                    k_up,
                    BlockCoordinate::new(x, y, z),
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

            if j_layers_cache[j as usize][0].1 == j_height && k_layers[0].1 == j_height && first_both_45 == s_dimensions {
                first_both_45 = j_height;
            }

            for (k, j_layers) in j_layers_cache.iter().enumerate() {
                let mut chunk_block_coords: ChunkBlockCoordinate = (i, i, i).into();
                match j_up {
                    BlockFace::Front | BlockFace::Back => chunk_block_coords.z = j,
                    BlockFace::Top | BlockFace::Bottom => chunk_block_coords.y = j,
                    BlockFace::Right | BlockFace::Left => chunk_block_coords.x = j,
                };
                match k_up {
                    BlockFace::Front | BlockFace::Back => chunk_block_coords.z = k as CoordinateType,
                    BlockFace::Top | BlockFace::Bottom => chunk_block_coords.y = k as CoordinateType,
                    BlockFace::Right | BlockFace::Left => chunk_block_coords.x = k as CoordinateType,
                };

                let k_height = match k_up {
                    BlockFace::Front => block_coords.z + chunk_block_coords.z,
                    BlockFace::Back => s_dimensions - (block_coords.z + chunk_block_coords.z),
                    BlockFace::Top => block_coords.y + chunk_block_coords.y,
                    BlockFace::Bottom => s_dimensions - (block_coords.y + chunk_block_coords.y),
                    BlockFace::Right => block_coords.x + chunk_block_coords.x,
                    BlockFace::Left => s_dimensions - (block_coords.x + chunk_block_coords.x),
                };

                if j_height < first_both_45 || k_height < first_both_45 {
                    // The top block needs different "top" to look good, the block can't tell which "up" looks good.
                    let block_up = Planet::get_planet_face_without_structure(
                        BlockCoordinate::new(
                            block_coords.x + chunk_block_coords.x,
                            block_coords.y + chunk_block_coords.y,
                            block_coords.z + chunk_block_coords.z,
                        ),
                        s_dimensions,
                    );
                    let block = block_ranges.edge_block(
                        j_height,
                        k_height,
                        j_layers,
                        &k_layers,
                        block_ranges.sea_level,
                        block_ranges.sea_block(),
                    );
                    if let Some(block) = block {
                        chunk.set_block_at(chunk_block_coords, block, block_up);
                    }
                }
            }
        }
    }
}

// Might trim 45s, see generate_edge_chunk.
fn generate_corner_chunk<S: BiosphereGenerationStrategy, T: Component + Clone + Default>(
    block_coords: BlockCoordinate,
    structure_coords: (f64, f64, f64),
    s_dimensions: CoordinateType,
    noise_generator: &noise::OpenSimplex,
    block_ranges: &BlockLayers<T>,
    chunk: &mut Chunk,
    x_up: BlockFace,
    y_up: BlockFace,
    z_up: BlockFace,
) {
    // x top height cache.
    let mut x_layers: Vec<Vec<(&Block, CoordinateType)>> = vec![vec![]; CHUNK_DIMENSIONS as usize * CHUNK_DIMENSIONS as usize];
    for j in 0..CHUNK_DIMENSIONS {
        for k in 0..CHUNK_DIMENSIONS {
            let index = flatten_2d(j as usize, k as usize, CHUNK_DIMENSIONS as usize);

            // Seed coordinates for the noise function.
            let seed_coords = match x_up {
                BlockFace::Right => (s_dimensions, block_coords.y + j, block_coords.z + k),
                _ => (0, block_coords.y + j, block_coords.z + k),
            }
            .into();

            // Unmodified top height.
            let mut height = s_dimensions;
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    x_up,
                    seed_coords,
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
    let mut y_layers: Vec<Vec<(&Block, CoordinateType)>> = vec![vec![]; CHUNK_DIMENSIONS as usize * CHUNK_DIMENSIONS as usize];
    for i in 0..CHUNK_DIMENSIONS {
        for k in 0..CHUNK_DIMENSIONS {
            let index = flatten_2d(i as usize, k as usize, CHUNK_DIMENSIONS as usize);

            // Seed coordinates for the noise function. Which loop variable goes to which xyz must agree everywhere.
            let seed_coords = match y_up {
                BlockFace::Top => (block_coords.x + i, s_dimensions, block_coords.z + k),
                _ => (block_coords.x + i, 0, block_coords.z + k),
            }
            .into();

            // Unmodified top height.
            let mut height = s_dimensions;
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    y_up,
                    seed_coords,
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
        for j in 0..CHUNK_DIMENSIONS {
            // Seed coordinates for the noise function.
            let seed_coords = match z_up {
                BlockFace::Front => (block_coords.x + i, block_coords.y + j, s_dimensions),
                _ => (block_coords.x + i, block_coords.y + j, 0),
            }
            .into();

            // Unmodified top height.
            let mut height = s_dimensions;
            let mut z_layers = vec![];
            for (block, level) in block_ranges.ranges.iter() {
                let level_top = S::get_top_height(
                    z_up,
                    seed_coords,
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

            for k in 0..CHUNK_DIMENSIONS {
                let z_height = match z_up {
                    BlockFace::Front => block_coords.z + k,
                    _ => s_dimensions - (block_coords.z + k),
                };
                let y_height = match y_up {
                    BlockFace::Top => block_coords.y + j,
                    _ => s_dimensions - (block_coords.y + j),
                };
                let x_height = match x_up {
                    BlockFace::Right => block_coords.x + i,
                    _ => s_dimensions - (block_coords.x + i),
                };

                let block_up = Planet::get_planet_face_without_structure(
                    BlockCoordinate::new(block_coords.x + i, block_coords.y + j, block_coords.z + k),
                    s_dimensions,
                );
                let block = block_ranges.corner_block(
                    x_height,
                    y_height,
                    z_height,
                    &x_layers[flatten_2d(j as usize, k as usize, CHUNK_DIMENSIONS as usize)],
                    &y_layers[flatten_2d(i as usize, k as usize, CHUNK_DIMENSIONS as usize)],
                    &z_layers,
                    block_ranges.sea_level,
                    block_ranges.sea_block(),
                );
                if let Some(block) = block {
                    chunk.set_block_at(ChunkBlockCoordinate::new(i, j, k), block, block_up);
                }
            }
        }
    }
}

const GUIDE_MIN: CoordinateType = 100;
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
        block_coords: BlockCoordinate,
        structure_coords: (f64, f64, f64),
        middle_air_start: CoordinateType,
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
    /// Don't touch this unless you're doing something extremely crazy.
    ///
    /// - `a` x, y, or z but generalized.
    /// - `intersection` is where the two edges are projected to meet, which is used as the limit to your height.
    /// - `s_dimensions` structure width/height/length.
    fn get_mirror_coefficient(a: CoordinateType, intersection: CoordinateType, s_dimensions: CoordinateType) -> f64 {
        let max = intersection;
        let min = intersection - GUIDE_MIN;
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

    /// "Where the math happens" - Dan.
    ///
    /// Combining two linear gradients so that they have the same end behaviors is "a little difficult". Thus the max functions.
    ///
    /// No touchy.
    ///
    /// - `height` If you were at the center of the face of a planet - that's how tall this column would be.
    /// - `c1` The first edge coefficient (from `get_mirror_coefficient`).
    /// - `c1_height` The height on c1's edge.
    /// - `c2` The second edge coefficient (from `get_mirror_coefficient`).
    /// - `c2_height` The height on c2's edge.
    fn merge(height: f64, c1: f64, c1_height: f64, c2: f64, c2_height: f64) -> CoordinateType {
        let c = if c1 + c2 == 0.0 { 0.0 } else { c1.max(c2) / (c1 + c2) };
        (height * (1.0 - c * (c1 + c2)) + c * (c1 * c1_height + c2 * c2_height)) as CoordinateType
    }

    /// Generates the "old" height, the one that's used if you're in the middle of a face.
    /// Also generates the height at any edge within GUIDE_MIN distance.
    /// Averages the "old" height with the edge heights with coefficients based on how close you are to the edge intersection.
    fn guide(
        noise_generator: &noise::OpenSimplex,
        block_up: BlockFace,
        block_coords: BlockCoordinate,
        structure_coords: (f64, f64, f64),
        middle_air_start: CoordinateType,
        amplitude: f64,
        delta: f64,
        iterations: usize,
        s_dimensions: CoordinateType,
    ) -> CoordinateType {
        // The amplitude * iterations is an approximation to account for needing to guide the terrain farther from the edge
        // the bumpier the terrain is. Terrain may still get too bumpy.
        let top = middle_air_start - (amplitude * iterations as f64) as CoordinateType;
        let bottom = s_dimensions - top;
        let min = top - GUIDE_MIN;

        // X.
        let mut x_coefficient = 0.0;
        let mut x_height = 0.0;
        if block_coords.x > min || block_coords.x < s_dimensions - min {
            let x_coord = if block_coords.x > s_dimensions / 2 { top } else { bottom };
            let x_seed = match block_up {
                BlockFace::Front => (x_coord, block_coords.y.clamp(bottom, top), top),
                BlockFace::Back => (x_coord, block_coords.y.clamp(bottom, top), bottom),
                BlockFace::Top => (x_coord, top, block_coords.z.clamp(bottom, top)),
                BlockFace::Bottom => (x_coord, bottom, block_coords.z.clamp(bottom, top)),
                BlockFace::Right => (x_coord, block_coords.y, block_coords.z),
                BlockFace::Left => (x_coord, block_coords.y, block_coords.z),
            }
            .into();
            x_height = self::get_block_height(
                noise_generator,
                x_seed,
                structure_coords,
                middle_air_start,
                amplitude,
                delta,
                iterations,
            );
            x_coefficient = Self::get_mirror_coefficient(block_coords.x, x_height as CoordinateType, s_dimensions);
        }

        // Y.
        let mut y_coefficient = 0.0;
        let mut y_height = 0.0;
        if block_coords.y > min || block_coords.y < s_dimensions - min {
            let y_coord = if block_coords.y > s_dimensions / 2 { top } else { bottom };
            let y_seed = match block_up {
                BlockFace::Front => (block_coords.x.clamp(bottom, top), y_coord, top),
                BlockFace::Back => (block_coords.x.clamp(bottom, top), y_coord, bottom),
                BlockFace::Top => (block_coords.x, y_coord, block_coords.z),
                BlockFace::Bottom => (block_coords.x, y_coord, block_coords.z),
                BlockFace::Right => (top, y_coord, block_coords.z.clamp(bottom, top)),
                BlockFace::Left => (bottom, y_coord, block_coords.z.clamp(bottom, top)),
            }
            .into();
            y_height = self::get_block_height(
                noise_generator,
                y_seed,
                structure_coords,
                middle_air_start,
                amplitude,
                delta,
                iterations,
            );
            y_coefficient = Self::get_mirror_coefficient(block_coords.y, y_height as CoordinateType, s_dimensions);
        }

        // Z.
        let mut z_coefficient = 0.0;
        let mut z_height = 0.0;
        if block_coords.z > min || block_coords.z < s_dimensions - min {
            let z_coord = if block_coords.z > s_dimensions / 2 { top } else { bottom };
            let z_seed = match block_up {
                BlockFace::Front => (block_coords.x, block_coords.y, z_coord),
                BlockFace::Back => (block_coords.x, block_coords.y, z_coord),
                BlockFace::Top => (block_coords.x.clamp(bottom, top), top, z_coord),
                BlockFace::Bottom => (block_coords.x.clamp(bottom, top), bottom, z_coord),
                BlockFace::Right => (top, block_coords.y.clamp(bottom, top), z_coord),
                BlockFace::Left => (bottom, block_coords.y.clamp(bottom, top), z_coord),
            }
            .into();
            z_height = self::get_block_height(
                noise_generator,
                z_seed,
                structure_coords,
                middle_air_start,
                amplitude,
                delta,
                iterations,
            );
            z_coefficient = Self::get_mirror_coefficient(block_coords.z, z_height as CoordinateType, s_dimensions);
        }

        match block_up {
            BlockFace::Front | BlockFace::Back => Self::merge(z_height, x_coefficient, x_height, y_coefficient, y_height),
            BlockFace::Top | BlockFace::Bottom => Self::merge(y_height, x_coefficient, x_height, z_coefficient, z_height),
            BlockFace::Right | BlockFace::Left => Self::merge(x_height, y_coefficient, y_height, z_coefficient, z_height),
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
        block_coords: BlockCoordinate,
        structure_coords: (f64, f64, f64),
        s_dimensions: CoordinateType,
        noise_generator: &noise::OpenSimplex,
        middle_air_start: CoordinateType,
        amplitude: f64,
        delta: f64,
        iterations: usize,
    ) -> CoordinateType {
        Self::guide(
            noise_generator,
            block_up,
            block_coords,
            structure_coords,
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
    sea_level: Option<CoordinateType>,
}

/// Stores the blocks and all the noise information for creating the top of their layer.
/// For example, the "stone" BlockLevel has the noise paramters that create the boundry between dirt and stone.
#[derive(Clone, Debug)]
pub struct BlockLayer {
    middle_depth: CoordinateType,
    delta: f64,
    amplitude: f64,
    iterations: usize,
}

impl BlockLayer {
    /// This layer doesn't use a noise function to generate its span, and is thus fixed at a certain depth.
    pub fn fixed_layer(middle_depth: CoordinateType) -> Self {
        Self {
            middle_depth,
            delta: 0.0,
            amplitude: 0.0,
            iterations: 0,
        }
    }

    /// This layer is based off a noise function and will appear at a varying depth based on the parameters
    pub fn noise_layer(middle_depth: CoordinateType, delta: f64, amplitude: f64, iterations: usize) -> Self {
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
        middle_depth: CoordinateType,
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
        middle_depth: CoordinateType,
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
        sea_level: CoordinateType,
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
        height: CoordinateType,
        block_layers: &[(&'a Block, CoordinateType)],
        sea_level: Option<CoordinateType>,
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
        j_height: CoordinateType,
        k_height: CoordinateType,
        j_layers: &[(&'a Block, CoordinateType)],
        k_layers: &[(&'a Block, CoordinateType)],
        sea_level: Option<CoordinateType>,
        sea_block: Option<&'a Block>,
    ) -> Option<&'a Block> {
        for (index, (block, j_layer_top)) in j_layers.iter().enumerate().rev() {
            if j_height <= *j_layer_top && k_height <= k_layers[index].1 {
                return Some(*block);
            }
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
        x_height: CoordinateType,
        y_height: CoordinateType,
        z_height: CoordinateType,
        x_layers: &[(&'a Block, CoordinateType)],
        y_layers: &[(&'a Block, CoordinateType)],
        z_layers: &[(&'a Block, CoordinateType)],
        sea_level: Option<CoordinateType>,
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
            let coords = ev.get_chunk_coordinates();

            if let Ok((mut structure, _)) = query.get_mut(structure_entity) {
                let Structure::Dynamic(planet) = structure.as_mut() else {
                    panic!("A planet must be dynamic!");
                };
                Some((structure_entity, planet.take_or_create_chunk_for_loading(coords)))
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

            let Structure::Dynamic(planet) = structure else {
                panic!("A planet must be dynamic!");
            };

            let s_dimensions = planet.dimensions();
            let location = *location;

            Some((chunk, s_dimensions, location, structure_entity))
        })
        .collect::<Vec<(Chunk, CoordinateType, Location, Entity)>>();

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
                let first_block_coord = chunk.chunk_coordinates().first_structure_block();

                // Get all possible planet faces from the chunk corners.
                let chunk_faces = Planet::chunk_planet_faces(first_block_coord, s_dimensions);
                match chunk_faces {
                    ChunkFaces::Face(up) => {
                        generate_face_chunk::<S, T>(
                            first_block_coord,
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
                            first_block_coord,
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
                            first_block_coord,
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
                timer.log_duration("Chunk:");
                (chunk, structure_entity)
            });

            generating.generating.push(GeneratingChunk::new(task));
        }
    }
}
