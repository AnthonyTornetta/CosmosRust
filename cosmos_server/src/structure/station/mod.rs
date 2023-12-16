//! Contains server-related station logic

use bevy::prelude::App;

pub mod events;
pub mod loading;
mod persistence;
pub mod server_station_builder;
mod sync;

pub(super) fn register(app: &mut App) {
    events::register(app);
    loading::register(app);
    sync::register(app);
    persistence::register(app);
}
