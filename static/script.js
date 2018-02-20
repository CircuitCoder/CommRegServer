const DEBOUNCE_TIMEOUT = 500;
const CATEGORIES = [
  '素拓',
  '艺术',
  '公益',
  '体育',
  '科创',
  '文化',
];

let conn;

function buildWsURI(key) {
  if (location.protocol === 'https:') {
      return `wss://${location.hostname}:38265/${key}`;
  } else {
      return `ws://${location.hostname}:38265/${key}`;
  }
}

function sendWait(data) {
  return new Promise((resolve, reject) => {
    conn.onmessage = msg => {
      resolve(JSON.parse(msg.data))
    };
    conn.send(JSON.stringify(data));
  });
}

function deepClone(a) {
  // Only support simple objects & arrays
  if(a === null) return null;
  if(Array.isArray(a)) return a.map(deepClone);
  if(typeof a === 'object') {
    const result = {};
    for(const k in a)
      result[k] = deepClone(a[k]);
    return result;
  }
  return a; // Simple assignment
}

function deepEq(a, b) {
  // Only sopports plain objs, arrays and fundmental types
  if(a === null) return b === null; // typeof null === 'object'
  if(Array.isArray(a)) {
    return Array.isArray(b)
      && a.length == b.length
      && a.every((e, id) => deepEq(e, b[id]));
  }

  if(typeof a === 'object') {
    return typeof b === 'object'
      && deepEq(Object.keys(a).sort(), Object.keys(b).sort())
      && Object.keys(a).every(id => deepEq(a[id], b[id]));
  }

  else return a === b;
}

const desc = {
  el: '#app',
  data: {
    connected: false,
    wrongKey: false,
    authKey: '',
    entries: [],
    referenceEntries: [],

    updateDebouncer: null,
    activeCategory: null,
    activeTag: null,
    tagFilter: '',
  },
  methods: {
    connect() {
      conn = new WebSocket(buildWsURI(this.authKey));
      conn.onmessage = (msg) => {
        try {
          const data = JSON.parse(msg.data);
          if(data.ok) {
            this.init();
            return true;
          }
        } catch(e) { console.error(e); }
        this.wrongKey = true;
      }
    },

    async init() {
      this.connected = true;
      await this.syncDown();
    },

    async syncDown() {
      const data = await sendWait({ cmd: 'list' });
      this.entries = data.sort((a,b) => {
        if(a.id < b.id) return -1;
        if(a.id > b.id) return 1;
        return 0;
      });
      this.referenceEntries = deepClone(this.entries);

      setTimeout(() => {
        let areas = document.querySelectorAll('.row textarea');
        areas.forEach(e => {
          this.autoresize(e);
        });
      });
    },

    async syncUp() {
      // Assuming the array has ascending ID
      const snapshot = deepClone(this.entries);
      let curPtr = 0;
      //TODO: deleting
      for(let e of snapshot) {
        if(curPtr >= this.referenceEntries.length || this.referenceEntries[curPtr].id  > e.id) {
          const data = await sendWait({ cmd: 'put', payload: e });
        } else if(this.referenceEntries[curPtr].id === e.id) {
          if(!deepEq(e, this.referenceEntries[curPtr])) {
            await sendWait({ cmd: 'put', payload: e });
          }
          ++curPtr;
        }
      }

      this.referenceEntries = snapshot;
      // TODO: Syncdown will abort editing process
    },

    findMaxId() {
      return this.entries.reduce((acc, e) => e.id > acc ? e.id : acc, 0);
    },

    add() {
      let id = this.findMaxId() + 1;
      this.entries.push({
        id,
        name: '',
        category: '',
        tags: [],
        desc: '',
        creation: 'FIXME',
        disbanded: null,
      });
    },

    inputCate(entry) {
      this.activeCategory = entry;
    },

    applyCate(cate) {
      this.activeCategory.category = cate;
    },

    discardCate(e) {
      this.activeCategory = null;
    },

    addTag(entry, ev) {
      ev.preventDefault();
      if(ev.target.value === '') return;
      // TODO: warn about duplicate
      entry.tags.push(ev.target.value);
      ev.target.value = '';

      this.tagFilter = '';
    },

    delTag(entry, id, ev) {
      entry.tags.splice(id, 1);
    },

    delLastTag(entry, ev) {
      if(ev.target.value !== '') return;
      ev.preventDefault();
      if(entry.tags.length　> 0) entry.tags.pop();
    },

    inputTag(entry, ev) {
      this.activeTag = entry;
      this.tagFilter = ev.target.value;
    },

    applyTag(tag) {
      this.activeTag.tags.push(tag);
    },

    discardTag(entry, ev) {
      if(ev.target.value !== '') this.addTag(entry, ev);
      this.activeTag = null;
    },

    updateTagFilter(ev) {
      this.tagFilter = ev.target.value;
    },

    autoresize(target) {
      // Minimal height: 2 lines + border = 60px
      target.style.height = '60px';
      // Then set height to scrollHeight
      console.log(target.scrollHeight);
      target.style.height = target.scrollHeight + 'px';
    },
  },

  computed: {
    frequentTags() {
      if(!this.entries) return [];

      const count = new Map();
      for(const e of this.entries)
        for(const t of e.tags)
          if(t.indexOf(this.tagFilter) === 0) { // tagFilter === '' is correctly handled
            if(count.has(t)) count.set(t, count.get(t) + 1);
            else count.set(t, 1);
          }

      const tags = Array.from(count.keys());
      tags.sort((a,b) => {
        if(count.get(a) < count.get(b)) return 1;
        else if(count.get(a) === count.get(b)) return 0;
        else return -1;
      });

      return tags.splice(0, 20); // First ten
    },
  },

  watch: {
    authKey() {
      this.wrongKey = false;
    },
    entries: {
      handler() {
        if(this.updateDebouncer !== null) {
          clearTimeout(this.updateDebouncer);
        }

        this.updateDebouncer = setTimeout(async () => {
          this.updateDebouncer = null;
          await this.syncUp();
          // TODO: notification
        }, DEBOUNCE_TIMEOUT);
      },
      deep: true,
    },
  },
};

function setup() {
  const app = new Vue(desc);
}
