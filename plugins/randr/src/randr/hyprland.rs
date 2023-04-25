use hyprland::{
    data,
    keyword::Keyword,
    shared::{HyprData, HyprDataVec},
};

use super::{Configure, Monitor, Randr};

pub struct Hyprland {
    monitors: Vec<data::Monitor>,
}

impl Hyprland {
    pub fn new() -> Self {
        Self {
            monitors: data::Monitors::get().unwrap().to_vec(),
        }
    }
}

impl Randr for Hyprland {
    fn get_monitors(&self) -> Vec<Monitor> {
        self.monitors
            .iter()
            .cloned()
            .map(|mon| Monitor {
                x: mon.x,
                y: mon.y,
                width: mon.width as u32,
                height: mon.height as u32,
                refresh_rate: mon.refresh_rate,
                scale: mon.scale,
                name: mon.name,
                id: mon.id as u64,
            })
            .collect()
    }

    fn configure(&self, mon: &Monitor, config: Configure) {
        match config {
            Configure::Mirror(rel) => Keyword::set(
                "monitor",
                format!("{},preferred,auto,1,mirror,{}", mon.name, rel.name),
            )
            .expect("Failed to configure monitor"),
            Configure::LeftOf(rel) => {
                let mut x = rel.x - mon.width as i32;
                if x < 0 {
                    Keyword::set(
                        "monitor",
                        format!(
                            "{},{}x{}@{},{}x{},{}",
                            rel.name,
                            rel.width,
                            rel.height,
                            rel.refresh_rate,
                            rel.x - x,
                            rel.y,
                            rel.scale
                        ),
                    )
                    .expect("Failed to configure monitor");
                    x = 0;
                }

                Keyword::set(
                    "monitor",
                    format!(
                        "{},{}x{}@{},{}x{},{}",
                        mon.name, mon.width, mon.height, mon.refresh_rate, x, rel.y, mon.scale
                    ),
                )
                .expect("Failed to configure monitor");
            }
            Configure::RightOf(rel) => Keyword::set(
                "monitor",
                format!(
                    "{},{}x{}@{},{}x{},1",
                    mon.name,
                    mon.width,
                    mon.height,
                    mon.refresh_rate,
                    rel.x + rel.width as i32,
                    rel.y
                ),
            )
            .expect("Failed to configure monitor"),
            Configure::Below(rel) => Keyword::set(
                "monitor",
                format!(
                    "{},{}x{}@{},{}x{},{}",
                    mon.name,
                    mon.width,
                    mon.height,
                    mon.refresh_rate,
                    rel.x,
                    rel.y + rel.height as i32,
                    mon.scale
                ),
            )
            .expect("Failed to configure monitor"),
            Configure::Above(rel) => {
                let mut y = rel.y - mon.height as i32;
                if y < 0 {
                    Keyword::set(
                        "monitor",
                        format!(
                            "{},{}x{}@{},{}x{},{}",
                            rel.name,
                            rel.width,
                            rel.height,
                            rel.refresh_rate,
                            rel.x,
                            rel.y - y,
                            rel.scale
                        ),
                    )
                    .expect("Failed to configure monitor");
                    y = 0;
                }

                Keyword::set(
                    "monitor",
                    format!(
                        "{},{}x{}@{},{}x{},{}",
                        mon.name, mon.width, mon.height, mon.refresh_rate, rel.x, y, mon.scale
                    ),
                )
                .expect("Failed to configure monitor");
            }
            Configure::Zero => Keyword::set(
                "monitor",
                format!(
                    "{},{}x{}@{},0x0,{}",
                    mon.name, mon.width, mon.height, mon.refresh_rate, mon.scale
                ),
            )
            .expect("Failed to configure monitor"),
        }
    }
}
