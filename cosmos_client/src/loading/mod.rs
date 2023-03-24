use bevy::prelude::{
    App, Commands, DespawnRecursiveExt, Entity, IntoSystemConfig, OnUpdate, Query, With, Without,
};
use cosmos_core::physics::location::{Location, SECTOR_DIMENSIONS};

use crate::{netty::flags::LocalPlayer, state::game_state::GameState};

const UNLOAD_DIST: f32 = SECTOR_DIMENSIONS * 10.0;

fn unload_far_entities(
    query: Query<(Entity, &Location), Without<LocalPlayer>>,
    my_loc: Query<&Location, With<LocalPlayer>>,
    mut commands: Commands,
) {
    if let Ok(my_loc) = my_loc.get_single() {
        for (ent, loc) in query.iter() {
            if loc.distance_sqrd(my_loc) > UNLOAD_DIST * UNLOAD_DIST {
                println!("Unloading entity at {loc}!");
                commands.entity(ent).despawn_recursive();
            }
        }
    }
}

pub(crate) fn register(app: &mut App) {
    app.add_system(unload_far_entities.in_set(OnUpdate(GameState::Playing)));
}