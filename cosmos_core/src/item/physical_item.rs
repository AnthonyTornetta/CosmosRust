//! Items that are thrown on the ground

use bevy::app::Update;
use bevy::core::Name;
use bevy::prelude::{Added, App, Commands, Entity, IntoSystemConfigs, Query};
use bevy::{prelude::Component, reflect::Reflect};
use bevy_rapier3d::prelude::{Collider, RigidBody};
use serde::{Deserialize, Serialize};

use crate::netty::sync::{sync_component, IdentifiableComponent, SyncableComponent};
use crate::netty::system_sets::NetworkingSystemsSet;

#[derive(Component, Reflect, Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
/// An item that is currently in the physical world (ie a dropped item)
pub struct PhysicalItem;

impl IdentifiableComponent for PhysicalItem {
    fn get_component_unlocalized_name() -> &'static str {
        "cosmos:physical_item"
    }
}

impl SyncableComponent for PhysicalItem {
    fn get_sync_type() -> crate::netty::sync::SyncType {
        crate::netty::sync::SyncType::ServerAuthoritative
    }
}

fn on_add_physical_item(mut commands: Commands, q_added: Query<Entity, Added<PhysicalItem>>) {
    for ent in q_added.iter() {
        commands
            .entity(ent)
            .insert((RigidBody::Dynamic, Collider::cuboid(0.1, 0.1, 0.1), Name::new("Physical Item")));
    }
}

pub(super) fn register(app: &mut App) {
    sync_component::<PhysicalItem>(app);

    app.add_systems(Update, on_add_physical_item.in_set(NetworkingSystemsSet::Between));
}
