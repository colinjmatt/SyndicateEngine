use macroquad::prelude::*;
use syndicate_engine::{
    engine::{assets::AssetIndex, config::window_conf},
    game::world::WorldState,
};

#[macroquad::main(window_conf)]
async fn main() {
    let asset_index = AssetIndex::discover("../original_assets");
    let mut world = WorldState::new(asset_index);

    loop {
        clear_background(Color::from_rgba(7, 10, 13, 255));
        world.update(get_frame_time());
        world.draw();
        next_frame().await;
    }
}
