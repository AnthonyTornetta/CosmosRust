//! Temporary: generates default shop prices

use bevy::{
    app::App,
    ecs::{schedule::OnEnter, system::Res},
};
use cosmos_core::{item::Item, registry::Registry};

use crate::state::GameState;

fn create_default_shop_entires(_items: Res<Registry<Item>>) {
    /*
    cosmos:grass=Grass
    cosmos:stone=Stone
    cosmos:dirt=Dirt
    cosmos:log=Log
    cosmos:laser_cannon=Laser Cannon
    cosmos:cherry_leaf=Cherry Leaf
    cosmos:redwood_log=Redwood Log
    cosmos:redwood_leaf=Redwood Leaf
    cosmos:ship_core=Ship Core
    cosmos:energy_cell=Energy Cell
    cosmos:reactor=Reactor
    cosmos:thruster=Thruster
    cosmos:light=Light
    cosmos:glass=Glass
    cosmos:molten_stone=Molten Stone
    cosmos:cheese=Cheese (Lava)
    cosmos:ice=Ice
    cosmos:water=Water
    cosmos:sand=Sand
    cosmos:cactus=Cactus
    cosmos:build_block=Build Block

    cosmos:ship_hull_grey=Grey Ship Hull
    cosmos:ship_hull_black=Black Ship Hull
    cosmos:ship_hull_dark_grey=Dark Grey Ship Hull
    cosmos:ship_hull_white=White Ship Hull
    cosmos:ship_hull_blue=Blue Ship Hull
    cosmos:ship_hull_dark_blue=Dark Blue Ship Hull
    cosmos:ship_hull_brown=Brown Ship Hull
    cosmos:ship_hull_green=Green Ship Hull
    cosmos:ship_hull_dark_green=Dark Green Ship Hull
    cosmos:ship_hull_orange=Orange Ship Hull
    cosmos:ship_hull_dark_orange=Dark Orange Ship Hull
    cosmos:ship_hull_pink=Pink Ship Hull
    cosmos:ship_hull_dark_pink=Dark Pink Ship Hull
    cosmos:ship_hull_purple=Purple Ship Hull
    cosmos:ship_hull_dark_purple=Dark Purple Ship Hull
    cosmos:ship_hull_red=Red Ship Hull
    cosmos:ship_hull_dark_red=Dark Red Ship Hull
    cosmos:ship_hull_yellow=Yellow Ship Hull
    cosmos:ship_hull_dark_yellow=Dark Yellow Ship Hull
    cosmos:ship_hull_mint=Mint Ship Hull

    cosmos:glass_white=White Glass
    cosmos:glass_blue=Blue Glass
    cosmos:glass_dark_blue=Dark Blue Glass
    cosmos:glass_brown=Brown Glass
    cosmos:glass_green=Green Glass
    cosmos:glass_dark_green=Dark Green Glass
    cosmos:glass_orange=Orange Glass
    cosmos:glass_dark_orange=Dark Orange Glass
    cosmos:glass_pink=Pink Glass
    cosmos:glass_dark_pink=Dark Pink Glass
    cosmos:glass_purple=Purple Glass
    cosmos:glass_dark_purple=Dark Purple Glass
    cosmos:glass_red=Red Glass
    cosmos:glass_dark_red=Dark Red Glass
    cosmos:glass_yellow=Yellow Glass
    cosmos:glass_dark_yellow=Dark Yellow Glass
    cosmos:glass_mint=Mint Glass

    cosmos:reactor_controller=Reactor Controller
    cosmos:reactor_casing=Reactor Casing
    cosmos:reactor_window=Reactor Window
    cosmos:reactor_cell=Reactor Power Cell
    cosmos:fan=Fan
    cosmos:storage=Storage
    cosmos:station_core=Station Core
    cosmos:test_ore=Test Ore
    cosmos:plasma_drill=Plasma Drill
    cosmos:shop=Shop */

    // ShopEntry::Buying {
    //     item_id: (),
    //     max_quantity_buying: (),
    //     price_per: (),
    // }
}

pub(super) fn register(app: &mut App) {
    app.add_systems(OnEnter(GameState::Playing), create_default_shop_entires);
}
