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

pub enum Availability {
    Available,
    Disbanded,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    id: i32, // Integer ID
    name: String, // Name
    category: String, // Category
    tags: Vec<String>, // Tags
    desc: String, // Description
    files: Vec<String>, // Files
    icon: Option<String>, // File used as icon
    creation: String, // YYYY-MM-DD
    disbandment: Option<String>, // YYYY-MM-DD
}

#[derive(Debug)]
pub enum StoreError {
    NotFound,
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
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
pub enum IndexType {
    Name,
    Category,
    Tag,
}

impl IndexType {
    pub fn score(&self) -> u64 {
        match *self {
            IndexType::Name => 10,
            IndexType::Category => 5,
            IndexType::Tag => 1,
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

    fn mem_del(&mut self, id: i32) -> Result<(), StoreError> {
        let (_, entry) = match self.entries.entry(id) {
            Vacant(_) => return Err(StoreError::NotFound),
            Occupied(entry)  => entry.remove_entry(),
        };

        self.del_index(entry.name.clone(), &Index::new(entry.id, IndexType::Name));
        self.del_index(entry.category.clone(), &Index::new(entry.id, IndexType::Category));
        for tag in &entry.tags {
            self.del_index(tag.clone(), &Index::new(entry.id, IndexType::Tag));
        }

        Ok(())
    }

    fn mem_put(&mut self, mut entry: Entry) -> Result<(i32, Vec<u8>), StoreError> {
        // TODO: recover from failure

        let original = self.entries.get(&entry.id);
        if original.is_none() {
            let result = serde_json::to_vec(&entry).unwrap();
            let id = entry.id;
            self.add_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));
            self.add_index(entry.category.clone(), Index::new(entry.id, IndexType::Category));
            for tag in &entry.tags {
                self.add_index(tag.clone(), Index::new(entry.id, IndexType::Tag));
            }
            self.entries.insert(id, entry);
            return Ok((id, result))
        }

        let original = original.unwrap().clone();
        if entry.name != original.name {
            self.del_index(original.name.clone(), &Index::new(entry.id, IndexType::Name));
            self.add_index(entry.name.clone(), Index::new(entry.id, IndexType::Name));
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

    fn filter<'a, T: Iterator<Item=&'a str>>(&self, avail: Option<Availability>, keywords: Option<T>) -> Vec<Entry> {
        // TODO: Impl
        let mut hash: HashMap<i32,u64> = HashMap::new();
        let buckets = if let Some(iter) = keywords {
            iter.filter_map(|k| self.index.get(k))
        } else {
            return match avail {
                None => self.entries.values().cloned().collect(),
                Some(Availability::Available) =>
                    self.entries.values().filter(|e| e.disbandment.is_none()).cloned().collect(),
                Some(Availability::Disbanded) =>
                    self.entries.values().filter(|e| e.disbandment.is_some()).cloned().collect(),
            }
        };

        for bucket in buckets {
            for &Index{ ref id, ref t } in bucket {
                *(hash.entry(*id).or_insert(0)) += t.score();
            };
        };

        let mut ids: Vec<i32> = hash.keys().cloned().collect();
        ids.sort_by(|a,b| hash[b].cmp(&hash[a]));

        let it = ids.iter().map(|i| &self.entries[i]);
        if let Some(a) = avail {
            match a {
                Availability::Available => it.filter(|e| e.disbandment.is_none()).cloned().collect(),
                Availability::Disbanded => it.filter(|e| e.disbandment.is_some()).cloned().collect(),
            }
        } else { it.cloned().collect() }
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
            store.internal.mem_put(entry).unwrap();
        }
        store
    }

    pub fn close(&mut self) {
        println!("Syncing storage...");
        // TODO: try to drop self.db
    }

    pub fn put(&mut self, entry: Entry) -> Result<(), StoreError> {
        let (id, content) = self.internal.mem_put(entry)?;
        self.db.put(WriteOptions::new(), id, &content).unwrap();
        Ok(())
    }

    pub fn del(&mut self, id: i32) -> Result<(), StoreError> {
        self.internal.mem_del(id)?;
        self.db.delete(WriteOptions::new(), id).unwrap();
        Ok(())
    }

    pub fn filter<'a, T: Iterator<Item=&'a str>>(&self, avail: Option<Availability>, keywords: Option<T>) -> Vec<Entry> {
        self.internal.filter(avail, keywords)
    }
}
