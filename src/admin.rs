use config::Config;
use key;
use serde::Serializer;
use serde_json::Value;
use serde_json;
use ring::error::Unspecified;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::*;
use std::io::Write;
use std::path::Path;
use std::sync::*;
use std;
use store::{Store, Entry, PullEntry};
use uuid::Uuid;
use ws::{Sender, Handshake, Message, Frame, CloseCode};
use ws;
use ws::util::Token;

lazy_static! {
    static ref SENDERS: RwLock<HashMap<Token, Sender>> = RwLock::new(HashMap::new());
}

fn err_to_wserr<T, I: Into<Cow<'static, str>>>(e: T, reason: I) -> ws::Error
  where T: 'static + std::error::Error + Send + Sync {
    ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), reason)
}

pub struct Handler {
    sender: Sender,
    store: &'static RwLock<Store>,
    config: &'static Config,
    uploading: Option<File>,
    limited: Option<i32>,
}

impl Handler {
    pub fn new(sender: Sender, store: &'static RwLock<Store>, config: &'static Config) -> Handler {
        Handler { sender, store, config, uploading: None, limited: None, }
    }

    fn list(&self) -> ws::Result<()> {
        let result = match self.limited {
            None => self.store.read().unwrap().pull(),
            Some(id) => self.store.read().unwrap().pull_fetch(id)
                .map_or_else(Vec::new, |e| {
                    let mut r = Vec::with_capacity(1);
                    r.push(e);
                    r
                }),
        };
        let s = match serde_json::to_string(&result) {
            Err(e) => return Err(err_to_wserr(e, "Serialization failed")),
            Ok(d) => d,
        };
        self.sender.send(s)?;
        Ok(())
    }

    fn put(&self, payload: Value) -> ws::Result<()> {
        let payload: Entry = match serde_json::from_value(payload) {
            Err(e) => return Err(err_to_wserr(e, "Deserialization failed")),
            Ok(d) => d,
        };

        if self.limited.is_some()
            && Some(payload.id()) != self.limited {

            // Permission denied
            self.sender.send("{\"ok\":0}")?;
            return Ok(())
        }

        let mut s = self.store.write().unwrap();
        let id = payload.id();

        if let Err(e) = s.stash(payload, self.limited.is_some()) {
            Err(err_to_wserr(e, "Storage failure"))
        } else {
            let payload = match serde_json::to_value(s.pull_fetch(id)) {
                Err(e) => return Err(err_to_wserr(e, "Serialization Failed")),
                Ok(p) => p,
            };

            std::mem::drop(s);
            let content = json!({
                "cmd": "update",
                "id": id,
                "payload": payload,
            }).to_string();

            let pong = json!({
                "ok": 1,
                "payload": payload,
            }).to_string();

            for (k, v) in SENDERS.read().unwrap().iter() {
                if k != &self.sender.token() {
                    v.send(content.clone())?;
                }
            }

            self.sender.send(pong)
        }
    }

    fn del(&self, target: Value) -> ws::Result<()> {
        if self.limited.is_some() {
            self.sender.send("{\"ok\":0}")?; // Denied
            return Ok(())
        }

        if let Value::Number(n) = target {
            if let Some(id) = n.as_i64() {
                if self.store.write().unwrap().del(id as i32).is_ok() {
                    self.sender.send("{\"ok\":1}")?;
                    return Ok(())
                }
            }
        }
        self.sender.send("{\"ok\":0}")?;
        // TODO: syncdown for del
        Ok(())
    }

    fn files(&self, target: Value) -> ws::Result<()> {
        let iter = match read_dir(Path::new("./static/store")) {
            Err(e) => return Err(err_to_wserr(e, "Not privleged to read directory")),
            Ok(i) => i,
        };

        let mut entry = None;
        if let Value::Number(n) = target {
            if let Some(i) = n.as_i64() {
                entry = Some(i as i32)
            }
        }

        if self.limited.is_some() {
            if self.limited != entry {
                self.sender.send("{\"ok\":0}")?;
                return Ok(())
            }
        }

        let filtered = iter.filter_map(|e| {
            let e = match e {
                Err(_) => return None,
                Ok(e) => e,
            };

            let ft = e.file_type();
            if ft.is_err() || !ft.unwrap().is_file() {
                return None;
            }

            e.path()
                .file_name()
                .filter(|s| *s != ".gitkeep")
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .map(|filename| (e.metadata().unwrap().modified().unwrap(), filename))
        });

        let mut collected: Vec<_> = if let Some(id) = entry {
            // Filter uploads by prefixes
            let prefix = format!("{}.", id);
            filtered.filter(|&(_, ref f)| f.starts_with(&prefix)).collect()
        } else { filtered.collect() };

        collected.sort_unstable_by(|a,b| b.cmp(a));

        let iter = collected.iter().map(|&(_, ref f)| f);

        let mut writer = Vec::with_capacity(128);
        let mut ser = serde_json::ser::Serializer::new(&mut writer);
        let result = ser.collect_seq(iter);
        if let Err(e) = result {
            Err(err_to_wserr(e, "Serialization failed"))
        } else {
            // TODO: maybe there is arbitary files in the directory
            self.sender.send(String::from_utf8(writer).unwrap())?;
            Ok(())
        }
    }

