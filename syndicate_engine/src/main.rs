mod engine;
mod game;

use engine::{assets::AssetIndex, config::window_conf};
use game::world::WorldState;
use macroquad::prelude::*;

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
