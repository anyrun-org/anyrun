use randr::Randr;

mod randr;
mod wlr_randr;

fn init() -> Box<dyn Randr> {}
