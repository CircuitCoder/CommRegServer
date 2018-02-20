use ws;
use ws::{Sender, Handshake, Message};
use store::{Store, Entry};
use std::sync::*;
use serde_json;
use serde_json::Value;
use std::str::Split;

pub struct Handler {
    sender: Sender,
    store: &'static RwLock<Store>,
}

impl Handler {
    pub fn new(sender: Sender, store: &'static RwLock<Store>) -> Handler {
        Handler { sender, store }
    }

    fn list(&self) -> ws::Result<()> {
        let s = match serde_json::to_string(&self.store.read().unwrap().filter::<Split<&str>>(None, None)) {
            Err(e) => return Err(ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), "Serialization failed")),
            Ok(d) => d,
        };
        self.sender.send(s)?;
        Ok(())
    }

    fn put(&self, payload: Value) -> ws::Result<()> {
        let payload: Entry = match serde_json::from_value(payload) {
            Err(e) => return Err(ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), "Deserialization failed")),
            Ok(d) => d,
        };
        self.store.write().unwrap().put(payload);
        self.sender.send("{\"ok\":1}")?;
        Ok(())
    }
}

impl ws::Handler for Handler {
    fn on_open(&mut self, shake: Handshake) -> ws::Result<()> {
        if shake.request.resource() == "/secret" {
            self.sender.send("{\"ok\":1}")?;
            return Ok(())
        }
        self.sender.send("{\"ok\":0}")?;
        self.sender.close(ws::CloseCode::Normal)?;
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> ws::Result<()> {
        let data: Value = match serde_json::from_slice(&msg.into_data()) {
            Err(e) => return Err(ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), "Deserialization failed")),
            Ok(d) => d,
        };
        if data["cmd"] == "list" {
            self.list()
        } else if data["cmd"] == "put" {
            // TODO: Can we remove this clone?
            self.put(data["payload"].clone())
        } else {
            Ok(())
        }
    }
}
