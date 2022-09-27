use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use cosmos_core::{
    block::blocks::Blocks,
    events::block_events::BlockChangedEvent,
    structure::{events::StructureCreated, ship::ship_builder::TShipBuilder, structure::Structure},
};

use crate::structure::ship::server_ship_builder::ServerShipBuilder;

pub struct CreateShipEvent {
    pub ship_transform: Transform,
}

fn event_reader(
    mut created_event_writer: EventWriter<StructureCreated>,
    mut block_changed_writer: EventWriter<BlockChangedEvent>,
    mut event_reader: EventReader<CreateShipEvent>,
    mut commands: Commands,
    blocks: Res<Blocks>,
) {
    for ev in event_reader.iter() {
        let mut entity = commands.spawn();

        let mut structure = Structure::new(10, 10, 10, entity.id());

        let builder = ServerShipBuilder::default();

        builder.insert_ship(
            &mut entity,
            ev.ship_transform.clone(),
            Velocity::zero(),
            &mut structure,
        );

        let block = blocks.block_from_id("cosmos:ship_core");

        structure.set_block_at(
            structure.blocks_width() / 2,
            structure.blocks_height() / 2,
            structure.blocks_length() / 2,
            block,
            &blocks,
            Some(&mut block_changed_writer),
        );

        entity.insert(structure);

        created_event_writer.send(StructureCreated {
            entity: entity.id(),
        });
    }
}

pub fn register(app: &mut App) {
    app.add_event::<CreateShipEvent>().add_system(event_reader);
}
