use byteorder::{ByteOrder, LittleEndian};
use config::Config;
use ring::aead;
use ring::digest;
use ring::error::Unspecified;
use ring::rand::{SystemRandom, SecureRandom};
use serde::Serializer;
use serde_json::Value;
use serde_json;
use std::borrow::Cow;
use std::fs::*;
use std::io::Write;
use std::path::Path;
use std::str::Split;
use std::sync::*;
use std;
use store::{Store, Entry};
use uuid::Uuid;
use ws::{Sender, Handshake, Message, Frame};
use ws;

fn err_to_wserr<T, I: Into<Cow<'static, str>>>(e: T, reason: I) -> ws::Error
  where T: 'static + std::error::Error + Send + Sync {
    ws::Error::new(ws::ErrorKind::Custom(Box::new(e)), reason)
}

pub struct Handler {
    sender: Sender,
    store: &'static RwLock<Store>,
    config: &'static Config,
    uploading: Option<File>,
}

impl Handler {
    pub fn new(sender: Sender, store: &'static RwLock<Store>, config: &'static Config) -> Handler {
        Handler { sender, store, config, uploading: None }
    }

    fn list(&self) -> ws::Result<()> {
        let s = match serde_json::to_string(&self.store.read().unwrap().filter::<Split<&str>>(None, None)) {
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

        if let Err(e) = self.store.write().unwrap().put(payload) {
            Err(err_to_wserr(e, "Storage failure"))
        } else {
            self.sender.send("{\"ok\":1}")?;
            Ok(())
        }
    }

    fn del(&self, target: Value) -> ws::Result<()> {
        if let Value::Number(n) = target {
            if let Some(id) = n.as_i64() {
                if self.store.write().unwrap().del(id as i32).is_ok() {
                    self.sender.send("{\"ok\":1}")?;
                    return Ok(())
                }
            }
        }
        self.sender.send("{\"ok\":0}")?;
        Ok(())
    }

    fn files(&self) -> ws::Result<()> {
        let iter = match read_dir(Path::new("./static/store")) {
            Err(e) => return Err(err_to_wserr(e, "Not privleged to read director")),
            Ok(i) => i,
        };

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

        let mut collected: Vec<_> = filtered.collect();
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

    // Authentication utilities for a single entry
    fn generate_key_impl(&self, target: i32) -> Result<String, Unspecified> {
        // Convert to bytes
        let mut buf: [u8; 4 + aead::MAX_TAG_LEN] = [0; 4 + aead::MAX_TAG_LEN]; // 4 bytes for input, MAX_LAG_LEN for cap
        LittleEndian::write_i32(&mut buf, target);

        // Create a 256-bit hash of the master secret
        let hash = digest::digest(&digest::SHA256, self.config.secret.as_bytes());

        // Creating sealing key
        let sealing_key = aead::SealingKey::new(&aead::AES_256_GCM, hash.as_ref())?;

        // Generate a 96-bit nonce
        let mut nonce: [u8; 12] = [0; 12];
        let rng = SystemRandom::new();
        rng.fill(&mut nonce)?;

        let len = aead::seal_in_place(
            &sealing_key,
            &nonce,
            &[],
            &mut buf,
            aead::MAX_TAG_LEN)?;
        let mut result = String::with_capacity(12 * 2 + len * 2);
        for byte in &nonce {
            write!(&mut result as &mut std::fmt::Write, "{:02x}", byte).map_err(|_| Unspecified)?
        }
        for byte in &buf[..len] {
            write!(&mut result as &mut std::fmt::Write, "{:02x}", byte).map_err(|_| Unspecified)?
        }
        Ok(result)
    }
}

impl ws::Handler for Handler {
    fn on_open(&mut self, shake: Handshake) -> ws::Result<()> {
        if shake.request.resource() == format!("/{}", self.config.secret) {
            self.sender.send("{\"ok\":1}")?;
            return Ok(())
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
                Err(e) => return Err(err_to_wserr(e, "Cannot create file")),
            };
            self.sender.send("{\"ok\":1}")?;
            Ok(())
        } else if data["cmd"] == "files" {
            self.files()
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
            let key = match self.generate_key_impl(number) {
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
