/*!
The crate for building plugins for Anyrun.

Each plugin needs 4 functions defined, `init`, `info`, `get_matches` and the `handler`. Documentation
on what each of these should be is found in their respective attribute macros.
!*/

pub use anyrun_interface::{self, HandleResult, Match, PluginInfo};
pub use anyrun_macros::{get_matches, handler, info, init};

/*
The macro to create a plugin, handles asynchronous execution of getting the matches and the boilerplate
for integrating with `stable_abi`.

# Arguments


* `$type`: The type of the shared data to be provided to various functions.
*/
