//! Creates a grass planet

use bevy::{
    prelude::{
        App, Commands, Component, DespawnRecursiveExt, Entity, EventReader, EventWriter,
        IntoSystemConfigs, OnUpdate, Query, Res,
    },
    tasks::AsyncComputeTaskPool,
};
use cosmos_core::{
    block::{Block, BlockFace},
    physics::location::Location,
    registry::Registry,
    structure::{
        chunk::{Chunk, CHUNK_DIMENSIONS},
        planet::Planet,
        ChunkInitEvent, Structure,
    },
    utils::resource_wrapper::ResourceWrapper,
};
use futures_lite::future;
use noise::NoiseFn;

use crate::GameState;

use super::{
    register_biosphere, GeneratingChunk, TBiosphere, TGenerateChunkEvent, TemperatureRange,
};

#[derive(Component, Debug, Default)]
/// Marks that this is for a grass biosphere
pub struct GrassBiosphereMarker;

/// Marks that a grass chunk needs generated
pub struct GrassChunkNeedsGeneratedEvent {
    x: usize,
    y: usize,
    z: usize,
    structure_entity: Entity,
}

impl TGenerateChunkEvent for GrassChunkNeedsGeneratedEvent {
    fn new(x: usize, y: usize, z: usize, structure_entity: Entity) -> Self {
        Self {
            x,
            y,
            z,
            structure_entity,
        }
    }
}

#[derive(Default, Debug)]
/// Creates a grass planet
pub struct GrassBiosphere;

impl TBiosphere<GrassBiosphereMarker, GrassChunkNeedsGeneratedEvent> for GrassBiosphere {
    fn get_marker_component(&self) -> GrassBiosphereMarker {
        GrassBiosphereMarker {}
    }

    fn get_generate_chunk_event(
        &self,
        x: usize,
        y: usize,
        z: usize,
        structure_entity: Entity,
    ) -> GrassChunkNeedsGeneratedEvent {
        GrassChunkNeedsGeneratedEvent::new(x, y, z, structure_entity)
    }
}

const AMPLITUDE: f64 = 7.0;
const DELTA: f64 = 0.05;
const ITERATIONS: usize = 9;

const STONE_LIMIT: usize = 4;

fn get_max_level(
    x: usize,
    y: usize,
    z: usize,
    structure_x: f64,
    structure_y: f64,
    structure_z: f64,
    noise_generastor: &noise::OpenSimplex,
    middle_air_start: usize,
) -> usize {
    let mut depth: f64 = 0.0;
    for iteration in 1..=ITERATIONS {
        let iteration = iteration as f64;
        depth += noise_generastor.get([
            (x as f64 + structure_x) * (DELTA / iteration),
            (y as f64 + structure_y) * (DELTA / iteration),
            (z as f64 + structure_z) * (DELTA / iteration),
        ]) * AMPLITUDE
            * iteration;
    }
    (middle_air_start as f64 + depth).round() as usize
}

fn notify_when_done_generating(
    mut query: Query<(Entity, &mut GeneratingChunk<GrassBiosphereMarker>)>,
    mut commands: Commands,
    mut event_writer: EventWriter<ChunkInitEvent>,
    mut structure_query: Query<&mut Structure>,
) {
    for (entity, mut generating_chunk) in query.iter_mut() {
        if let Some(chunks) = future::block_on(future::poll_once(&mut generating_chunk.task)) {
            commands.entity(entity).despawn_recursive();

            for (chunk, structure_entity) in chunks {
                if let Ok(mut structure) = structure_query.get_mut(structure_entity) {
                    let (x, y, z) = (
                        chunk.structure_x(),
                        chunk.structure_y(),
                        chunk.structure_z(),
                    );

                    structure.set_chunk(chunk);

                    event_writer.send(ChunkInitEvent {
                        structure_entity,
                        x,
                        y,
                        z,
                    });
                }
            }
        }
    }
}

