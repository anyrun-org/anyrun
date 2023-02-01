pub struct Mode {
    width: u32,
    height: u32,
    rate: f64,
    preferred: bool,
    current: bool,
}

pub struct Monitor {
    output: String,
    modes: Vec<Mode>,
    x: i32,
    y: i32,
    enabled: bool,
}

pub trait Randr {
    fn get_monitors(&self) -> Vec<Monitor>;
    fn set_mode(&self, mon: &Monitor, mode: &Mode) -> Result<(), String>;
}
