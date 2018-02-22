use rocket::*;
use rocket::http::*;
use rocket::request::*;
use rocket_contrib::Json;
use store::Store;
use store::*;
use store::Availability::*;
use std::sync::*;
use std::str::{Split, Utf8Error};

impl<'a> FromParam<'a> for Availability {
    type Error = &'a RawStr;
    fn from_param(param: &'a RawStr) -> Result<Availability, Self::Error> {
        if param == "available" {
            Ok(Available)
        } else if param == "disbanded" {
            Ok(Disbanded)
        } else {
            Err(param)
        }
    }
}

#[get("/<avail>/<search>", rank=2)]
fn list(store: State<&RwLock<Store>>, avail: Availability, search: &RawStr) -> Result<Json<Vec<Entry>>, Utf8Error> {
    Ok(Json(store.read()
        .unwrap()
        .filter(Some(avail), Some(search.url_decode()?.split(' ')))))
}

#[get("/<avail>", rank=2)]
fn list_all(store: State<&RwLock<Store>>, avail: Availability) -> Json<Vec<Entry>> {
    Json(store.read().unwrap().filter::<Split<&str>>(Some(avail), None))
}

#[get("/fetch/<id>")]
fn fetch(store: State<&RwLock<Store>>, id: i32) -> Option<Json<Entry>> {
    store.read().unwrap().fetch(id).map(Json)
}

pub fn routes() -> Vec<Route> {
    routes![list, list_all, fetch]
}
