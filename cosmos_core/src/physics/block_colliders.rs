//! Assigns each block their respective collider

use std::f32::consts::PI;

use bevy::{
    math::Quat,
    prelude::{App, IntoSystemConfigs, OnEnter, Res, ResMut, States, Vec3},
};
use bevy_rapier3d::prelude::Collider;

use crate::{
    block::Block,
    registry::{create_registry, identifiable::Identifiable, Registry},
};

#[derive(Debug, Clone, Copy)]
/// How the collider interacts with the world
pub enum BlockColliderMode {
    /// This type of collider will be physically interact with other colliders
    NormalCollider,
    /// This type of collider will not physically interact with the world, but can still be used in raycasts + other physics calculations
    SensorCollider,
}

#[derive(Debug, Clone)]
/// A custom collider a block may have
///
/// Note that this should not go outside the bounds of the block, or breaking/placing will not work when you are targetting this collider.
pub struct CustomCollider {
    /// How far away this collider's origin is from the center of this block
    pub offset: Vec3,
    /// Collider's rotation
    pub rotation: Quat,
    /// The collider to use
    pub collider: Collider,
    /// What mode this collider should be treated with
    pub mode: BlockColliderMode,
}

#[derive(Debug, Clone)]
/// The collider that should be used for this face
pub struct FaceColldier {
    /// Use this collider if this face isn't connected to anything
    pub non_connected: Vec<CustomCollider>,
    /// Use this collider if this face is connected to something
    pub connected: Vec<CustomCollider>,
}

#[derive(Debug, Clone)]
/// Represents a collider that will change when this is connected to other blocks
pub struct ConnectedCollider {
    /// Face's collider
    pub right: FaceColldier,
    /// Face's collider
    pub left: FaceColldier,
    /// Face's collider
    pub top: FaceColldier,
    /// Face's collider
    pub bottom: FaceColldier,
    /// Face's collider
    pub front: FaceColldier,
    /// Face's collider
    pub back: FaceColldier,
}

#[derive(Debug, Clone)]
/// The type of collider a block has
pub enum BlockColliderType {
    /// Takes an entire block
    Full(BlockColliderMode),
    /// A custom collider that is more complex than the default options
    Custom(Vec<CustomCollider>),
    /// Represents a collider that will change when this is connected to other blocks
    Connected(Box<ConnectedCollider>),
    /// No collider at all
    Empty,
}

#[derive(Debug, Clone)]
/// Determines how a block interacts with its physics environment
pub struct BlockCollider {
    /// What type of collider this is
    pub collider: BlockColliderType,
    id: u16,
    unlocalized_name: String,
}

impl BlockCollider {
    /// The unlocalized_name field should be the block this is a collider for.
    pub fn new(collider: BlockColliderType, block_unlocalized_name: impl Into<String>) -> Self {
        Self {
            collider,
            id: 0,
            unlocalized_name: block_unlocalized_name.into(),
        }
    }
}

