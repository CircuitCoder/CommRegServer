const DEBOUNCE_TIMEOUT = 500;
const CATEGORIES = [
  '素拓',
  '艺术',
  '公益',
  '体育',
  '科创',
  '文化',
];
const DATE_FORMAT = "YYYY-MM-DD";
const FP_FORMAT = "Y-m-d";
let CONFIG;

let conn;

function buildWsURI(key) {
  let host = CONFIG.ws.host;
  let port = ':' + CONFIG.ws.port;
  if(host === '0.0.0.0') { // Virtual interface
    host = location.hostname;
  }
  if(CONFIG.proxied) {
    host = location.hostname;
    port = location.port;
    if(port !== '') port = ':' + port;

    key = CONFIG.proxied + '/' + key;
  } else {
    key = '/' + key;
  };

  if (location.protocol === 'https:') {
      return `wss://${host}${port}${key}`;
  } else {
      return `ws://${host}${port}${key}`;
  }
}

function sendWait(data, raw = false) {
  return new Promise((resolve, reject) => {
    conn.onmessage = msg => {
      resolve(JSON.parse(msg.data))
    };
    const compiled = raw ? data : JSON.stringify(data);
    conn.send(compiled);
  });
}

function readAsTA(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      return resolve(reader.result);
    };
    reader.onerror = reject;
    reader.readAsArrayBuffer(file);
  });
}

