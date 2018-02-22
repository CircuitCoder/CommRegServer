#![feature(plugin)]
#![plugin(rocket_codegen)]
#![cfg_attr(feature="clippy", plugin(clippy))]

#![feature(integer_atomics)]
#![feature(option_filter)]
#![feature(nll)]
#![feature(conservative_impl_trait)]

extern crate ws;
extern crate rocket;
extern crate rocket_contrib;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate ctrlc;

extern crate leveldb;

extern crate uuid;

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

lazy_static! {
    pub static ref STORE: RwLock<Store> = RwLock::new(Store::new());
    pub static ref CONFIG: Config = Config::load();
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
        ws::listen(format!("{}:{}", CONFIG.ws.host, CONFIG.ws.port),
            |sender| admin::Handler::new(sender, &STORE, &CONFIG)).unwrap();
    });
}

fn main() {
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