fn register_custom_colliders(blocks: Res<Registry<Block>>, mut registry: ResMut<Registry<BlockCollider>>) {
    registry.register(BlockCollider::new(BlockColliderType::Empty, "cosmos:air"));

    const EPSILON: f32 = 0.001;

    if blocks.contains("cosmos:short_grass") {
        registry.register(BlockCollider::new(
            BlockColliderType::Custom(vec![CustomCollider {
                collider: Collider::cuboid(0.5, 0.2, 0.5),
                mode: BlockColliderMode::SensorCollider,
                rotation: Quat::IDENTITY,
                offset: Vec3::new(0.0, -(0.5 - 0.2), 0.0),
            }]),
            "cosmos:short_grass",
        ));
    }

    if blocks.contains("cosmos:ramp") {
        registry.register(BlockCollider::new(
            BlockColliderType::Custom(vec![
                // top
                CustomCollider {
                    rotation: Quat::from_axis_angle(Vec3::X, -PI / 4.0),
                    collider: Collider::cuboid(0.5, EPSILON, 2.0f32.sqrt() / 2.0),
                    mode: BlockColliderMode::NormalCollider,
                    offset: Vec3::ZERO,
                },
                // left
                CustomCollider {
                    rotation: Quat::IDENTITY,
                    collider: Collider::triangle(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(-0.5, -0.5, 0.5), Vec3::new(-0.5, 0.5, 0.5)),
                    mode: BlockColliderMode::NormalCollider,
                    offset: Vec3::ZERO,
                },
                // right
                CustomCollider {
                    rotation: Quat::IDENTITY,
                    collider: Collider::triangle(Vec3::new(0.5, -0.5, -0.5), Vec3::new(0.5, -0.5, 0.5), Vec3::new(0.5, 0.5, 0.5)),
                    mode: BlockColliderMode::NormalCollider,
                    offset: Vec3::ZERO,
                },
                // bottom
                CustomCollider {
                    rotation: Quat::IDENTITY,
                    collider: Collider::cuboid(0.5, EPSILON, 0.5),
                    mode: BlockColliderMode::NormalCollider,
                    offset: Vec3::new(0.0, -0.5 + EPSILON, 0.0),
                },
                // front
                CustomCollider {
                    rotation: Quat::IDENTITY,
                    collider: Collider::cuboid(0.5, 0.5, EPSILON),
                    mode: BlockColliderMode::NormalCollider,
                    offset: Vec3::new(0.0, 0.0, 0.5 + EPSILON),
                },
            ]),
            "cosmos:ramp",
        ));
    }

    if blocks.contains("cosmos:power_cable") {
        registry.register(BlockCollider::new(
            BlockColliderType::Connected(Box::new(ConnectedCollider {
                top: FaceColldier {
                    non_connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, EPSILON, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, 0.2, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                    connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, 0.25, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, 0.25, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                },
                bottom: FaceColldier {
                    non_connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, EPSILON, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, -0.2 - EPSILON, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                    connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, 0.25, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, -0.25, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                },
                front: FaceColldier {
                    non_connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, 0.2, EPSILON),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, 0.0, 0.2),
                        rotation: Quat::IDENTITY,
                    }],
                    connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, 0.2, 0.25),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, 0.0, 0.25),
                        rotation: Quat::IDENTITY,
                    }],
                },
                back: FaceColldier {
                    non_connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, 0.2, EPSILON),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, 0.0, -0.2 - EPSILON),
                        rotation: Quat::IDENTITY,
                    }],
                    connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.2, 0.2, 0.25),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.0, 0.0, -0.25),
                        rotation: Quat::IDENTITY,
                    }],
                },
                right: FaceColldier {
                    non_connected: vec![CustomCollider {
                        collider: Collider::cuboid(EPSILON, 0.2, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.2, 0.0, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                    connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.25, 0.2, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(0.25, 0.0, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                },
                left: FaceColldier {
                    non_connected: vec![CustomCollider {
                        collider: Collider::cuboid(EPSILON, 0.2, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(-0.2 - EPSILON, 0.0, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                    connected: vec![CustomCollider {
                        collider: Collider::cuboid(0.25, 0.2, 0.2),
                        mode: BlockColliderMode::NormalCollider,
                        offset: Vec3::new(-0.25, 0.0, 0.0),
                        rotation: Quat::IDENTITY,
                    }],
                },
            })),
            "cosmos:power_cable",
        ));
    }
}

fn register_all_colliders(blocks: Res<Registry<Block>>, mut registry: ResMut<Registry<BlockCollider>>) {
    for block in blocks.iter() {
        if registry.from_id(block.unlocalized_name()).is_none() {
            registry.register(BlockCollider::new(
                BlockColliderType::Full(BlockColliderMode::NormalCollider),
                block.unlocalized_name(),
            ));
        }
    }
}

impl Identifiable for BlockCollider {
    fn id(&self) -> u16 {
        self.id
    }

    fn set_numeric_id(&mut self, id: u16) {
        self.id = id;
    }

    fn unlocalized_name(&self) -> &str {
        self.unlocalized_name.as_str()
    }
}

pub(super) fn register<T: States + Copy>(app: &mut App, post_loading_state: T) {
    create_registry::<BlockCollider>(app, "cosmos:block_colliders");

    app.add_systems(
        OnEnter(post_loading_state),
        (register_custom_colliders, register_all_colliders).chain(),
    );
}
