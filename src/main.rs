#![feature(plugin)]
#![plugin(rocket_codegen)]
#![cfg_attr(feature="clippy", plugin(clippy))]

#![feature(integer_atomics)]
#![feature(option_filter)]
#![feature(nll)]
#![feature(conservative_impl_trait)]
#![feature(catch_expr)]

#![allow(print_literal)]
#![allow(needless_pass_by_value)]

extern crate ws;
extern crate rocket;
extern crate rocket_contrib;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;
extern crate serde;

#[macro_use]
extern crate serde_json;
extern crate serde_yaml;
extern crate ctrlc;

extern crate leveldb;

extern crate uuid;
extern crate ring;
extern crate byteorder;

extern crate jieba;

mod store;
mod query;
mod admin;
mod config;

use rocket::Rocket;
use rocket::response::NamedFile;
use rocket_contrib::Json;
use rocket::config::Environment;
use store::Store;
use std::sync::*;
use config::Config;
use ws::WebSocket;

const PING_INTERVAL: u64 = 1; // s

lazy_static! {
    pub static ref STORE: RwLock<Store> = RwLock::new(Store::new());
    pub static ref CONFIG: Config = Config::load();
    pub static ref PING_PAYLOAD: Vec<u8> = vec![97];
}

#[get("/<path..>", rank=5)]
fn serve_static(path: std::path::PathBuf) -> Option<NamedFile> {
    NamedFile::open(std::path::Path::new("static/").join(path)).ok()
}

#[get("/", rank=5)]
fn serve_index() -> Option<NamedFile> {
    NamedFile::open(std::path::Path::new("static/index.html")).ok()
}

#[get("/config")]
fn serve_config() -> Json<Config> {
    Json(CONFIG.clone())
}

fn boot_web() {
    let config = rocket::Config::build(Environment::Staging)
        .address(CONFIG.web.host.clone())
        .port(CONFIG.web.port)
        .unwrap();

    std::thread::spawn(move || {
        Rocket::custom(config, true)
            .mount("/query", query::routes())
            .mount("/", routes![serve_static, serve_index, serve_config])
            .manage(&*STORE)
            .launch();
    });
}

fn boot_ws() {
    std::thread::spawn(|| {
        let server = WebSocket::new(|sender|
                                    admin::Handler::new(sender, &STORE, &CONFIG)).unwrap();

        let broadcaster = server.broadcaster();
        // Pinging
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(PING_INTERVAL));
                let _result = broadcaster.ping(PING_PAYLOAD.clone());
                // Silently ignores
            }
        });

        server.listen(format!("{}:{}", CONFIG.ws.host, CONFIG.ws.port)).unwrap();
    });
}

fn main() {
    let panic_handler = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        panic_handler(info);
        std::process::exit(1);
    }));

    let (tx, rx) = std::sync::mpsc::channel();

    boot_ws();
    boot_web();

    ctrlc::set_handler(move || {
        tx.send(0).unwrap();
    }).unwrap();

    rx.recv().unwrap(); // Wait for Ctrl-C
    STORE.write().expect("Previous writing failed.").close();
    std::process::exit(0);
}
