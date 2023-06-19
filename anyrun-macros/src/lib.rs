use proc_macro::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, parse_quote, Attribute, Ident, ReturnType, Type};

/// The function to handle the selection of an item. Takes a `Match` as its first argument, and the second argument can be one of:
/// - &T
/// - &mut T
/// - <Nothing>
/// where T is the type returned by `init`.
///
/// Should return a `HandleResult` with the appropriate action.
#[proc_macro_attribute]
pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as syn::ItemFn);
    let fn_name = &function.sig.ident;

    let data = if function.sig.inputs.len() == 2 {
        if match function.sig.inputs.last() {
            Some(syn::FnArg::Typed(pat)) => match &*pat.ty {
                Type::Reference(reference) => {
                    reference.mutability.is_some()
                }
                _ => return quote! { compile_error!("Last argument must be either a reference to the shared data or should not be present at all.") }.into(),
            },
            Some(_) => return quote! { compile_error!("`self` argument, really?") }.into(),
            None => unreachable!(),
        } {
            quote! {
                ANYRUN_INTERNAL_DATA.write().unwrap().as_mut().unwrap(),
            }
        } else {
            quote! {
                ANYRUN_INTERNAL_DATA.read().unwrap().as_ref().unwrap(),
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_handle_selection(
            selection: ::anyrun_plugin::anyrun_interface::Match,
        ) -> ::anyrun_plugin::anyrun_interface::HandleResult {
            #function

            #fn_name(
                selection,
                #data
            )
        }
    }
    .into()
}

/// Function that takes the current text input as an `RString` as the first argument, and the second argument can be one of:
/// - &T
/// - &mut T
/// - <Nothing>
/// where T is the type returned by `init`.
///
/// It should return an `RVec` of `Match`es.
#[proc_macro_attribute]
pub fn get_matches(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as syn::ItemFn);
    let fn_name = &function.sig.ident;

    let fn_call = if function.sig.inputs.len() == 2 {
        let data = if match function.sig.inputs.last() {
            Some(syn::FnArg::Typed(pat)) => match &*pat.ty {
                Type::Reference(reference) => {
                    reference.mutability.is_some()
                }
                _ => return quote! { compile_error!("Last argument must be either a reference to the shared data or should not be present at all.") }.into(),
            },
            Some(_) => return quote! { compile_error!("`self` argument, really?") }.into(),
            None => unreachable!(),
        } {
            quote! {
                ANYRUN_INTERNAL_DATA.write().unwrap().as_mut()
            }
        } else {
            quote! {
                ANYRUN_INTERNAL_DATA.read().unwrap().as_ref()
            }
        };
        quote! {
            if let Some(data) = #data {
                #fn_name(input, data)
            } else {
                ::abi_stable::std_types::RVec::new()
            }
        }
    } else {
        quote! {
            #fn_name(input)
        }
    };

    quote! {
        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_get_matches(input: ::abi_stable::std_types::RString) -> u64 {
            #function

            let current_id =
                ANYRUN_INTERNAL_ID_COUNTER.load(::std::sync::atomic::Ordering::Relaxed);
            ANYRUN_INTERNAL_ID_COUNTER
                .store(current_id + 1, ::std::sync::atomic::Ordering::Relaxed);

            let handle = ::std::thread::spawn(move || {
                #fn_call
            });

            *ANYRUN_INTERNAL_THREAD.lock().unwrap() = Some((handle, current_id));

            current_id
        }
    }
    .into()
}

/// Function that returns the plugin info as a `PluginInfo` object. Takes no arguments.
#[proc_macro_attribute]
pub fn info(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as syn::ItemFn);
    let fn_name = &function.sig.ident;

    quote! {
        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_info() -> ::anyrun_plugin::anyrun_interface::PluginInfo {
            #function

            #fn_name()
        }
    }
    .into()
}

/// Function that takes an `RString` as the only argument, which points to the anyrun config directory. Returns the data
/// the plugin operates on. This data is accessible as both a normal borrow and a mutable borrow to `get_matches` and `handler`.
#[proc_macro_attribute]
pub fn init(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as syn::ItemFn);
    let fn_name = &function.sig.ident;
    let data_type = match &function.sig.output {
        ReturnType::Default => quote! {()},
        ReturnType::Type(_, data_type) => quote! {#data_type},
    };

    quote! {
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
        static ANYRUN_INTERNAL_DATA: ::std::sync::RwLock<Option<#data_type>> =
            ::std::sync::RwLock::new(None);

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
        fn anyrun_internal_init(config_dir: ::abi_stable::std_types::RString) {
            #function

            ::std::thread::spawn(|| {
                let mut lock = ANYRUN_INTERNAL_DATA.write().unwrap();
                *lock = Some(#fn_name(config_dir));
            });
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn config_args(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemStruct);
    let ident = &item.ident;

    let mut opt_item = item.clone();

    opt_item.attrs = vec![parse_quote!(#[derive(::clap::Args)])];
    opt_item.ident = Ident::new(&format!("{}Args", opt_item.ident), Span::call_site().into());

    let opt_ident = &opt_item.ident;

    let mut operations = quote!();

    for field in opt_item.fields.iter_mut() {
        let ty = &field.ty;
        let ident = &field.ident;
        field.ty = Type::Verbatim(quote!(Option<#ty>));
        field.attrs = vec![parse_quote!(#[arg(long)])];

        operations = quote! {
            #operations
            if let Some(val) = opt.#ident {
                self.#ident = val;
            }
        }
    }

    quote! {
        #item

        #opt_item

        impl #ident {
            fn merge_opt(&mut self, opt: #opt_ident) {
                #operations
            }
        }
    }
    .into()
}
