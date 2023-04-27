use super::Randr;

pub struct Dummy;

impl Randr for Dummy {
    fn get_monitors(&self) -> Vec<super::Monitor> {
        Vec::new()
    }

    fn configure(&self, _mon: &super::Monitor, _config: super::Configure) {}
}
