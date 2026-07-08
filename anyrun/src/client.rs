use crate::app;
use crate::args::Args;
use crate::dbus::setup_dbus;
use crate::dbus::DaemonState;
use gtk4::{gio, glib, prelude::*};
use relm4::ComponentController;
use std::cell::RefCell;
use std::io::{self, IsTerminal, Read, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

pub(crate) fn determine_app_flags(static_load: bool) -> gio::ApplicationFlags {
    if static_load {
        gio::ApplicationFlags::NON_UNIQUE
    } else {
        gio::ApplicationFlags::FLAGS_NONE
    }
}

pub fn run_client(args: Args) {
    let time = Instant::now();
    let duration = time.elapsed();
    println!("[Start Application at {:?}]", duration);

    let static_load = args.config.has_plugins();

    let read_init_data = |args: Args| {
        let mut stdin = Vec::new();
        if !io::stdin().is_terminal() {
            io::stdin()
                .lock()
                .take(2 * 1024 * 1024)
                .read_to_end(&mut stdin)
                .ok();
        }
        let env: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| {
                k == "HOME"
                    || k.starts_with("XDG_")
                    || k == "PATH"
                    || k == "DISPLAY"
                    || k == "WAYLAND_DISPLAY"
                    || k.starts_with("ANYRUN_")
                    || k == "LANG"
                    || k == "TERM"
            })
            .collect();
        app::AppInit { args, stdin, env }
    };

    if !static_load {
        if let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) {
            let params = ("org.anyrun.anyrun",).to_variant();
            let has_owner = conn
                .call_sync(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "GetNameOwner",
                    Some(&params),
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    1000,
                    None::<&gio::Cancellable>,
                )
                .is_ok();

            if has_owner {
                let payload = read_init_data(args.clone());
                let serialized = serde_json::to_vec(&payload).unwrap();
                let bytes = glib::Bytes::from_owned(serialized);
                let msg = glib::Variant::from_bytes::<(Vec<u8>,)>(&bytes);

                let dbus_connect_duration = time.elapsed();
                println!(
                    "[D-Bus connection established at {:?}]",
                    dbus_connect_duration
                );

                match conn.call_sync(
                    Some("org.anyrun.anyrun"),
                    "/org/anyrun/anyrun",
                    "org.anyrun.Anyrun",
                    "Show",
                    Some(&msg),
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    -1,
                    None::<&gio::Cancellable>,
                ) {
                    Ok(val) => {
                        let call_duration = time.elapsed();
                        println!("[D-Bus call returned at {:?}]", call_duration);
                        if let Some(b) = val.child_value(0).get::<Vec<u8>>() {
                            if let Ok(app::PostRunAction::Stdout(out_data)) =
                                serde_json::from_slice::<app::PostRunAction>(&b)
                            {
                                let mut out = io::stdout().lock();
                                let _ = out.write_all(&out_data);
                                let _ = out.flush();
                            }
                        }
                        let duration = time.elapsed();
                        println!("[Client finished via sync IPC at {:?}]", duration);
                        return;
                    }
                    Err(err) => {
                        eprintln!("Daemon communication failed: {err}");
                        return;
                    }
                }
            }
        }
    }

    let app = gtk4::Application::new(Some("org.anyrun.anyrun"), determine_app_flags(static_load));
    if let Err(e) = app.register(None::<&gio::Cancellable>) {
        eprintln!("Registration error: {e}");
        return;
    }

    let duration = time.elapsed();
    println!("[Call dbus at {:?}]", duration);

    if !static_load && app.is_remote() {
        let conn = app.dbus_connection().expect("No D-Bus connection");
        let payload = read_init_data(args);

        let serialized = serde_json::to_vec(&payload).unwrap();
        let bytes = glib::Bytes::from_owned(serialized);
        let msg = glib::Variant::from_bytes::<(Vec<u8>,)>(&bytes);

        let main_loop = glib::MainLoop::new(None, false);
        let loop_clone = main_loop.clone();

        conn.call(
            Some("org.anyrun.anyrun"),
            "/org/anyrun/anyrun",
            "org.anyrun.Anyrun",
            "Show",
            Some(&msg),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            None::<&gio::Cancellable>,
            move |res| {
                if let Ok(val) = res {
                    if let Some(b) = val.child_value(0).get::<Vec<u8>>() {
                        if let Ok(app::PostRunAction::Stdout(out_data)) =
                            serde_json::from_slice::<app::PostRunAction>(&b)
                        {
                            let mut out = io::stdout().lock();
                            let _ = out.write_all(&out_data);
                            let _ = out.flush();
                        }
                    }
                }
                loop_clone.quit();
            },
        );
        let duration = time.elapsed();
        println!("[Check at {:?}]", duration);

        main_loop.run();
    } else {
        let shared_init = Arc::new(read_init_data(args));

        app.connect_activate(move |app| {
            let controller = app::App::launch(app, (*shared_init).clone(), None, None);
            let state = Rc::new(RefCell::new(DaemonState {
                sender: controller.sender().clone(),
                provider_child: None,
            }));
            setup_dbus(app, state);
            controller.sender().emit(app::AppMsg::Activate(None));
        });
        app.run_with_args(&Vec::<String>::new());
    }
}

#[cfg(test)]
mod tests {
    use super::determine_app_flags;
    use gtk4::gio;

    #[test]
    fn static_load_flag() {
        assert_eq!(determine_app_flags(true), gio::ApplicationFlags::NON_UNIQUE,);
    }

    #[test]
    fn normal_flag() {
        assert_eq!(
            determine_app_flags(false),
            gio::ApplicationFlags::FLAGS_NONE,
        );
    }
}
