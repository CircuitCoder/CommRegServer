use rocket::*;
use rocket::http::*;
use rocket::request::*;
use rocket_contrib::Json;
use store::Store;
use store::*;
use store::Availability::*;
use std::sync::*;

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

#[get("/hello")]
fn hello() -> &'static str {
    "Hello!"
}

#[get("/<avail>/<search>")]
fn available(store: State<&RwLock<Store>>, avail: Availability, search: String) -> Json<Vec<Entry>> {
    Json(store.read().unwrap().filter(Some(avail), Some(search.split('+'))))
}

pub fn routes() -> Vec<Route> {
    routes![hello, available]
}
