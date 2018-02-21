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
extern crate ctrlc;

extern crate leveldb;

extern crate uuid;

mod store;
mod query;
mod admin;

use rocket::Rocket;
use rocket::response::NamedFile;
use store::Store;
use std::sync::*;

lazy_static! {
    pub static ref STORE: RwLock<Store> = RwLock::new(Store::new());
}

#[get("/<path..>", rank=5)]
fn serve_static(path: std::path::PathBuf) -> Option<NamedFile> {
    NamedFile::open(std::path::Path::new("static/").join(path)).ok()
}

#[get("/", rank=5)]
fn serve_index() -> Option<NamedFile> {
    NamedFile::open(std::path::Path::new("static/index.html")).ok()
}

fn boot_web() {
    std::thread::spawn(|| {
        Rocket::ignite()
            .mount("/query", query::routes())
            .mount("/", routes![serve_static, serve_index])
            .manage(&STORE)
            .launch();
    });
}

fn boot_ws() {
    std::thread::spawn(|| {
        ws::listen("0.0.0.0:38265", |sender| admin::Handler::new(sender, &STORE)).unwrap();
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
