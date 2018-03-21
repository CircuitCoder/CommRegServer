extern crate crs;
extern crate serde;
extern crate csv;

use std::io;
use crs::store;
use csv::Reader;

fn main() {
    let mut store = store::Store::new();
    let mut rdr = Reader::from_reader(io::stdin());
    let mut curid = store.highest_id();
    for result in rdr.deserialize() {
        let raw: store::RawEntry = result.unwrap();
        curid += 1;
        let entry: store::Entry = raw.extend(curid);
        println!("Inserting: {:?}", entry);
        store.put(entry, false).unwrap();
    }
}
