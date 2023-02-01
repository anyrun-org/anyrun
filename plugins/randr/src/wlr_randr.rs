use std::process::Command;

use crate::randr::{Mode, Monitor, Randr};

pub struct WlrRandr {
    pub exec: String,
}

impl WlrRandr {
    fn run_wlr_randr(&self, args: &[&str]) -> String {
        String::from_utf8(Command::new(self.exec).args(args).output().unwrap().stdout).unwrap()
    }
}

impl Randr for WlrRandr {
    fn get_monitors(&self) -> Vec<Monitor> {
        let output = self.run_wlr_randr(&[]);
        todo!()
    }

    fn set_mode(&self, mon: &Monitor, mode: &Mode) -> Result<(), String> {
        todo!()
    }
}
