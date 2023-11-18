//! Load a cubemap texture onto a cube like a skybox and cycle through different compressed texture formats

use bevy::{
    asset::LoadState,
    core_pipeline::Skybox,
    prelude::*,
    render::render_resource::{TextureViewDescriptor, TextureViewDimension},
};

/// Order from top to bottom:
/// Right, Left, Top, Bottom, Front, Back
const CUBEMAP: &str = "skybox/skybox.png";

#[derive(Resource)]
struct Cubemap {
    is_loaded: bool,
    image_handle: Handle<Image>,
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let skybox_handle = asset_server.load(CUBEMAP);

    commands.insert_resource(Cubemap {
        is_loaded: false,
        image_handle: skybox_handle,
    });
}

fn added_skybox(mut query: Query<&mut Skybox, Added<Skybox>>, cubemap: Res<Cubemap>) {
    for mut skybox in query.iter_mut() {
        if cubemap.is_loaded {
            skybox.0 = cubemap.image_handle.clone();
        }
    }
}

fn asset_loaded(
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut cubemap: ResMut<Cubemap>,
    mut skyboxes: Query<&mut Skybox>,
) {
    if !cubemap.is_loaded && asset_server.get_load_state(cubemap.image_handle.clone_weak()) == Some(LoadState::Loaded) {
        let image = images.get_mut(&cubemap.image_handle).unwrap();
        // NOTE: PNGs do not have any metadata that could indicate they contain a cubemap texture,
        // so they appear as one texture. The following code reconfigures the texture as necessary.
        if image.texture_descriptor.array_layer_count() == 1 {
            image.reinterpret_stacked_2d_as_array(image.texture_descriptor.size.height / image.texture_descriptor.size.width);
            image.texture_view_descriptor = Some(TextureViewDescriptor {
                dimension: Some(TextureViewDimension::Cube),
                ..default()
            });
        }

        for mut skybox in skyboxes.iter_mut() {
            skybox.0 = cubemap.image_handle.clone();
        }

        cubemap.is_loaded = true;
    }
}

pub(super) fn register(app: &mut App) {
    app //.add_plugin(MaterialPlugin::<CubemapMaterial>::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (added_skybox, asset_loaded));
}
