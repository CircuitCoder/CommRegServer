#![feature(plugin)]
#![cfg_attr(feature="clippy", plugin(clippy))]

extern crate crs;
extern crate csv;
extern crate serde;

use crs::store;
use crs::config;
use crs::key;

use std::io;
use std::str::Split;

fn main() {
    let store = store::Store::new();
    let config = config::Config::load();
    let entries = store.filter::<Split<&str>>(None, None);

    let mut writer = csv::Writer::from_writer(io::stdout());
    for e in &entries {
        writer.serialize(
            (
                e.id(),
                e.name(),
                e.name_eng(),
                key::generate_key(e.id(), config.secret.as_bytes()).unwrap()
            )
        ).unwrap();
    };
}
