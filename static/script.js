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

let conn;

function buildWsURI(key) {
  if (location.protocol === 'https:') {
      return `wss://${location.hostname}:38265/${key}`;
  } else {
      return `ws://${location.hostname}:38265/${key}`;
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

async function uploadFile(dt) {
  const segs = dt.name.split('.');
  const ext = segs[segs.length-1];

  const createResult = await sendWait({ cmd: 'upload', ext });
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
    wrongKey: false,
    authKey: '',
    entries: [],
    referenceEntries: [],
    files: null,
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

    async listFiles() {
      this.files = await sendWait({ cmd: 'files' });
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
      if([...ev.dataTransfer.types].includes('Files'))
        ev.preventDefault();
    },

    async drop(ev) {
      if([...ev.dataTransfer.types].includes('Files'))
        ev.preventDefault();
      this.dragging = 0;
      await this.upload(ev.dataTransfer.files);
    },

    async upload(list) {
      this.uploading = 0;
      for(const f of list)
        if(f.type.indexOf('image/') === 0)
          ++this.uploading;

      for(const f of list)
        if(f.type.indexOf('image/') === 0) {
          await uploadFile(f);
          --this.uploading;
        }
      // Upload finished, refresh list
      await this.listFiles();
    },

    dragEnter(ev) {
      if([...ev.dataTransfer.types].includes('Files'))
        ++this.dragging;
    },

    dragLeave(ev) {
      if([...ev.dataTransfer.types].includes('Files'))
        --this.dragging;
    },

    addFile(entry) {
      if(this.files === null) this.listFiles();
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
      if(entry.icon === files[0])
        entry.icon = null;
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
      console.log(result);
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
