use std::{
    cell::RefCell,
    io::{self, IsTerminal, Read, Write},
    rc::Rc,
};

use clap::{Parser, Subcommand};
use gtk::{glib, prelude::*};
use gtk4::{
    self as gtk,
    gio::{self},
};
use gtk4_layer_shell::LayerShell;
use relm4::Sender;
use serde::{Deserialize, Serialize};

use crate::config::{Config, ConfigArgs};

mod app;
mod config;
mod plugin_box;
mod provider;

/// The interface through which the daemon
/// responds to launch requests
const INTERFACE_XML: &str = r#"
<node>
    <interface name="org.anyrun.Anyrun">
        <method name="Show">
            <arg type="ay" name="args" direction="in"/>
            <arg type="ay" name="result" direction="out"/>
        </method>
        <method name="Close">
            <arg type="ay" name="result" direction="out"/>
        </method>
        <method name="Quit"></method>
    </interface>
</node> 
"#;

#[derive(Debug, glib::Variant)]
struct Show {
    args: Vec<u8>,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum CloseError {
    NotShowed,
}

enum InterfaceMethod {
    Show(Show),
    Close,
    Quit,
}

impl DBusMethodCall for InterfaceMethod {
    fn parse_call(
        _obj_path: &str,
        _interface: Option<&str>,
        method: &str,
        params: glib::Variant,
    ) -> Result<Self, glib::Error> {
        match method {
            "Show" => Ok(params.get::<Show>().map(Self::Show)),
            "Close" => Ok(Some(Self::Close)),
            "Quit" => Ok(Some(Self::Quit)),
            _ => Err(glib::Error::new(
                gio::DBusError::UnknownMethod,
                "No such method",
            )),
        }
        .and_then(|p| {
            p.ok_or_else(|| glib::Error::new(gio::DBusError::InvalidArgs, "Invalid parameters"))
        })
    }
}

/// A wayland native, highly customizable runner.
#[derive(Parser, Clone, Debug, Serialize, Deserialize)]
#[command(version, about)]
pub struct Args {
    /// Override the path to the config directory
    #[arg(short, long)]
    config_dir: Option<String>,
    #[command(flatten)]
    config: ConfigArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone, Copy, Deserialize, Serialize)]
enum Command {
    Daemon,
    Close,
    Quit,
}

/// Refcelled state for the daemon DBus listener
pub struct DaemonState {
    sender: Option<Sender<app::AppMsg>>,
}

