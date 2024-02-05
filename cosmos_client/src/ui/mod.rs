//! Responsible for all the user interfaces the client can have

use bevy::prelude::App;

pub mod components;
pub mod crosshair;
pub mod debug_info_display;
pub mod hotbar;
pub mod item_renderer;
pub mod message;
mod ship_flight;

pub(super) fn register(app: &mut App) {
    crosshair::register(app);
    hotbar::register(app);
    debug_info_display::register(app);
    item_renderer::register(app);
    message::register(app);
    ship_flight::register(app);
    components::register(app);
}