    pub fn generate_key(&self, target: i32) -> Result<String, Unspecified> {
        key::generate_key(target, self.config.secret.as_bytes())
    }

    pub fn try_decrypt_key(&self, key: &str) -> Option<i32> {
        key::try_decrypt_key(key, self.config.secret.as_bytes())
    }
}

impl ws::Handler for Handler {
    fn on_close(&mut self, code: CloseCode, reason: &str) {
        // Closed
        SENDERS
            .write()
            .unwrap()
            .remove(&self.sender.token());
    }

    fn on_open(&mut self, shake: Handshake) -> ws::Result<()> {
        SENDERS
            .write()
            .unwrap()
            .insert(self.sender.token(), self.sender.clone());

        let res = shake.request.resource();
        if res == format!("/{}", self.config.secret) {
            // Is administrative account
            self.sender.send("{\"ok\":1}")?;
            return Ok(())
        } else if res.len() > 1 && res.starts_with('/') {
            if let Some(id) = self.try_decrypt_key(&res[1..]) {
                self.limited = Some(id);
                self.sender.send(format!("{{\"ok\":1,\"limited\":{}}}", id))?;
                return Ok(())
            }
        }
        self.sender.send("{\"ok\":0}")?;
        self.sender.close(ws::CloseCode::Normal)?;
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> ws::Result<()> {
        let data: Value = match serde_json::from_slice(&msg.into_data()) {
            Err(e) => return Err(err_to_wserr(e, "Deserialization failed")),
            Ok(d) => d,
        };
        if data["cmd"] == "list" {
            self.list()
        } else if data["cmd"] == "commit" || data["cmd"] == "discard" {
            if self.limited.is_some() {
                self.sender.send("{\"ok\":0}")?; // Denied
                return Ok(())
            }

            let id = match data["id"] {
                Value::Number(ref i) => match i.as_i64() {
                    Some(i) => i as i32,
                    None => {
                        self.sender.send("{\"ok\":0}")?;
                        return Ok(());
                    }
                },
                _ => {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                },
            };

            let mut s = self.store.write().unwrap();
            if data["cmd"] == "commit" {
                if let Err(e) = s.commit(id) {
                    return Err(err_to_wserr(e, "Storage failure"))
                }
            } else {
                s.discard(id);
            };

            let payload = match serde_json::to_value(s.pull_fetch(id)) {
                Err(e) => return Err(err_to_wserr(e, "Serialization Failed")),
                Ok(p) => p,
            };

            let content = json!({
                "cmd": "update",
                "id": id,
                "payload": payload,
            }).to_string();

            self.sender.broadcast(content);
            self.sender.send("{\"ok\":1}")
        } else if data["cmd"] == "len" {
            let len = self.store.read().unwrap().len();
            self.sender.send(format!("{{\"ok\":1,\"len\":{}}}", len))?;
            Ok(())
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

            let mut entry: Option<i32> = None;
            if let Value::Number(ref n) = data["entry"] {
                if let Some(i) = n.as_i64() {
                    entry = Some(i as i32);
                };
            };

            let entry = match entry {
                Some(e) => e,
                _ => {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                }
            };

            if let Some(id) = self.limited {
                if id != entry {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                }
            }

            let fullname = format!("{}.{}.{}", entry, basename, ext);
            let path = Path::new("static/store/").join(fullname);
            let opening = OpenOptions::new().write(true).create(true).open(path);
            self.uploading = match opening {
                Ok(f) => Some(f),
                Err(e) => return Err(err_to_wserr(e, "Cannot create file")),
            };
            self.sender.send("{\"ok\":1}")?;
            Ok(())
        } else if data["cmd"] == "files" {
            self.files(data["entry"].clone())
        } else if data["cmd"] == "genKey" {
            let number = match data["target"] {
                Value::Number(ref n) => {
                    if let Some(n) = n.as_i64() { n as i32 }
                    else {
                        self.sender.send("{\"ok\":0}")?;
                        return Ok(());
                    }
                },
                _ => {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                },
            };
            let key = match self.generate_key(number) {
                Err(_) => {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                },
                Ok(s) => s
            };
            let s = json!({
                "ok": 1,
                "key": key,
            }).to_string();
            self.sender.send(s)?;
            Ok(())
        } else if data["cmd"] == "deleteFile" {
            let filename = match data["target"] {
                Value::String(ref s) => s,
                _ => {
                    self.sender.send("{\"ok\":0}")?;
                    return Ok(());
                },
            };
            let path = Path::new("static/store/").join(filename);
            if let Err(e) = remove_file(path) {
                self.sender.send("{\"ok\":0}")?;
            } else {
                self.sender.send("{\"ok\":1}")?;
            }
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

        match file.write_all(frame.payload()) {
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
