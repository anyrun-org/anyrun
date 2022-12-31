pub use anyrun_interface::{self, Match, PluginInfo};

/**
The macro to create a plugin, handles asynchronous execution of getting the matches and the boilerplate
for integrating with `stable_abi`.

# Arguments

* `$init`: Function that takes an `RString` as the only argument, which points to the anyrun config directory. Returns the data
the plugin operates on.

* `$info`: Function that returns the plugin info as a `PluginInfo` object. Takes no arguments.

* `$get_matches`: Function that takes the current text input as an `RString` as the only argument, and returns an `RVec<Match>`.
This is run asynchronously automatically.

* `$handler`: The function to handle the selection of an item. Takes a `Match` as it's only argument and returns a `HandleResult` with
the appropriate action.

* `$type`: The type of the shared data to be provided to various functions.
**/
#[macro_export]
macro_rules! plugin {
    ($init:ident, $info:ident, $get_matches:ident, $handler:ident, $type:ty) => {
        static ANYRUN_INTERNAL_THREAD: ::std::sync::Mutex<
            Option<(
                ::std::thread::JoinHandle<
                    ::abi_stable::std_types::RVec<::anyrun_plugin::anyrun_interface::Match>,
                >,
                u64,
            )>,
        > = ::std::sync::Mutex::new(None);
        static ANYRUN_INTERNAL_ID_COUNTER: ::std::sync::atomic::AtomicU64 =
            ::std::sync::atomic::AtomicU64::new(0);
        static ANYRUN_INTERNAL_DATA: ::std::sync::Mutex<Option<$type>> =
            ::std::sync::Mutex::new(None);

        #[::abi_stable::export_root_module]
        fn anyrun_internal_init_root_module() -> ::anyrun_plugin::anyrun_interface::PluginRef {
            use ::abi_stable::prefix_type::PrefixTypeTrait;
            ::anyrun_plugin::anyrun_interface::Plugin {
                init: anyrun_internal_init,
                info: anyrun_internal_info,
                get_matches: anyrun_internal_get_matches,
                poll_matches: anyrun_internal_poll_matches,
                handle_selection: anyrun_internal_handle_selection,
            }
            .leak_into_prefix()
        }

        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_init(config_dir: ::abi_stable::std_types::RString) {
            ::std::thread::spawn(|| {
                *ANYRUN_INTERNAL_DATA.lock().unwrap() = Some($init(config_dir));
            });
        }

        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_info() -> ::anyrun_plugin::anyrun_interface::PluginInfo {
            $info()
        }

        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_get_matches(input: ::abi_stable::std_types::RString) -> u64 {
            let current_id =
                ANYRUN_INTERNAL_ID_COUNTER.load(::std::sync::atomic::Ordering::Relaxed);
            ANYRUN_INTERNAL_ID_COUNTER
                .store(current_id + 1, ::std::sync::atomic::Ordering::Relaxed);

            let handle = ::std::thread::spawn(move || {
                if let Some(data) = ANYRUN_INTERNAL_DATA.lock().unwrap().as_mut() {
                    $get_matches(input, data)
                } else {
                    ::abi_stable::std_types::RVec::new()
                }
            });

            *ANYRUN_INTERNAL_THREAD.lock().unwrap() = Some((handle, current_id));

            current_id
        }

        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_poll_matches(id: u64) -> ::anyrun_plugin::anyrun_interface::PollResult {
            match ANYRUN_INTERNAL_THREAD.try_lock() {
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

            let (thread, _) = ANYRUN_INTERNAL_THREAD.lock().unwrap().take().unwrap();
            ::anyrun_plugin::anyrun_interface::PollResult::Ready(thread.join().unwrap())
        }

        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_handle_selection(
            selection: ::anyrun_plugin::anyrun_interface::Match,
        ) -> ::anyrun_plugin::anyrun_interface::HandleResult {
            $handler(
                selection,
                ANYRUN_INTERNAL_DATA.lock().unwrap().as_mut().unwrap(),
            )
        }
    };
}
