use proc_macro::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, Ident, ReturnType, Type};

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
            ::core::option::Option::Some(syn::FnArg::Typed(pat)) => match &*pat.ty {
                Type::Reference(reference) => {
                    reference.mutability.is_some()
                }
                _ => return quote! { compile_error!("Last argument must be either a reference to the shared data or should not be present at all.") }.into(),
            },
            ::core::option::Option::Some(_) => return quote! { compile_error!("`self` argument, really?") }.into(),
            ::core::option::Option::None => unreachable!(),
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
            ::core::option::Option::Some(syn::FnArg::Typed(pat)) => match &*pat.ty {
                Type::Reference(reference) => {
                    reference.mutability.is_some()
                }
                _ => return quote! { compile_error!("Last argument must be either a reference to the shared data or should not be present at all.") }.into(),
            },
            ::core::option::Option::Some(_) => return quote! { compile_error!("`self` argument, really?") }.into(),
            ::core::option::Option::None => unreachable!(),
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
            match ::std::panic::catch_unwind(|| {
                if let ::core::option::Option::Some(data) = #data {
                    #fn_name(input, data)
                } else {
                    ::abi_stable::std_types::RVec::new()
                }
            }
        ) {
                ::core::result::Result::Ok(result) => result,
                ::core::result::Result::Err(_) => {
                    ::std::eprintln!("Plugin '{}' panicked", anyrun_internal_info().name);
                    ::abi_stable::std_types::RVec::new()
                }
            }
        }
    } else {
        quote! {
            match ::std::panic::catch_unwind(|| {
                #fn_name(input)
            }) {
                ::core::result::Result::Ok(result) => result,
                ::core::result::Result::Err(_) => {
                    ::std::eprintln!("Plugin '{}' panicked", anyrun_internal_info().name);
                    ::abi_stable::std_types::RVec::new()
                }
            }
        }
    };

    quote! {
        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_get_matches(input: ::abi_stable::std_types::RString) -> ::abi_stable::std_types::RVec<::anyrun_plugin::anyrun_interface::Match> {
            #function

            #fn_call
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
        static ANYRUN_INTERNAL_DATA: ::std::sync::RwLock<Option<#data_type>> =
            ::std::sync::RwLock::new(None);

        #[::abi_stable::export_root_module]
        fn anyrun_internal_init_root_module() -> ::anyrun_plugin::anyrun_interface::PluginRef {
            use ::abi_stable::prefix_type::PrefixTypeTrait;
            ::anyrun_plugin::anyrun_interface::Plugin {
                init: anyrun_internal_init,
                info: anyrun_internal_info,
                get_matches: anyrun_internal_get_matches,
                handle_selection: anyrun_internal_handle_selection,
            }
            .leak_into_prefix()
        }

        #[::abi_stable::sabi_extern_fn]
        fn anyrun_internal_init(config_dir: ::abi_stable::std_types::RString) {
            #function

            ::std::thread::spawn(|| {
                let mut lock = ANYRUN_INTERNAL_DATA.write().unwrap();
                *lock = ::core::option::Option::Some(#fn_name(config_dir));
            });
        }
    }
    .into()
}

#[proc_macro_derive(ConfigArgs, attributes(config_args))]
pub fn config_args(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemStruct);
    let ident = &item.ident;

    let opt_ident = Ident::new(&format!("{}Args", item.ident), Span::call_site().into());

    let mut operations = quote!();
    let mut fields = quote!();
    for field in item.fields.iter() {
        let mut skip = false;
        for attr in &field.attrs {
            if attr.path().is_ident("config_args") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("skip") {
                        skip = true;
                        return Ok(());
                    }

                    Err(meta.error("Unrecognized macro input"))
                })
                .unwrap();
            }
        }
        if skip {
            continue;
        }
        let ty = &field.ty;
        let ident = field.ident.as_ref().unwrap();

        operations = quote! {
            #operations
            if let ::core::option::Option::Some(val) = opt.#ident {
                self.#ident = val;
            }
        };

        fields = quote! {
            #fields
            #[arg(long)]
            #ident: Option<#ty>,
        };
    }

    quote! {
        #[derive(::clap::Args)]
        struct #opt_ident {
            #fields
        }

        impl #ident {
            fn merge_opt(&mut self, opt: #opt_ident) {
                #operations
            }
        }
    }
    .into()
}
