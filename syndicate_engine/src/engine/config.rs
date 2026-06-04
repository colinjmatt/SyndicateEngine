use macroquad::prelude::*;

pub fn window_conf() -> Conf {
    Conf {
        window_title: "SyndicateEngine - clean-room tactical prototype".to_string(),
        window_width: 1440,
        window_height: 900,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}