fn generate_planet(
    mut query: Query<(&mut Structure, &Location)>,
    mut events: EventReader<GrassChunkNeedsGeneratedEvent>,
    noise_generator: Res<ResourceWrapper<noise::OpenSimplex>>,
    blocks: Res<Registry<Block>>,
    mut commands: Commands,
) {
    let chunks = events
        .iter()
        .take(200)
        .filter_map(|ev| {
            if let Ok((mut structure, _)) = query.get_mut(ev.structure_entity) {
                Some((
                    ev.structure_entity,
                    structure.take_or_create_chunk_for_loading(ev.x, ev.y, ev.z),
                ))
            } else {
                None
            }
        })
        .collect::<Vec<(Entity, Chunk)>>();

    let grass = blocks.from_id("cosmos:grass").unwrap();
    let dirt = blocks.from_id("cosmos:dirt").unwrap();
    let stone = blocks.from_id("cosmos:stone").unwrap();

    let thread_pool = AsyncComputeTaskPool::get();

    let grass = grass.clone();
    let dirt = dirt.clone();
    let stone = stone.clone();
    // Not super expensive, only copies about 256 8 bit values.
    // Still not ideal though.
    let noise_generator = **noise_generator;

    let chunks = chunks
        .into_iter()
        .flat_map(|(structure_entity, chunk)| {
            let Ok((structure, location)) = query.get(structure_entity) else {
                return None;
            };

            let s_width = structure.blocks_width();
            let s_height = structure.blocks_height();
            let s_length = structure.blocks_length();
            let location = *location;

            Some((
                chunk,
                s_width,
                s_height,
                s_length,
                location,
                structure_entity,
            ))
        })
        .collect::<Vec<(Chunk, usize, usize, usize, Location, Entity)>>();

    if !chunks.is_empty() {
        println!("Pooling {} chunks!", chunks.len());

        let task = thread_pool.spawn(async move {
            let mut done_chunks = Vec::with_capacity(chunks.len());

            for (mut chunk, s_width, s_height, s_length, location, structure_entity) in chunks {
                let grass = &grass;
                let dirt = &dirt;
                let stone = &stone;

                let middle_air_start = s_height - CHUNK_DIMENSIONS * 5;

                let actual_pos = location.absolute_coords_f64();

                let structure_z = actual_pos.z;
                let structure_y = actual_pos.y;
                let structure_x = actual_pos.x;

                for z in 0..CHUNK_DIMENSIONS {
                    let actual_z = chunk.structure_z() * CHUNK_DIMENSIONS + z;
                    for y in 0..CHUNK_DIMENSIONS {
                        let actual_y: usize = chunk.structure_y() * CHUNK_DIMENSIONS + y;
                        for x in 0..CHUNK_DIMENSIONS {
                            if chunk.has_block_at(x, y, z) {
                                continue;
                            }

                            let actual_x = chunk.structure_x() * CHUNK_DIMENSIONS + x;

                            let current_max = get_max_level(
                                actual_x,
                                actual_y,
                                actual_z,
                                structure_x,
                                structure_y,
                                structure_z,
                                &noise_generator,
                                middle_air_start,
                            );

                            let mut cover_x = actual_x as i64;
                            let mut cover_y = actual_y as i64;
                            let mut cover_z = actual_z as i64;
                            let block_up = Planet::planet_face_without_structure(
                                actual_x, actual_y, actual_z, s_width, s_height, s_length,
                            );

                            let current_height = match block_up {
                                BlockFace::Top => {
                                    cover_y += 1;
                                    actual_y
                                }
                                BlockFace::Bottom => {
                                    cover_y -= 1;
                                    s_height - actual_y
                                }
                                BlockFace::Front => {
                                    cover_z += 1;
                                    actual_z
                                }
                                BlockFace::Back => {
                                    cover_z -= 1;
                                    s_height - actual_z
                                }
                                BlockFace::Right => {
                                    cover_x += 1;
                                    actual_x
                                }
                                BlockFace::Left => {
                                    cover_x -= 1;
                                    s_height - actual_x
                                }
                            };

                            if current_height < current_max - STONE_LIMIT {
                                chunk.set_block_at(x, y, z, stone, block_up);
                            } else if current_height < current_max {
                                // Getting the noise values for the "covering" block.
                                let cover_height = current_height + 1;

                                let cover_max = if cover_x < 0 || cover_y < 0 || cover_z < 0 {
                                    0
                                } else {
                                    get_max_level(
                                        cover_x as usize,
                                        cover_y as usize,
                                        cover_z as usize,
                                        structure_x,
                                        structure_y,
                                        structure_z,
                                        &noise_generator,
                                        middle_air_start,
                                    )
                                };

                                if cover_height < cover_max {
                                    // In dirt range and covered -> dirt.
                                    chunk.set_block_at(x, y, z, dirt, block_up)
                                } else {
                                    // In dirt range and uncovered -> grass.
                                    chunk.set_block_at(x, y, z, grass, block_up)
                                }
                            }
                        }
                    }
                }

                done_chunks.push((chunk, structure_entity));
            }

            done_chunks
        });

        commands.spawn(GeneratingChunk::<GrassBiosphereMarker>::new(task));
    }
}

pub(super) fn register(app: &mut App) {
    register_biosphere::<GrassBiosphereMarker, GrassChunkNeedsGeneratedEvent>(
        app,
        "cosmos:biosphere_grass",
        TemperatureRange::new(0.0, 1000000000.0),
    );

    app.add_systems(
        (generate_planet, notify_when_done_generating).in_set(OnUpdate(GameState::Playing)),
    );
}
