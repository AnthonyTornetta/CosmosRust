use bevy::prelude::App;

pub mod blocks;
pub mod recipes;

pub(super) fn register(app: &mut App) {
    recipes::register(app);
    blocks::register(app);
}
