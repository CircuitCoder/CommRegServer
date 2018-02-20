use ws;
use ws::{Sender, Handshake, Message, Frame};
use store::{Store, Entry};
use std::sync::*;
use serde_json;
use serde_json::Value;
use std::str::Split;
use uuid::Uuid;
use std::fs::*;
use std::path::Path;
use std::io::Write;

pub struct Handler {
    sender: Sender,
    store: &'static RwLock<Store>,
    uploading: Option<File>,
}

impl Handler {
    pub fn new(sender: Sender, store: &'static RwLock<Store>) -> Handler {
        Handler { sender, store, uploading: None }
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

    fn del(&self, target: Value) -> ws::Result<()> {
        if let Value::Number(n) = target {
            if let Some(id) = n.as_i64() {
                if let Ok(_) = self.store.write().unwrap().del(id as i32) {
                    self.sender.send("{\"ok\":1}")?;
                    return Ok(())
                }
            }
        }
        self.sender.send("{\"ok\":0}")?;
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
        } else if data["cmd"] == "del" {
            // TODO: Can we remove this clone?
            self.del(data["target"].clone())
        } else if data["cmd"] == "upload" {
            // Generate uuid
            let uuid = Uuid::new_v4();
            let basename = uuid.hyphenated();
            let ext = match data["ext"] {
                Value::String(ref s) => s,
                _ => {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                },
            };
            let path = Path::new("static/store/").join(format!("{}.{}", basename, ext));
            let opening = OpenOptions::new().write(true).create(true).open(path);
            self.uploading = match opening {
                Ok(f) => Some(f),
                Err(e) => return Err(ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), "Cannot create file")),
            };
            self.sender.send("{\"ok\":1}")?;
            Ok(())
        } else {
            Ok(())
        }
    }

    fn on_frame(&mut self, frame: Frame) -> ws::Result<Option<Frame>> {
        if frame.is_control() {
            return Ok(Some(frame));
        };

        let file = match self.uploading {
            None => return Ok(Some(frame)),
            Some(ref mut f) => f,
        };

        match file.write_all(&frame.payload()) {
            Err(e) => Err(ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), "Writing failed")),
            Ok(_) => {
                if frame.is_final() {
                    self.uploading = None; // Drop the file to close it
                    self.sender.send("{\"ok\":1}")?;
                }
                Ok(None)
            },
        }
    }
}
