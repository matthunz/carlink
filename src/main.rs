use car_api::{Client, Vehicle};
use dioxus::prelude::*;
use dioxus_desktop::{
    tao::{dpi::PhysicalPosition, window},
    use_window, Config, PhysicalSize, WindowBuilder,
};
use dioxus_router::prelude::*;
use dioxus_signals::use_signal;
use fermi::{use_init_atom_root, use_read, use_set, Atom};
use image::ImageFormat;
use log::LevelFilter;
use std::{
    io::{BufReader, Cursor, IoSlice, Read},
    rc::Rc,
    thread,
};
use tokio::sync::mpsc;
use tray_icon::{
    menu::{Menu, MenuEvent},
    Icon, TrayIconBuilder, TrayIconEvent,
};

const _: &str = manganis::font!({ families: ["Roboto"] });

fn load_icon() -> tray_icon::Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let bytes = include_bytes!("../assets/icon.png");
        let reader = BufReader::new(Cursor::new(&bytes[..]));
        let image = image::load(reader, ImageFormat::Png)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}

#[tokio::main]
async fn main() {
    // Init debug
    dioxus_logger::init(LevelFilter::Info).expect("failed to init logger");
    console_error_panic_hook::set_once();

    log::info!("starting app");

    let config = Config::new().with_window(
        WindowBuilder::new()
            .with_resizable(false)
            .with_inner_size(PhysicalSize::new(400., 400.))
            .with_decorations(false).with_visible(false),
    );
    dioxus_desktop::launch_cfg(app, config);
}

fn app(cx: Scope) -> Element {
    use_init_atom_root(cx);

    let window = use_window(cx);

    let channel = use_signal(cx, || {
        let (tx, rx) = mpsc::unbounded_channel::<(f64, f64)>();
        (tx, RefCell::new(Some(rx)))
    });
    let (tx, rx) = &*channel();

    to_owned![window];
    use_future(cx, (), move |_| {
        let mut rx = rx.borrow_mut().take().unwrap();
        async move {
            while let Some((x, y)) = rx.recv().await {
                let size = window.outer_size();
                window.set_visible(!window.is_visible());
                window.set_outer_position(PhysicalPosition::new(x - size.width as f64 / 2., y));
            }
        }
    });

    to_owned![tx];
    let tray_icon = use_signal(cx, || {
        let menu = Menu::new();
        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("system-tray - tray icon library!")
            .with_menu(Box::new(menu))
            .with_icon(load_icon())
            .build()
            .unwrap();

        let menu_channel = MenuEvent::receiver();
        let tray_channel = TrayIconEvent::receiver();

        thread::spawn(move || loop {
            if let Ok(event) = menu_channel.try_recv() {
                println!("{event:?}");
            }

            if let Ok(event) = tray_channel.try_recv() {
                tx.send((
                    event.icon_rect.left + (event.icon_rect.right - event.icon_rect.left) / 2.,
                    event.icon_rect.bottom,
                ))
                .unwrap();
                println!("{event:?}");
            }
        });

        tray_icon
    });

    render! { Router::<Route> {} }
}

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[layout(Layout)]
    #[route("/")]
    Home,
    #[route("/login")]
    Login,
    #[route("/vehicles")]
    Vehicles,
    #[route("/vehicles/:id")]
    Vehicle { id: String },
}

#[component]
fn Layout(cx: Scope) -> Element {
    cx.render(rsx! { div { position: "fixed", top: 0, left: 0, width: "100vw", height: "100vh", font: "16px Roboto", color: "#fff", background: "#000",  Outlet::<Route> {} } })
}

#[component]
fn Home(cx: Scope) -> Element {
    let navigator = use_navigator(cx);
    let session_id = use_read(cx, &SESSION_ID).clone();

    if session_id.is_none() {
        navigator.push(Route::Login);
    } else {
        navigator.push(Route::Vehicles);
    }

    cx.render(rsx! { "Loading..."})
}

#[component]
fn Login(cx: Scope) -> Element {
    let client = use_read(cx, &CLIENT);
    let navigator = use_navigator(cx);

    let set_session_id = use_set(cx, &SESSION_ID).clone();

    cx.render(rsx! {
        form { onsubmit: move |event| {
                to_owned![client, navigator, set_session_id];
                async move {
                    let session_id = client
                        .login(&event.values["username"][0], &event.values["password"][0])
                        .await;
                    set_session_id(Some(session_id));
                    navigator.push(Route::Vehicles);
                }
            },
            input { r#type: "text", name: "username" }
            input { r#type: "password", name: "password" }
            input { r#type: "submit" }
        }
    })
}

static CLIENT: Atom<Rc<Client>> = Atom(|_| Rc::new(Client::us()));

static SESSION_ID: Atom<Option<String>> = Atom(|_| None);

static VEHICLES: Atom<Option<Vec<Vehicle>>> = Atom(|_| None);

#[component]
fn Vehicles(cx: Scope) -> Element {
    let client = use_read(cx, &CLIENT).clone();
    let session_id = use_read(cx, &SESSION_ID).clone();

    let vehicles = use_read(cx, &VEHICLES);
    let set_vehicles = use_set(cx, &VEHICLES).clone();

    use_effect(cx, &session_id, move |session_id| async move {
        if let Some(session_id) = session_id {
            let new_vehicles = client.vehicles(&session_id).await;
            set_vehicles(Some(new_vehicles));
        }
    });

    let vehicle_items = vehicles.as_ref().map(|vehicles| {
        let items = vehicles.iter().map(|vehicle| {
            cx.render(rsx! {
                li {
                    Link {
                        to: Route::Vehicle {
                            id: vehicle.vehicle_key.clone(),
                        },
                        "{vehicle.nick_name} - {vehicle.model_name} ({vehicle.trim})"
                    }
                }
            })
        });

        cx.render(rsx! {
            ul { items }
        })
    });

    cx.render(rsx! {
        h4 { "Vehicles" }
        vehicle_items
    })
}

#[component]
fn Vehicle(cx: Scope, id: String) -> Element {
    let client = use_read(cx, &CLIENT).clone();
    let session_id = use_read(cx, &SESSION_ID).clone();
    let vehicles = use_read(cx, &VEHICLES);

    let lock = use_signal(cx, || Some(true));

    if let Some(vehicles) = vehicles {
        let vehicle = vehicles
            .iter()
            .find(|vehicle| &vehicle.vehicle_key == id)
            .unwrap();

        let lock_button = if let Some(is_locked) = *lock() {
            cx.render(rsx! {
                button { onclick: move |_| {
                        lock.set(None);
                        let vehicle_id = vehicle.vehicle_key.clone();
                        to_owned![client, session_id];
                        async move {
                            let session_id = session_id.as_ref().unwrap();
                            if is_locked {
                                client.unlock(session_id, &vehicle_id).await;
                            } else {
                                client.lock(session_id, &vehicle_id).await;
                            }
                            lock.set(Some(!is_locked));
                        }
                    },
                    if is_locked { "Unlock" } else { "Lock" }
                }
            })
        } else {
            cx.render(rsx! {"Loading..."})
        };

        cx.render(rsx! {
            h4 { "{vehicle.nick_name}" }
            lock_button
        })
    } else {
        None
    }
}
