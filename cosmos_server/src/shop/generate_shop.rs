//! Responsible for spawning shops across the universe

use std::time::Duration;

use bevy::{
    prelude::{in_state, App, Commands, EventReader, IntoSystemConfigs, Query, Res, ResMut, Update, Vec3, With},
    time::common_conditions::on_timer,
    utils::HashSet,
};
use cosmos_core::{
    entities::player::Player,
    physics::location::{Location, Sector, SectorUnit, SECTOR_DIMENSIONS, SYSTEM_SECTORS},
    state::GameState,
    structure::station::station_builder::STATION_LOAD_DISTANCE,
    utils::quat_math::random_quat,
};
use rand::{seq::IteratorRandom, Rng};

use crate::{
    init::init_world::ServerSeed,
    persistence::loading::{LoadingBlueprintSystemSet, NeedsBlueprintLoaded},
    rng::get_rng_for_sector,
    universe::generation::{GenerateSystemEvent, SystemGenerationSet, SystemItem, UniverseSystems},
};

fn generate_shops(
    mut systems: ResMut<UniverseSystems>,
    mut evr_generate_system: EventReader<GenerateSystemEvent>,
    server_seed: Res<ServerSeed>,
) {
    for ev in evr_generate_system.read() {
        let Some(system) = systems.system_mut(ev.system) else {
            continue;
        };

        let mut rng = get_rng_for_sector(&server_seed, &ev.system.negative_most_sector());

        let n_shops = rng.gen_range(20..=50);

        let asteroid_shops_percent = rng.gen::<f32>() * 0.5 + 0.25; // At least 25% to a max of 75%

        let asteroid_shops = (asteroid_shops_percent * n_shops as f32) as i32;
        let non_asteroid_shops = n_shops - asteroid_shops;

        let multiplier = SECTOR_DIMENSIONS;
        let adder = -SECTOR_DIMENSIONS / 2.0;

        let mut placed_shops = HashSet::default();

        for _ in 0..asteroid_shops {
            let Some(generated_item) = system
                .iter()
                .filter(|x| !placed_shops.contains(&x.location.sector()) && matches!(x.item, SystemItem::Asteroid(_)))
                .choose_stable(&mut rng)
            else {
                continue;
            };

            placed_shops.insert(generated_item.location.sector());
            let loc = Location::new(
                Vec3::new(
                    rng.gen::<f32>() * multiplier + adder,
                    rng.gen::<f32>() * multiplier + adder,
                    rng.gen::<f32>() * multiplier + adder,
                ),
                generated_item.location.sector(),
            );

            system.add_item(loc, SystemItem::Shop);
        }

        for _ in 0..non_asteroid_shops {
            let sector = Sector::new(
                rng.gen_range(0..SYSTEM_SECTORS as SectorUnit),
                rng.gen_range(0..SYSTEM_SECTORS as SectorUnit),
                rng.gen_range(0..SYSTEM_SECTORS as SectorUnit),
            ) + ev.system.negative_most_sector();

            if system.items_at(sector).next().is_some() {
                continue;
            }

            let loc = Location::new(
                Vec3::new(
                    rng.gen::<f32>() * multiplier + adder,
                    rng.gen::<f32>() * multiplier + adder,
                    rng.gen::<f32>() * multiplier + adder,
                ),
                sector,
            );

            system.add_item(loc, SystemItem::Shop);
        }
    }
}

fn spawn_shop(
    q_players: Query<&Location, With<Player>>,
    server_seed: Res<ServerSeed>,
    mut commands: Commands,
    mut systems: ResMut<UniverseSystems>,
) {
    let mut generated_shops = HashSet::new();

    for player_loc in q_players.iter() {
        let Some(system) = systems.system_mut(player_loc.get_system_coordinates()) else {
            continue;
        };

        for station_loc in system
            .iter()
            .flat_map(|x| match &x.item {
                SystemItem::Shop => Some(x.location),
                _ => None,
            })
            .filter(|x| !system.is_sector_generated_for(x.sector(), "cosmos:shop"))
        {
            if generated_shops.contains(&station_loc.sector()) {
                continue;
            }

            let sector_diff = (station_loc.sector() - player_loc.sector()).abs();
            if sector_diff.max_element() > STATION_LOAD_DISTANCE as SectorUnit {
                continue;
            }

            let mut rng = get_rng_for_sector(&server_seed, &station_loc.sector());

            commands.spawn(NeedsBlueprintLoaded {
                path: "default_blueprints/shop/default.bp".into(),
                rotation: random_quat(&mut rng),
                spawn_at: station_loc,
            });

            generated_shops.insert(station_loc.sector());
        }

        for &generated_shop in &generated_shops {
            system.mark_sector_generated_for(generated_shop, "cosmos:shop");
        }
    }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        (
            generate_shops.in_set(SystemGenerationSet::Station),
            spawn_shop.run_if(on_timer(Duration::from_secs(1))),
        )
            .chain()
            .before(LoadingBlueprintSystemSet::BeginLoadingBlueprints)
            .run_if(in_state(GameState::Playing)),
    );
}
