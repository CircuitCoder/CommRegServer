use leveldb::database::Database;
use leveldb::iterator::Iterable;
use leveldb::kv::KV;
use leveldb::options::*;
use serde_json;
use std::collections::*;
use std::collections::hash_map::Entry::*;
use std::error::Error;
use std::fmt;
use std::path::Path;
use jieba::Jieba;

fn get_false() -> bool {
    false
}

lazy_static! {
    pub static ref JIEBA: Jieba = Jieba::new(Path::new("./deps/jieba/lib/dict")).unwrap();
}

pub enum Availability {
    Available,
    Disbanded,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

    #[serde(default = "get_false")]
    deleted: bool,
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
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    NotFound,
    Denied,
    InvalidString,
    DeletedEntry,
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
    index: HashMap<String, HashSet<Index>>,
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
            self.del_index(seg.to_owned(), &Index::new(id, IndexType::NameSeg));
        };

        Ok(())
    }

    fn add_index(&mut self, key: String, target: Index) -> bool {
        let entry = self.index.entry(key).or_insert_with(HashSet::new);
        entry.insert(target)
    }

    fn del_index(&mut self, key: String, target: &Index) -> bool {
        match self.index.entry(key) {
            Vacant(_) => false,
            Occupied(ref mut entry) => entry.get_mut().remove(target)
        }
    }

    fn mem_del(&mut self, id: i32) -> Result<Vec<u8>, StoreError> {
        let mut entry = match self.entries.entry(id) {
            Vacant(_) => return Err(StoreError::NotFound),
            Occupied(mut entry)  => entry,
        };

        let entry = entry.get_mut();

        self.del_index(entry.name.clone(), &Index::new(entry.id, IndexType::Name));
        self.del_name_seg(entry.name.clone(), entry.id)?;
        self.del_index(entry.category.clone(), &Index::new(entry.id, IndexType::Category));
        for tag in &entry.tags {
            self.del_index(tag.clone(), &Index::new(entry.id, IndexType::Tag));
        }

        entry.deleted = true;
        let result = serde_json::to_vec(entry).unwrap();
        Ok(result)
    }

    fn mem_put(&mut self, mut entry: Entry, restricted: bool) -> Result<(i32, Vec<u8>), StoreError> {
        // TODO: recover from failure

        let original = self.entries.get(&entry.id);

        if restricted {
            match original {
                None => return Err(StoreError::Denied),
                Some(e) if e.disbandment.is_some() => return Err(StoreError::Denied),
                _ => {}, // No-op
            }
        }

        if original.is_none() {
            let result = serde_json::to_vec(&entry).unwrap();
            let id = entry.id;
            self.add_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));
            self.add_name_seg(entry.name.clone(), entry.id)?;
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
            self.del_index(original.name.clone(), &Index::new(entry.id, IndexType::Name));
            self.add_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));

            self.del_name_seg(original.name.clone(), entry.id)?;
            self.add_name_seg(entry.name.clone(), entry.id)?;
        }
        if entry.category != original.category {
            self.del_index(original.category.clone(), &Index::new(entry.id, IndexType::Category));
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
                    self.del_index(tag.clone(), &Index::new(entry.id, IndexType::Tag));
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
                self.del_index(otag_content, &Index::new(entry.id, IndexType::Tag));
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
        keywords: Option<T>,
        sort_by_id: bool) -> Vec<Entry> {
        // TODO: Impl
        let mut hash: HashMap<i32,i64> = HashMap::new();
        let words = if let Some(iter) = keywords {
            iter.filter_map(|k| {
                let s = k.clone();
                JIEBA.cut_for_search(&s).ok()
            })
        } else {
            let source = self.entries.values().filter(|e| !e.deleted);
            let mut result: Vec<Entry> = match avail {
                None => source.cloned().collect(),
                Some(Availability::Available) =>
                    source.filter(|e| e.disbandment.is_none()).cloned().collect(),
                Some(Availability::Disbanded) =>
                    source.filter(|e| e.disbandment.is_some()).cloned().collect(),
            };

            if sort_by_id {
                result.sort_unstable_by_key(|a| { a.id });
            } else {
                result.sort_unstable_by(|a, b| { a.name.cmp(&b.name) });
            }

            return result;
        };

        for word in words {
            let buckets = word.iter().filter_map(|k| self.index.get(k));
            for bucket in buckets {
                for &Index{ ref id, ref t } in bucket {
                    *(hash.entry(*id).or_insert(0)) += t.score() as i64;
                };
            };
        };

        let mut ids: Vec<i32> = hash.keys().cloned().collect();

        if sort_by_id {
            ids.sort_unstable_by_key(|i| { (-hash[i], i.clone()) });
        } else {
            ids.sort_unstable_by_key(|i| { (-hash[i], &self.entries[i].name) });
        }

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

    fn highest_id(&self) -> i32 {
        self.entries.keys().max().cloned().unwrap_or(0)
    }
}

pub struct Store {
    db: Database<i32>,
    internal: InternalStore,
}

impl Store {
    pub fn new() -> Store {
        let mut dbopt = Options::new();
        dbopt.create_if_missing = true;
        let db = Database::open(Path::new("./db"), dbopt).unwrap();
        let mut store = Store {
            db,
            internal: InternalStore {
                entries: HashMap::new(),
                index: HashMap::new(),
            },
        };

        let iter = store.db.iter(ReadOptions::new());
        for (_, slice) in iter {
            let entry: Entry = serde_json::from_slice(&slice).unwrap();
            store.internal.mem_put(entry, false).unwrap();
        }
        store
    }

    pub fn close(&mut self) {
        println!("Syncing storage...");
        // TODO: try to drop self.db
    }

    pub fn len(&self) -> i32 {
        self.internal.len()
    }

    pub fn put(&mut self, entry: Entry, restricted: bool) -> Result<(), StoreError> {
        let (id, content) = self.internal.mem_put(entry, restricted)?;
        self.db.put(WriteOptions::new(), id, &content).unwrap();
        Ok(())
    }

    pub fn del(&mut self, id: i32) -> Result<(), StoreError> {
        let entry = self.internal.mem_del(id)?;
        self.db.put(WriteOptions::new(), id, &entry).unwrap();
        Ok(())
    }

    pub fn filter<'a, T: Iterator<Item=&'a str>>(
        &self,
        avail: Option<Availability>,
        keywords: Option<T>,
        sort_by_id: bool) -> Vec<Entry> {
        self.internal.filter(avail, keywords, sort_by_id)
    }

    pub fn fetch(&self, id: i32) -> Option<Entry> {
        self.internal.fetch(id)
    }

    pub fn highest_id(&self) -> i32 {
        self.internal.highest_id()
    }
}
