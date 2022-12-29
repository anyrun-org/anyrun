pub use anyrun_interface::{self, Match, PluginInfo};

/**
The macro to create a plugin, handles asynchronous execution of getting the matches and the boilerplate
for integrating with `stable_abi`.

# Arguments

* `$init`: Function that takes an `RString` as the only argument, which points to the anyrun config directory. It returns nothing.
The path is used for plugin specific config files.
**NOTE**: Should not block or block for a long time. If this blocks the main thread will too.

* `$info`: Function that returns the plugin info as a `PluginInfo` object. Takes no arguments.

* `$get_matches`: Function that takes the current text input as an `RString` as the only argument, and returns an `RVec<Match>`.
This is run asynchronously automatically.

* `$handler`: The function to handle the selection of an item. Takes a `Match` as it's only argument and returns a `HandleResult` with
the appropriate action.
**/
#[macro_export]
macro_rules! plugin {
    ($init:ident, $info:ident, $get_matches:ident, $handler:ident) => {
        mod anyrun_plugin_internal {
            static THREAD: ::std::sync::Mutex<
                Option<(
                    ::std::thread::JoinHandle<
                        ::abi_stable::std_types::RVec<::anyrun_plugin::anyrun_interface::Match>,
                    >,
                    u64,
                )>,
            > = ::std::sync::Mutex::new(None);
            static ID_COUNTER: ::std::sync::atomic::AtomicU64 =
                ::std::sync::atomic::AtomicU64::new(0);

            #[::abi_stable::export_root_module]
            fn init_root_module() -> ::anyrun_plugin::anyrun_interface::PluginRef {
                use ::abi_stable::prefix_type::PrefixTypeTrait;
                ::anyrun_plugin::anyrun_interface::Plugin {
                    init,
                    info,
                    get_matches,
                    poll_matches,
                    handle_selection,
                }
                .leak_into_prefix()
            }

            #[::abi_stable::sabi_extern_fn]
            fn init(config_dir: ::abi_stable::std_types::RString) {
                super::$init(config_dir);
            }

            #[::abi_stable::sabi_extern_fn]
            fn info() -> ::anyrun_plugin::anyrun_interface::PluginInfo {
                super::$info()
            }

            #[::abi_stable::sabi_extern_fn]
            fn get_matches(input: ::abi_stable::std_types::RString) -> u64 {
                let current_id = ID_COUNTER.load(::std::sync::atomic::Ordering::Relaxed);
                ID_COUNTER.store(current_id + 1, ::std::sync::atomic::Ordering::Relaxed);

                let handle = ::std::thread::spawn(move || super::$get_matches(input));

                *THREAD.lock().unwrap() = Some((handle, current_id));

                current_id
            }

            #[::abi_stable::sabi_extern_fn]
            fn poll_matches(id: u64) -> ::anyrun_plugin::anyrun_interface::PollResult {
                match THREAD.try_lock() {
                    Ok(thread) => match thread.as_ref() {
                        Some((thread, task_id)) => {
                            if *task_id == id {
                                if !thread.is_finished() {
                                    return ::anyrun_plugin::anyrun_interface::PollResult::Pending;
                                }
                            } else {
                                return ::anyrun_plugin::anyrun_interface::PollResult::Cancelled;
                            }
                        }
                        None => return ::anyrun_plugin::anyrun_interface::PollResult::Cancelled,
                    },
                    Err(_) => return ::anyrun_plugin::anyrun_interface::PollResult::Pending,
                }

                let (thread, _) = THREAD.lock().unwrap().take().unwrap();
                ::anyrun_plugin::anyrun_interface::PollResult::Ready(thread.join().unwrap())
            }

            #[::abi_stable::sabi_extern_fn]
            fn handle_selection(
                selection: ::anyrun_plugin::anyrun_interface::Match,
            ) -> ::anyrun_plugin::anyrun_interface::HandleResult {
                super::$handler(selection)
            }
        }
    };
}