fn main() {
    let args = Args::parse();
    let flags = if matches!(args.command, Some(Command::Daemon)) {
        gio::ApplicationFlags::IS_SERVICE
    } else {
        Default::default()
    };
    let app = gtk::Application::new(Some("org.anyrun.anyrun"), flags);
    app.register(Option::<&gio::Cancellable>::None).unwrap();

    let dbus_conn = app.dbus_connection().unwrap();

    let interface = gio::DBusNodeInfo::for_xml(INTERFACE_XML)
        .unwrap()
        .lookup_interface("org.anyrun.Anyrun")
        .unwrap();

    let proxy = gio::DBusProxy::new_sync(
        &dbus_conn,
        gio::DBusProxyFlags::empty(),
        Some(&interface),
        Some("org.anyrun.anyrun"),
        "/org/anyrun/anyrun",
        "org.anyrun.Anyrun",
        Option::<&gio::Cancellable>::None,
    )
    .unwrap();

    match args.command {
        None => {
            let stdin = if io::stdin().is_terminal() {
                Vec::new()
            } else {
                let mut buf = Vec::new();
                io::stdin().read_to_end(&mut buf).unwrap();
                buf
            };
            let env = std::env::vars().collect();

            if app.is_remote() {
                let res = proxy
                    .call_sync(
                        "Show",
                        Some(
                            &(serde_json::to_vec(&app::AppInit { args, stdin, env }).unwrap(),)
                                .to_variant(),
                        ),
                        gio::DBusCallFlags::NONE,
                        1_000_000_000, // Very long timeout to get results from the daemon
                        Option::<&gio::Cancellable>::None,
                    )
                    .unwrap();

                let (bytes,): (Vec<u8>,) = FromVariant::from_variant(&res).unwrap();

                let res =
                    serde_json::from_slice::<Result<app::PostRunAction, app::ShowError>>(&bytes)
                        .unwrap();
                match res {
                    Ok(app::PostRunAction::Stdout(stdout)) => {
                        io::stdout().lock().write_all(&stdout).unwrap()
                    }
                    Ok(app::PostRunAction::None) => (),
                    Err(app::ShowError::AlreadyShowed) => {
                        eprintln!("[anyrun] Anyrun is already visible.");
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("\x1B[1;33m[anyrun] Warning: started in standalone mode, clipboard functionality will be unavailable and startup speed is reduced. \
                    Consider starting the daemon alongside your compositor by making sure `anyrun daemon` is ran somewhere.\x1B[0m");

                app.connect_activate(move |app| {
                    app::App::launch(
                        app,
                        app::AppInit {
                            args: args.clone(),
                            stdin: stdin.clone(),
                            env: env.clone(),
                        },
                        None,
                    );
                });
            }
            app.run_with_args(&Vec::<String>::new());
        }
        Some(Command::Close) => {
            if !app.is_remote() {
                eprintln!("[anyrun] Can't close the launcher if no daemon exists");
                std::process::exit(1);
            }

            let res = proxy
                .call_sync(
                    "Close",
                    None,
                    gio::DBusCallFlags::NONE,
                    100,
                    Option::<&gio::Cancellable>::None,
                )
                .unwrap();

            let (bytes,): (Vec<u8>,) = FromVariant::from_variant(&res).unwrap();
            let res = serde_json::from_slice(&bytes).unwrap();
            match res {
                Ok(()) => {}
                Err(CloseError::NotShowed) => {
                    eprintln!("[anyrun] Anyrun isn't currently visible");
                    std::process::exit(1);
                }
            }

            app.run_with_args(&Vec::<String>::new());
        }
        Some(Command::Quit) => {
            if !app.is_remote() {
                eprintln!("[anyrun] Can't quit the daemon if it isn't running.");
                std::process::exit(1);
            }

            proxy
                .call_sync(
                    "Quit",
                    None,
                    gio::DBusCallFlags::NONE,
                    100,
                    Option::<&gio::Cancellable>::None,
                )
                .unwrap();
            app.run_with_args(&Vec::<String>::new());
        }
        Some(Command::Daemon) => {
            let _hold_guard = app.hold();

            let state = Rc::new(RefCell::new(DaemonState { sender: None }));

            // Create an empty window on launch and show it to make sure GPU resources are initialized
            // This should help with first launch speed
            let window = gtk::Window::new();
            window.init_layer_shell();
            window.set_visible(true);
            window.close();

            dbus_conn
                .register_object("/org/anyrun/anyrun", &interface)
                .typed_method_call::<InterfaceMethod>()
                .invoke(glib::clone!(
                    #[weak_allow_none]
                    app,
                    #[strong]
                    state,
                    move |_conn, _sender, method, invocation| {
                        let app = app.unwrap();
                        match method {
                            InterfaceMethod::Show(show) => {
                                // Only launch an instance if another one doesn't exist
                                if state.borrow().sender.is_none() {
                                    state.borrow_mut().sender = Some(app::App::launch(
                                        &app,
                                        serde_json::from_slice(&show.args).unwrap(),
                                        Some((state.clone(), invocation)),
                                    ));
                                } else {
                                    invocation.return_value(Some(
                                        &(serde_json::to_vec(&Err::<app::PostRunAction, _>(
                                            app::ShowError::AlreadyShowed,
                                        ))
                                        .unwrap(),)
                                            .to_variant(),
                                    ));
                                }
                            }
                            InterfaceMethod::Close => {
                                // If launcher is open, return an ok value. If launcher is closed, return an err to
                                // facilitate a non zero exit code
                                if let Some(sender) = state.borrow().sender.clone() {
                                    sender.emit(app::AppMsg::Action(config::Action::Close));
                                    invocation.return_value(Some(
                                        &(serde_json::to_vec(&Ok::<(), CloseError>(())).unwrap(),)
                                            .to_variant(),
                                    ));
                                } else {
                                    invocation.return_value(Some(
                                        &(serde_json::to_vec(&Err::<(), _>(CloseError::NotShowed))
                                            .unwrap(),)
                                            .to_variant(),
                                    ))
                                }
                            }
                            InterfaceMethod::Quit => {
                                invocation.return_value(None);
                                app.quit();
                            }
                        }
                    }
                ))
                .build()
                .unwrap();

            app.run_with_args(&Vec::<String>::new());
        }
    }
}
