// Re-export store & config
#![feature(nll)]

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate leveldb;
extern crate uuid;
extern crate ring;
extern crate byteorder;
extern crate jieba;

#[macro_use]
extern crate lazy_static;

pub mod config;
pub mod store;