async function uploadFile(dt, entry) {
  const segs = dt.name.split('.');
  const ext = segs[segs.length-1];

  const createResult = await sendWait({ cmd: 'upload', ext, entry });
  if(!createResult.ok) return false;

  const content = await readAsTA(dt);
  const uploadResult = await sendWait(content, true);
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
    connectionDown: false,
    wrongKey: false,
    authKey: '',
    limited: null,
    locked: false,
    entries: [],
    referenceEntries: [],
    fileStore: {},
    searchStr: '',
    // TODO: filtered changes when searchStr changes, or input loses focus

    updateDebouncer: null,
    activeCategory: null,
    activeTag: null,
    activeTagInput: null,
    activeFile: null,
    tagFilter: '',
    dragging: 0,
    uploading: false,
    pendingDeletion: null,

    ctrlDown: false,
  },
  created() {
    window.addEventListener('keydown', e => {
      if(e.key === 'Control') this.ctrlDown = true;
      console.log(this.ctrlDown);
    });
    window.addEventListener('keyup', e => {
      if(e.key === 'Control') this.ctrlDown = false;
    });
  },
  methods: {
    connect() {
      conn = new WebSocket(buildWsURI(this.authKey));
      conn.onmessage = (msg) => {
        try {
          const data = JSON.parse(msg.data);
          if(data.ok) {
	    if('limited' in data) this.limited = data.limited;

            this.connectionDown = false;
            if(!this.connected)
              this.init();
            // TODO: regular refresh
            return true;
          }
        } catch(e) { console.error(e); }
        this.wrongKey = true;
      }
      conn.onclose = () => {
        this.connectionDown = true;
	setTimeout(() => {
	  this.connect(); // Try reconnect immediately
	}, 1000);
      };
    },

    async init() {
      this.connected = true;
      await this.syncDown();
      this.filteredEntries = [...this.entries]; // Show all
    },

    async syncDown() {
      const data = await sendWait({ cmd: 'list' });
      this.entries = data.sort((a,b) => {
        if(a.id < b.id) return -1;
        if(a.id > b.id) return 1;
        return 0;
      });
      this.referenceEntries = deepClone(this.entries);

      if(this.limited !== null)
	this.locked = this.entries.length === 0 || this.entries[0].disbandment !== null;

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
      for(let e of snapshot) {
        while(curPtr < this.referenceEntries.length && this.referenceEntries[curPtr].id < e.id) {
          // Delete
          await sendWait({ cmd: 'del', target: this.referenceEntries[curPtr].id });
          ++curPtr;
        }

        if(curPtr >= this.referenceEntries.length || this.referenceEntries[curPtr].id > e.id) {
          // New
          const data = await sendWait({ cmd: 'put', payload: e });
        } else {
          if(!deepEq(e, this.referenceEntries[curPtr])) {
            // Update
            await sendWait({ cmd: 'put', payload: e });
          }
          ++curPtr;
        }
      }
      while(curPtr < this.referenceEntries.length) {
        // Delete tailing
        await sendWait({ cmd: 'del', target: this.referenceEntries[curPtr].id });
        ++curPtr;
      }

      this.referenceEntries = snapshot;
      // TODO: Syncdown will abort editing process
    },

    async listFiles(id) {
      Vue.set(this.fileStore, id, await sendWait({ cmd: 'files', entry: id }));
      console.log(this.fileStore);
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
        files: [],
        icon: null,
        creation: moment().format(DATE_FORMAT),
        disbandment: null,
      });

      setTimeout(() => {
        // Next frame
        this.$refs.anchor.scrollIntoView({ behavior: 'smooth' });
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
      if(entry.tags.includes(ev.target.value)) return;
      entry.tags.push(ev.target.value);
      entry.tags.sort();
      ev.target.value = '';

      this.tagFilter = '';
    },

    delTag(entry, id, ev) {
      entry.tags.splice(id, 1);
    },

    delLastTag(entry, ev) {
      if(ev.target.value !== '') return;
      ev.preventDefault();
      if(entry.tags.length　> 0) {
        const popped = entry.tags.pop();
        ev.target.value = popped;
        this.tagFilter = popped;
      }
    },

    inputTag(entry, ev) {
      this.activeTag = entry;
      this.activeTagInput = ev.target;
    },

    applyTag(tag) {
      this.activeTag.tags.push(tag);
      this.activeTag.tags.sort();
      if(this.activeTagInput) {
        this.activeTagInput.value = '';
        this.tagFilter = '';
      }
    },

    discardTag(entry, ev) {
      if(ev.target.value !== '') this.addTag(entry, ev);
      this.activeTag = null;
    },

    discardDeletion() {
      this.pendingDeletion = null;
    },

    doDelete(id) {
      if(this.pendingDeletion === id) {
        // Do deletion
        this.pendingDeletion = null;
        const index = this.entries.findIndex(e => e.id === id);
        this.entries.splice(index, 1);
      } else {
        setTimeout(() => {
          // Next frame
          this.pendingDeletion = id;
        });
      }
    },

    updateTagFilter(ev) {
      this.tagFilter = ev.target.value;
    },

    autoresize(target) {
      // Minimal height: 2 lines + border = 60px
      target.style.height = '60px';
      // Then set height to scrollHeight
      target.style.height = target.scrollHeight + 'px';
    },

    // Drag 'n Drop
    dragOver(ev) {
      if(this.activeFile === null) return;
      if([...ev.dataTransfer.types].includes('Files'))
        ev.preventDefault();
    },

    async drop(ev) {
      if(this.activeFile === null) return;
      if([...ev.dataTransfer.types].includes('Files'))
        ev.preventDefault();
      this.dragging = 0;
      await this.upload(ev.dataTransfer.files, this.activeFile.id);
    },

    async upload(list, id) {
      this.uploading = 0;
      for(const f of list)
        if(f.type.indexOf('image/') === 0)
          ++this.uploading;

      for(const f of list)
        if(f.type.indexOf('image/') === 0) {
          await uploadFile(f, id);
          --this.uploading;
        }
      // Upload finished, refresh list
      await this.listFiles(this.activeFile.id);
    },

    dragEnter(ev) {
      if(this.activeFile === null) return;
      if([...ev.dataTransfer.types].includes('Files'))
        ++this.dragging;
    },

    dragLeave(ev) {
      if(this.activeFile === null) return;
      if([...ev.dataTransfer.types].includes('Files'))
        --this.dragging;
    },

    addFile(entry) {
      setTimeout(() => {
        this.activeFile = entry;
      });
    },

    discardFile() {
      this.activeFile = null;
    },

    insertFile(file) {
      if(this.activeFile === null) return;
      if(this.activeFile.files.includes(file)) return;
      this.activeFile.files.push(file);
    },

    removeFile(entry, index) {
      const files = entry.files.splice(index, 1);

      // Empty the icon if needed
      if(entry.icon === files[0])
        entry.icon = null;

      // Update desc
      let segs = entry.desc.split('\n');

      for(let i = 0; i < segs.length; ++i) {
        const frontEmpty = i === 0 || segs[i-1] === '';
        const backEmpty = i === segs.length-1 || segs[i+1] === '';
        if(frontEmpty && backEmpty) {
          // Check for syntax
          let result = segs[i].match(/^<(\d+)>$/);
          if(!result) continue;
          let id = parseInt(result[1], 10);
          if(id === index+1) {
            if(i === 0) {
              segs.splice(i, 2); // Removes this one
              --i;
            } else {
              segs.splice(i-1, 2); // Removes this one
              i-=2;
            }
          } else if(id > index+1) segs[i] = `<${id-1}>`;
        }
      }

      entry.desc = segs.filter(e => e !== null).join('\n');

      const elIndex = this.entries.findIndex(e => e === entry);
      const el = this.$refs.descs[elIndex];
      setTimeout(() => {
        this.autoresize(el);
      });
    },

    setIcon(entry, file) {
      entry.icon = file;
    },

    removeIcon(entry) {
      entry.icon = null;
    },

    manualUpload() {
      this.$refs.fileSelector.click();
      this.waitForInput
    },

    doUpload() {
      if(this.$refs.fileSelector.value === '') // Already empty
        return;
      this.upload(this.$refs.fileSelector.files);
      this.$refs.fileSelector.value = '';
    },

    async genKey(entry) {
      // TODO: disable this when using as an individual
      const result = await sendWait({ cmd: 'genKey', target: entry.id });
      prompt("The world has stopped now. Copy the key and let me forget it for good.", result.key);
    },

    disband(entry) {
      entry.disbandment = moment().format(DATE_FORMAT); // ISO 8601
    },

    activate(entry) {
      entry.disbandment = null;
    },

    setupFlatpickr(event) {
      if(event.target._flatpickr) // Already setup
        return;
      const fp = flatpickr(event.target, { dateFormat: FP_FORMAT });
      fp.open();
    },

    storeUri(uri) {
      return '/store/' + uri;
    },

    discardAll() {
      this.discardDeletion();
      this.discardFile();
    },

    dispatchFileAction(file) {
      if(!this.ctrlDown) this.insertFile(file);
      else this.fullDeleteFile(file);
    },

    async fullDeleteFile(file) {
      let id = this.activeFile.id;
      await sendWait({ cmd: 'deleteFile', target: file });
      await this.listFiles(id);
    },
  },

  computed: {
    frequentTags() {
      if(!this.entries) return [];

      const count = new Map();
      for(const e of this.entries)
        for(const t of e.tags)
          if(t.indexOf(this.tagFilter) === 0) {
            // tagFilter === '' is correctly handled
            if(count.has(t)) count.set(t, count.get(t) + 1);
            else count.set(t, 1);
          }

      let tags = Array.from(count.keys());

      // Filter present tags
      if(this.activeTag)
        tags = tags.filter(t => !this.activeTag.tags.includes(t));

      tags.sort((a,b) => {
        if(count.get(a) < count.get(b)) return 1;
        else if(count.get(a) === count.get(b)) return 0;
        else return -1;
      });

      return tags.splice(0, 20); // First ten
    },

    filteredFiles() {
      if(this.files === null) return null;
      // Filter present tags
      if(this.activeFile)
        return this.files.filter(f => !this.activeFile.files.includes(f));
      else return this.files;
    },

    filteredEntries() {
      let result = this.entries;
      const segs = this.searchStr.split(' ');
      for(const seg of segs) {
        if(seg === "") continue;
        if(seg.match(/#\d+/)) { // Is id filter
          const filter = parseInt(seg.substr(1), 10);
          result = result.filter(e => e.id === filter);
        } else {
          result = result.filter(e =>
            e.name.indexOf(seg) !== -1
            || e.category === seg
            || e.tags.includes(seg)
          );
        }
      }
      return result;
    },

    files() {
      if(!this.activeFile) return null;
      const id = this.activeFile.id;
      if(!(id in this.fileStore)) {
        Vue.set(this.fileStore, id, []);
        this.listFiles(id);
      }
      console.log(this.fileStore);
      return this.fileStore[id];
    },
  },

  watch: {
    authKey() {
      this.wrongKey = false;
    },
    entries: {
      handler() {
        if(this.updateDebouncer !== null) {
          clearInterval(this.updateDebouncer);
        }

        this.updateDebouncer = setInterval(async () => {
          if(this.connectionDown) return;
          clearInterval(this.updateDebouncer);
          this.updateDebouncer = null;
          await this.syncUp();
          // TODO: notification
        }, DEBOUNCE_TIMEOUT);
      },
      deep: true,
    },
  },
};

async function setup() {
  // Fetch query
  const resp = await fetch('/config');
  const config = await resp.json();
  CONFIG = config;

  document.body.style.display = 'block';

  // Bootstrap app
  const app = new Vue(desc);
}
