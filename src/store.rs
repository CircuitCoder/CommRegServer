use leveldb::database::Database;
use leveldb::iterator::Iterable;
use leveldb::kv::KV;
use leveldb::options::*;
use serde_json;
use std::collections::*;
use std::collections::hash_map::Entry::*;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use jieba::Jieba;

fn get_false() -> bool {
    false
}

fn is_false(b: &bool) -> bool {
    return !b;
}

lazy_static! {
    pub static ref JIEBA: Jieba = Jieba::new(Path::new("./deps/jieba/lib/dict")).unwrap();
}

pub enum Availability {
    Available,
    Disbanded,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    id: i32, // Integer ID
    name: String, // Name
    name_eng: String, // English name
    category: String, // Category
    tags: Vec<String>, // Tags
    desc: String, // Description
    desc_eng: String, // English description
    files: Vec<String>, // Files
    icon: Option<String>, // File used as icon
    creation: String, // YYYY-MM-DD
    disbandment: Option<String>, // YYYY-MM-DD

    #[serde(default = "get_false", skip_serializing_if="is_false")]
    deleted: bool,

    #[serde(default = "get_false", skip_serializing_if="is_false")]
    hidden: bool,
}

impl Entry {
    pub fn id(&self) -> i32 {
        self.id
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RawEntry {
    name: String,
    name_eng: String,
    category: String,
    tags: String,
    desc: String,
    desc_eng: String,
    creation: String, // YYYY-MM-DD
    disbandment: Option<String>, // YYYY-MM-DD
}

impl RawEntry {
    pub fn extend(self, id: i32) -> Entry {
        let trimmed = self.tags.trim();
        let tags = if trimmed == "" {
            vec![]
        } else {
            self.tags.split(" ").map(str::to_owned).collect()
        };

        Entry {
            id,
            name: self.name,
            name_eng: self.name_eng,
            category: self.category,
            tags,
            desc: self.desc,
            desc_eng: self.desc_eng,
            files: vec![],
            icon: None,
            creation: self.creation,
            disbandment: self.disbandment,
            deleted: false,
            hidden: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StashedEntry {
    #[serde(flatten)]
    entry: Entry,
    timestamp: u64,
}

impl StashedEntry {
    fn create(entry: Entry) -> Self {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        StashedEntry{
            entry: entry,
            timestamp: t.as_secs() * 1000 + t.subsec_nanos() as u64 / 1_000_000
        }
    }

    fn content(&self) -> &Entry {
        &self.entry
    }

    fn get(self) -> Entry {
        self.entry
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum PullEntry {
    Stashed(StashedEntry),
    Unmodified(Entry),
}

impl PullEntry {
    fn id(&self) -> i32 {
        match self {
            &PullEntry::Stashed(ref e) => e.content().id,
            &PullEntry::Unmodified(ref e) => e.id,
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    NotFound,
    Denied,
    InvalidString,
    DeletedEntry,
    SystemError,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StoreError")
    }
}

impl Error for StoreError {
    fn description(&self) -> &'static str {
        match *self {
            StoreError::NotFound => "Entry not found",
            StoreError::Denied => "Operation denied",
            StoreError::InvalidString => "Invalid String: contains NUL",
            StoreError::DeletedEntry => "Deleted entries cannot be modified",
            StoreError::SystemError => "Cannot invoke system API",
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
pub enum IndexType {
    Name,
    NameSeg,
    Category,
    Tag,
}

impl IndexType {
    pub fn score(&self) -> u64 {
        match *self {
            IndexType::Name => 10,
            IndexType::NameSeg => 1,
            IndexType::Category => 5,
            IndexType::Tag => 2,
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
pub struct Index {
    id: i32,
    t: IndexType,
}

impl Index {
    pub fn new(id: i32, t: IndexType) -> Index {
        Index{ id, t }
    }
}

struct InternalStore {
    entries: HashMap<i32, Entry>,
    index: HashMap<String, HashMap<Index, i64>>,
}

impl InternalStore {
    fn len(&self) -> i32 {
        self.entries.len() as i32
    }

    fn add_name_seg(&mut self, name: String, id: i32) -> Result<(), StoreError> {
        let segs = match JIEBA.cut_for_search(&name) {
            Err(_) => return Err(StoreError::InvalidString),
            Ok(s) => s,
        };

        for seg in segs.iter() {
            self.add_index(seg.to_owned(), Index::new(id, IndexType::NameSeg));
        };

        Ok(())
    }

    fn del_name_seg(&mut self, name: String, id: i32) -> Result<(), StoreError> {
        let segs = match JIEBA.cut_for_search(&name) {
            Err(_) => return Err(StoreError::InvalidString),
            Ok(s) => s,
        };

        for seg in segs.iter() {
            self.del_index(seg.to_owned(), Index::new(id, IndexType::NameSeg));
        };

        Ok(())
    }

    fn add_index(&mut self, key: String, target: Index) {
        let entry = self.index.entry(key).or_insert_with(HashMap::new);
        *(entry.entry(target).or_insert(0)) += 1;
    }

    fn del_index(&mut self, key: String, target: Index) -> bool {
        let mut entry = match self.index.entry(key) {
            Vacant(_) => return false,
            Occupied(mut entry) => entry
        };

        let mut inner = match entry.get_mut().entry(target) {
            Vacant(_) => return false,
            Occupied(mut entry) => entry
        };

        let content = inner.get_mut();
        *content -= 1;
        if *content == 0 {
            inner.remove_entry();
        };
        true
    }

    fn mem_del(&mut self, id: i32) -> Result<Vec<u8>, StoreError> {
        let mut entry = match self.entries.entry(id) {
            Vacant(_) => return Err(StoreError::NotFound),
            Occupied(mut entry)  => entry,
        };

        let entry = entry.get_mut();

        self.del_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));
        self.del_index(entry.name_eng.clone(), Index::new(entry.id, IndexType::Name));
        self.del_name_seg(entry.name.clone(), entry.id)?;
        self.del_name_seg(entry.name_eng.clone(), entry.id)?;
        self.del_index(entry.category.clone(), Index::new(entry.id, IndexType::Category));
        for tag in &entry.tags {
            self.del_index(tag.clone(), Index::new(entry.id, IndexType::Tag));
        }

        entry.deleted = true;
        let result = serde_json::to_vec(entry).unwrap();
        Ok(result)
    }

    fn mem_load(&mut self, entry: Entry) {
        self.entries.insert(entry.id, entry);
    }

    fn mem_put(&mut self, mut entry: Entry) -> Result<(i32, Vec<u8>), StoreError> {
        // TODO: recover from failure

        let original = self.entries.get(&entry.id);

        if original.is_none() {
            let result = serde_json::to_vec(&entry).unwrap();
            let id = entry.id;
            self.add_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));
            self.add_index(entry.name_eng.clone(), Index::new(entry.id, IndexType::Name));
            self.add_name_seg(entry.name.clone(), entry.id)?;
            self.add_name_seg(entry.name_eng.clone(), entry.id)?;
            self.add_index(entry.category.clone(), Index::new(entry.id, IndexType::Category));
            for tag in &entry.tags {
                self.add_index(tag.clone(), Index::new(entry.id, IndexType::Tag));
            }
            self.entries.insert(id, entry);
            return Ok((id, result))
        }

        let original = original.unwrap().clone();

        if original.deleted {
            return Err(StoreError::DeletedEntry);
        };

        if entry.name != original.name {
            self.del_index(original.name.clone(), Index::new(entry.id, IndexType::Name));
            self.add_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));

            self.del_name_seg(original.name.clone(), entry.id)?;
            self.add_name_seg(entry.name.clone(), entry.id)?;
        }

        if entry.name_eng != original.name_eng {
            self.del_index(original.name_eng.clone(), Index::new(entry.id, IndexType::Name));
            self.add_index(entry.name_eng.clone(), Index::new(entry.id, IndexType::Name));

            self.del_name_seg(original.name_eng.clone(), entry.id)?;
            self.add_name_seg(entry.name_eng.clone(), entry.id)?;
        }

        if entry.category != original.category {
            self.del_index(original.category.clone(), Index::new(entry.id, IndexType::Category));
            self.add_index(entry.category.clone(), Index::new(entry.id, IndexType::Category));
        }

        // Sorting tags
        entry.tags.sort();
        let mut ctags = entry.tags.iter();
        let mut otags = original.tags.iter();

        let mut ctag = ctags.next();
        let mut otag = otags.next();
        loop {
            let ctag_content;
            let otag_content;
            if ctag.is_none() {
                while let Some(tag) = otag {
                    self.del_index(tag.clone(), Index::new(entry.id, IndexType::Tag));
                    otag = otags.next();
                }
                break;
            } else {
                ctag_content = ctag.unwrap().clone();
            }

            if otag.is_none() {
                while let Some(tag) = ctag {
                    self.add_index(tag.clone(), Index::new(entry.id, IndexType::Tag));
                    ctag = ctags.next();
                }
                break;
            } else {
                otag_content = otag.unwrap().clone();
            }

            if ctag_content == otag_content {
                ctag = ctags.next();
                otag = otags.next();
            } else if otag_content < ctag_content {
                self.del_index(otag_content, Index::new(entry.id, IndexType::Tag));
                otag = otags.next();
            } else {
                self.add_index(ctag_content, Index::new(entry.id, IndexType::Tag));
                ctag = ctags.next();
            }
        }

        let result = serde_json::to_vec(&entry).unwrap();
        let id = entry.id;
        self.entries.insert(id, entry);
        Ok((id, result))
    }

    fn filter<'a, T: Iterator<Item=&'a str>>(
        &self,
        avail: Option<Availability>,
        keywords: Option<T>) -> Vec<Entry> {
        // TODO: Impl
        let mut hash: HashMap<i32,i64> = HashMap::new();
        let words = if let Some(iter) = keywords {
            iter.filter_map(|k| {
                JIEBA.cut_for_search(k).ok()
            })
        } else {
            let source = self.entries
                .values()
                .filter(|e| !e.deleted && !e.hidden);
            let mut result: Vec<Entry> = match avail {
                None => source.cloned().collect(),
                Some(Availability::Available) =>
                    source.filter(|e| e.disbandment.is_none()).cloned().collect(),
                Some(Availability::Disbanded) =>
                    source.filter(|e| e.disbandment.is_some()).cloned().collect(),
            };

            result.sort_unstable_by(|a, b| { a.name.cmp(&b.name) });

            return result;
        };

        for word in words {
            let buckets = word.iter().filter_map(|k| self.index.get(k));
            for bucket in buckets {
                for (&Index{ ref id, ref t }, &count) in bucket.iter() {
                    *(hash.entry(*id).or_insert(0)) +=
                        (t.score() as i64)*count;
                };
            };
        };

        let mut ids: Vec<i32> = hash.keys().cloned().collect();

        ids.sort_unstable_by_key(|i| { (-hash[i], &self.entries[i].name) });

        let it = ids.iter().map(|i| &self.entries[i]);
        if let Some(a) = avail {
            match a {
                Availability::Available => it.filter(|e| e.disbandment.is_none()).cloned().collect(),
                Availability::Disbanded => it.filter(|e| e.disbandment.is_some()).cloned().collect(),
            }
        } else { it.cloned().collect() }
    }

    fn fetch(&self, id: i32) -> Option<Entry> {
        self.entries.get(&id).cloned()
    }

    fn cmp_entry(&self, entry: &Entry) -> bool {
        match self.entries.get(&entry.id) {
            Some(e) => e == entry,
            None => false,
        }
    }

    fn highest_id(&self) -> i32 {
        self.entries.keys().max().cloned().unwrap_or(0)
    }
}

pub struct Store {
    db: Database<i32>,
    stash: HashMap<i32, StashedEntry>,
    internal: InternalStore,
}

impl Store {
    pub fn new() -> Store {
        let mut dbopt = Options::new();
        dbopt.create_if_missing = true;
        let db = Database::open(Path::new("./db"), dbopt).unwrap();

        let stash = File::open(Path::new("./stash.json"))
            .map(|f| serde_json::from_reader(f).unwrap())
            .unwrap_or_else(|e| HashMap::new());

        let mut store = Store {
            db,
            stash,
            internal: InternalStore {
                entries: HashMap::new(),
                index: HashMap::new(),
            },
        };

        let iter = store.db.iter(ReadOptions::new());
        for (_, slice) in iter {
            let entry: Entry = serde_json::from_slice(&slice).unwrap();
            if entry.deleted {
                store.internal.mem_load(entry);
            } else {
                store.internal.mem_put(entry).unwrap();
            }
        }
        store
    }

    pub fn close(&mut self) {
        println!("Syncing storage...");
        let stash = File::create(Path::new("./stash.json")).unwrap();
        serde_json::to_writer(stash, &self.stash).unwrap();
        // TODO: try to drop self.db
    }

    pub fn len(&self) -> i32 {
        self.internal.len()
    }

    pub fn pull(&self) -> Vec<PullEntry> {
        let all = self.internal.entries.values().filter(|v| { !v.deleted });
        let mut result: Vec<PullEntry> = all.map(|v| {
            match self.stash.get(&v.id) {
                None => PullEntry::Unmodified(v.clone()),
                Some(e) => PullEntry::Stashed(e.clone()),
            }
        }).collect();
        result.sort_unstable_by_key(|a| { a.id() });
        result
    }

    pub fn pull_fetch(&self, id: i32) -> Option<PullEntry> {
        self.stash
            .get(&id)
            .cloned()
            .map(PullEntry::Stashed)
            .or_else(|| { self.fetch(id).map(PullEntry::Unmodified) })
    }

    pub fn stash(&mut self, mut entry: Entry, restricted: bool) -> Result<(), StoreError> {
        if entry.id > self.internal.len() {
            // Is a new entry

            if restricted {
                return Err(StoreError::Denied);
            }

            let id = entry.id;
            self.stash.insert(id, StashedEntry::create(entry.clone()));

            entry.hidden = true;
            self.put(entry)
        } else if self.internal.cmp_entry(&entry) {
            self.discard(entry.id);
            Ok(())
        } else {
            let id = entry.id;
            self.stash.insert(id, StashedEntry::create(entry));
            Ok(())
        }
    }

    pub fn commit(&mut self, id: i32) -> Result<(), StoreError> {
        match self.stash.entry(id) {
            Vacant(_) => Ok(()),
            Occupied(m) => {
                let (_, v) = m.remove_entry();
                self.put(v.get())
            }
        }
    }

    pub fn discard(&mut self, id: i32) {
        if let Occupied(m) = self.stash.entry(id) {
            m.remove_entry();
        }
    }

    fn put(&mut self, entry: Entry) -> Result<(), StoreError> {
        let (id, content) = self.internal.mem_put(entry)?;
        self.db.put(WriteOptions::new(), id, &content).unwrap();
        Ok(())
    }

    pub fn del(&mut self, id: i32) -> Result<(), StoreError> {
        let entry = self.internal.mem_del(id)?;
        self.db.put(WriteOptions::new(), id, &entry).unwrap();
        self.stash.remove(&id);
        Ok(())
    }

    pub fn filter<'a, T: Iterator<Item=&'a str>>(
        &self,
        avail: Option<Availability>,
        keywords: Option<T>) -> Vec<Entry> {
        self.internal.filter(avail, keywords)
    }

    pub fn fetch(&self, id: i32) -> Option<Entry> {
        self.internal.fetch(id)
    }

    pub fn highest_id(&self) -> i32 {
        self.internal.highest_id()
    }
}
