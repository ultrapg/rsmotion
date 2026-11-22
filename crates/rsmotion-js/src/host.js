"use strict";
(() => {
  if (globalThis.__rsmotion_host_installed) return;
  globalThis.__rsmotion_host_installed = true;

  const ops = Deno.core.ops;
  const HTML_NS = "http://www.w3.org/1999/xhtml";
  const SVG_NS = "http://www.w3.org/2000/svg";
  const START_MS = Date.now();

  globalThis.window = globalThis;
  globalThis.self = globalThis;
  globalThis.global = globalThis;
  globalThis.remotion_isMainTab = true;

  if (typeof globalThis.crypto === "undefined") {
    globalThis.crypto = {};
  }
  if (typeof globalThis.crypto.getRandomValues !== "function") {
    let _seed = 0x9e3779b9;
    globalThis.crypto.getRandomValues = (arr) => {
      if (!arr || typeof arr.length !== "number") return arr;
      for (let i = 0; i < arr.length; i++) {
        _seed = (_seed ^ (_seed << 13)) >>> 0;
        _seed = (_seed ^ (_seed >>> 17)) >>> 0;
        _seed = (_seed ^ (_seed << 5)) >>> 0;
        arr[i] = _seed & 0xff;
      }
      return arr;
    };
  }
  if (typeof globalThis.crypto.randomUUID !== "function") {
    globalThis.crypto.randomUUID = () => {
      const b = new Uint8Array(16);
      globalThis.crypto.getRandomValues(b);
      b[6] = (b[6] & 0x0f) | 0x40;
      b[8] = (b[8] & 0x3f) | 0x80;
      const h = Array.from(b, (x) => x.toString(16).padStart(2, "0"));
      return `${h.slice(0, 4).join("")}-${h.slice(4, 6).join("")}-${h.slice(6, 8).join("")}-${h.slice(8, 10).join("")}-${h.slice(10, 16).join("")}`;
    };
  }

  // --- Console bridging ---
  const formatArgs = (args) =>
    args
      .map((a) => {
        if (typeof a === "string") return a;
        if (a instanceof Error) return a.stack || a.message || String(a);
        try {
          return JSON.stringify(a);
        } catch (_) {
          return String(a);
        }
      })
      .join(" ");

  globalThis.console = {
    log: (...a) => ops.op_console_log("info", formatArgs(a)),
    info: (...a) => ops.op_console_log("info", formatArgs(a)),
    debug: (...a) => ops.op_console_log("debug", formatArgs(a)),
    warn: (...a) => ops.op_console_log("warn", formatArgs(a)),
    error: (...a) => ops.op_console_log("error", formatArgs(a)),
  };

  // --- Base64 & UTF-8 helpers ---
  const B64_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  globalThis.btoa = (str) => {
    const s = String(str);
    let out = "";
    for (let i = 0; i < s.length; i += 3) {
      const a = s.charCodeAt(i) & 0xff;
      const b = i + 1 < s.length ? s.charCodeAt(i + 1) & 0xff : 0;
      const c = i + 2 < s.length ? s.charCodeAt(i + 2) & 0xff : 0;
      const triple = (a << 16) | (b << 8) | c;
      out += B64_CHARS[(triple >> 18) & 63];
      out += B64_CHARS[(triple >> 12) & 63];
      out += i + 1 < s.length ? B64_CHARS[(triple >> 6) & 63] : "=";
      out += i + 2 < s.length ? B64_CHARS[triple & 63] : "=";
    }
    return out;
  };
  globalThis.atob = (b64) => {
    const clean = String(b64).replace(/[\s=]+$/g, "");
    let out = "";
    for (let i = 0; i < clean.length; i += 4) {
      const a = B64_CHARS.indexOf(clean[i]);
      const b = B64_CHARS.indexOf(clean[i + 1]);
      const c = i + 2 < clean.length ? B64_CHARS.indexOf(clean[i + 2]) : 0;
      const d = i + 3 < clean.length ? B64_CHARS.indexOf(clean[i + 3]) : 0;
      const triple = (a << 18) | (b << 12) | (c << 6) | d;
      out += String.fromCharCode((triple >> 16) & 0xff);
      if (i + 2 < clean.length) out += String.fromCharCode((triple >> 8) & 0xff);
      if (i + 3 < clean.length) out += String.fromCharCode(triple & 0xff);
    }
    return out;
  };

  if (typeof globalThis.TextEncoder === "undefined") {
    globalThis.TextEncoder = class TextEncoder {
      encode(str = "") {
        const bytes = [];
        for (let i = 0; i < str.length; i++) {
          let cp = str.codePointAt(i);
          if (cp > 0xffff) i++;
          if (cp < 0x80) {
            bytes.push(cp);
          } else if (cp < 0x800) {
            bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
          } else if (cp < 0x10000) {
            bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
          } else {
            bytes.push(
              0xf0 | (cp >> 18),
              0x80 | ((cp >> 12) & 0x3f),
              0x80 | ((cp >> 6) & 0x3f),
              0x80 | (cp & 0x3f),
            );
          }
        }
        return new Uint8Array(bytes);
      }
    };
  }

  if (typeof globalThis.TextDecoder === "undefined") {
    globalThis.TextDecoder = class TextDecoder {
      decode(buf = new Uint8Array()) {
        const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
        let out = "";
        for (let i = 0; i < bytes.length; ) {
          const b0 = bytes[i];
          if (b0 < 0x80) {
            out += String.fromCharCode(b0);
            i += 1;
          } else if ((b0 & 0xe0) === 0xc0) {
            out += String.fromCharCode(((b0 & 0x1f) << 6) | (bytes[i + 1] & 0x3f));
            i += 2;
          } else if ((b0 & 0xf0) === 0xe0) {
            out += String.fromCharCode(
              ((b0 & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f),
            );
            i += 3;
          } else {
            const cp =
              ((b0 & 0x07) << 18) |
              ((bytes[i + 1] & 0x3f) << 12) |
              ((bytes[i + 2] & 0x3f) << 6) |
              (bytes[i + 3] & 0x3f);
            out += String.fromCodePoint(cp);
            i += 4;
          }
        }
        return out;
      }
    };
  }

  // --- URL & URLSearchParams ---
  class RSMURLSearchParams {
    constructor(init = "") {
      this._entries = [];
      if (typeof init === "string") {
        const s = init.startsWith("?") ? init.slice(1) : init;
        if (s) {
          for (const part of s.split("&")) {
            if (!part) continue;
            const eq = part.indexOf("=");
            const k = decodeURIComponent((eq >= 0 ? part.slice(0, eq) : part).replace(/\+/g, " "));
            const v = eq >= 0 ? decodeURIComponent(part.slice(eq + 1).replace(/\+/g, " ")) : "";
            this._entries.push([k, v]);
          }
        }
      } else if (init && typeof init === "object") {
        for (const [k, v] of Object.entries(init)) {
          this._entries.push([String(k), String(v)]);
        }
      }
    }
    append(k, v) { this._entries.push([String(k), String(v)]); }
    delete(k) { this._entries = this._entries.filter(([ek]) => ek !== String(k)); }
    get(k) { const f = this._entries.find(([ek]) => ek === String(k)); return f ? f[1] : null; }
    getAll(k) { return this._entries.filter(([ek]) => ek === String(k)).map(([, v]) => v); }
    has(k) { return this._entries.some(([ek]) => ek === String(k)); }
    set(k, v) {
      this.delete(k);
      this._entries.push([String(k), String(v)]);
    }
    entries() { return this._entries[Symbol.iterator](); }
    keys() { return this._entries.map(([k]) => k)[Symbol.iterator](); }
    values() { return this._entries.map(([, v]) => v)[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
    forEach(cb, thisArg) {
      for (const [k, v] of this._entries) cb.call(thisArg, v, k, this);
    }
    toString() {
      return this._entries
        .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
        .join("&");
    }
  }

  function normalizePathSegments(path) {
    const parts = path.split("/");
    const stack = [];
    for (let i = 0; i < parts.length; i++) {
      const seg = parts[i];
      if (seg === "" && i === 0) continue;
      if (seg === ".") continue;
      if (seg === "..") {
        if (stack.length > 0) stack.pop();
        continue;
      }
      if (seg !== "" || i === parts.length - 1) stack.push(seg);
    }
    return "/" + stack.join("/");
  }

  class RSMURL {
    constructor(input, base) {
      const raw = String(input);
      const schemeMatch = raw.match(/^([a-zA-Z][a-zA-Z0-9+.-]*):(.*)$/);
      let resolved = raw;

      if (!schemeMatch) {
        const baseObj = base instanceof RSMURL ? base : new RSMURL(base || "http://127.0.0.1/");
        if (raw.startsWith("//")) {
          resolved = `${baseObj.protocol}${raw}`;
        } else if (raw.startsWith("#")) {
          resolved = `${baseObj.origin}${baseObj.pathname}${baseObj.search}${raw}`;
        } else if (raw.startsWith("?")) {
          resolved = `${baseObj.origin}${baseObj.pathname}${raw}`;
        } else if (raw.startsWith("/")) {
          resolved = `${baseObj.origin}${raw}`;
        } else {
          const baseDir = baseObj.pathname.endsWith("/")
            ? baseObj.pathname
            : baseObj.pathname.slice(0, baseObj.pathname.lastIndexOf("/") + 1);
          const qIdx = raw.indexOf("?");
          const hIdx = raw.indexOf("#");
          const cut = [qIdx, hIdx].filter((x) => x >= 0).sort((a, b) => a - b)[0] ?? raw.length;
          const relPath = raw.slice(0, cut);
          const tail = raw.slice(cut);
          resolved = `${baseObj.origin}${normalizePathSegments(baseDir + relPath)}${tail}`;
        }
      }

      const m = resolved.match(/^([a-zA-Z][a-zA-Z0-9+.-]*):(?:\/\/([^/?#]*))?([^?#]*)(\?[^#]*)?(#.*)?$/);
      if (!m) {
        throw new TypeError(`Invalid URL: ${raw}`);
      }
      this.protocol = `${m[1].toLowerCase()}:`;
      const authority = m[2] || "";
      const portIdx = authority.lastIndexOf(":");
      if (portIdx >= 0) {
        this.hostname = authority.slice(0, portIdx);
        this.port = authority.slice(portIdx + 1);
      } else {
        this.hostname = authority;
        this.port = "";
      }
      this.host = authority;
      this.origin = authority ? `${this.protocol}//${authority}` : "null";
      this.pathname = m[3] ? (authority ? normalizePathSegments(m[3]) : m[3]) : authority ? "/" : "";
      this.search = m[4] || "";
      this.hash = m[5] || "";
      this.searchParams = new RSMURLSearchParams(this.search);
    }
    get href() {
      const auth = this.host ? `//${this.host}` : "";
      const q = this.searchParams.toString();
      const search = q ? `?${q}` : this.search;
      return `${this.protocol}${auth}${this.pathname}${search}${this.hash}`;
    }
    toString() { return this.href; }
    toJSON() { return this.href; }
  }

  let __nextBlobId = 1;
  const __blobRegistry = new Map();
  const __blobVideoMeta = new Map();
  const VIDEO_FRAME_MIME = "application/x-rsmotion-video-frame";

  function concatBlobParts(parts = []) {
    const enc = new TextEncoder();
    const chunks = [];
    let total = 0;
    for (const p of parts) {
      let arr;
      if (p instanceof ArrayBuffer) arr = new Uint8Array(p);
      else if (ArrayBuffer.isView(p)) arr = new Uint8Array(p.buffer, p.byteOffset, p.byteLength);
      else if (p && p._bytes instanceof Uint8Array) arr = p._bytes;
      else arr = enc.encode(String(p));
      chunks.push(arr);
      total += arr.byteLength;
    }
    const out = new Uint8Array(total);
    let off = 0;
    for (const c of chunks) {
      out.set(c, off);
      off += c.byteLength;
    }
    return out;
  }

  class RSMBlob {
    constructor(parts = [], options = {}) {
      this.type = options && options.type ? String(options.type) : "";
      this._bytes = concatBlobParts(parts);
    }
    get size() { return this._bytes.byteLength; }
    arrayBuffer() {
      const copy = new Uint8Array(this._bytes.byteLength);
      copy.set(this._bytes);
      return Promise.resolve(copy.buffer);
    }
    text() {
      return Promise.resolve(new TextDecoder().decode(this._bytes));
    }
  }
  globalThis.Blob = RSMBlob;

  RSMURL.createObjectURL = (blob) => {
    const url = `blob:http://127.0.0.1/${__nextBlobId++}`;
    __blobRegistry.set(url, blob);
    if (blob && blob.type === VIDEO_FRAME_MIME && blob._bytes instanceof Uint8Array) {
      try {
        const meta = JSON.parse(new TextDecoder().decode(blob._bytes));
        __blobVideoMeta.set(url, meta);
      } catch (_) {}
    }
    return url;
  };
  RSMURL.revokeObjectURL = (url) => {
    const key = String(url);
    __blobRegistry.delete(key);
    __blobVideoMeta.delete(key);
  };

  globalThis.URL = RSMURL;
  globalThis.URLSearchParams = RSMURLSearchParams;

  // --- Events & Targets ---
  class Event {
    constructor(type, init = {}) {
      this.type = String(type);
      this.bubbles = Boolean(init.bubbles);
      this.cancelable = Boolean(init.cancelable);
      this.defaultPrevented = false;
      this.target = null;
      this.currentTarget = null;
      this.timeStamp = Date.now();
    }
    preventDefault() { this.defaultPrevented = true; }
    stopPropagation() {}
    stopImmediatePropagation() {}
  }
  class CustomEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      this.detail = init.detail !== undefined ? init.detail : null;
    }
  }
  class ErrorEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      this.message = init.message || "";
      this.filename = init.filename || "";
      this.lineno = init.lineno || 0;
      this.colno = init.colno || 0;
      this.error = init.error;
    }
  }
  class EventTarget {
    constructor() {
      this._listeners = new Map();
    }
    addEventListener(type, cb, options) {
      if (typeof cb !== "function") return;
      let list = this._listeners.get(type);
      if (!list) {
        list = [];
        this._listeners.set(type, list);
      }
      const once = Boolean(typeof options === "object" && options !== null && options.once);
      if (!list.some((item) => item.cb === cb)) {
        list.push({ cb, once });
      }
    }
    removeEventListener(type, cb) {
      const list = this._listeners.get(type);
      if (!list) return;
      const idx = list.findIndex((item) => item.cb === cb);
      if (idx >= 0) list.splice(idx, 1);
    }
    dispatchEvent(evt) {
      if (!evt || typeof evt !== "object") return true;
      evt.target = evt.target || this;
      evt.currentTarget = this;
      const list = this._listeners.get(evt.type);
      if (list && list.length > 0) {
        const snapshot = [...list];
        for (const item of snapshot) {
          if (item.once) {
            const idx = list.indexOf(item);
            if (idx >= 0) list.splice(idx, 1);
          }
          item.cb.call(this, evt);
        }
      }
      return !evt.defaultPrevented;
    }
  }

  globalThis.Event = Event;
  globalThis.CustomEvent = CustomEvent;
  globalThis.ErrorEvent = ErrorEvent;
  globalThis.EventTarget = EventTarget;

  class AbortSignal extends EventTarget {
    constructor() {
      super();
      this.aborted = false;
      this.reason = undefined;
      this.onabort = null;
    }
    throwIfAborted() {
      if (this.aborted) throw this.reason;
    }
    static abort(reason) {
      const s = new AbortSignal();
      s.aborted = true;
      s.reason = reason !== undefined ? reason : new Error("Aborted");
      return s;
    }
  }
  class AbortController {
    constructor() {
      this.signal = new AbortSignal();
    }
    abort(reason) {
      if (this.signal.aborted) return;
      this.signal.aborted = true;
      this.signal.reason = reason !== undefined ? reason : new Error("Aborted");
      const evt = new Event("abort");
      if (typeof this.signal.onabort === "function") this.signal.onabort(evt);
      this.signal.dispatchEvent(evt);
    }
  }
  globalThis.AbortSignal = AbortSignal;
  globalThis.AbortController = AbortController;

  class BroadcastChannel extends EventTarget {
    constructor(name) {
      super();
      this.name = String(name);
      this.onmessage = null;
    }
    postMessage() {}
    close() {}
  }
  globalThis.BroadcastChannel = BroadcastChannel;

  // --- Deterministic Task / Timer / rAF Scheduler ---
  let __nextTimerId = 1;
  const __immediateQueue = [];
  const __rafQueue = [];
  const __longTimers = new Map();

  globalThis.setTimeout = (fn, delay = 0, ...args) => {
    if (typeof fn !== "function") return 0;
    const id = __nextTimerId++;
    const ms = Number(delay) || 0;
    if (ms <= 50) {
      __immediateQueue.push({ id, fn, args, cancelled: false });
    } else {
      __longTimers.set(id, { id, fn, args, dueAt: Date.now() + ms, interval: null });
    }
    return id;
  };

  globalThis.clearTimeout = (id) => {
    __longTimers.delete(id);
    for (const item of __immediateQueue) {
      if (item.id === id) item.cancelled = true;
    }
  };

  globalThis.setInterval = (fn, delay = 0, ...args) => {
    if (typeof fn !== "function") return 0;
    const id = __nextTimerId++;
    const ms = Math.max(16, Number(delay) || 16);
    __longTimers.set(id, { id, fn, args, dueAt: Date.now() + ms, interval: ms });
    return id;
  };

  globalThis.clearInterval = (id) => {
    globalThis.clearTimeout(id);
  };

  globalThis.requestAnimationFrame = (cb) => {
    if (typeof cb !== "function") return 0;
    const id = __nextTimerId++;
    __rafQueue.push({ id, cb, cancelled: false });
    return id;
  };

  globalThis.cancelAnimationFrame = (id) => {
    for (const item of __rafQueue) {
      if (item.id === id) item.cancelled = true;
    }
  };

  class MessageChannel {
    constructor() {
      const p1 = { onmessage: null, postMessage: (data) => {
        if (typeof p2.onmessage === "function") {
          const id = __nextTimerId++;
          __immediateQueue.push({ id, fn: p2.onmessage, args: [{ data }], cancelled: false });
        }
      }};
      const p2 = { onmessage: null, postMessage: (data) => {
        if (typeof p1.onmessage === "function") {
          const id = __nextTimerId++;
          __immediateQueue.push({ id, fn: p1.onmessage, args: [{ data }], cancelled: false });
        }
      }};
      this.port1 = p1;
      this.port2 = p2;
    }
  }
  globalThis.MessageChannel = MessageChannel;

  if (typeof globalThis.queueMicrotask !== "function") {
    globalThis.queueMicrotask = (fn) => Promise.resolve().then(fn);
  }

  globalThis.__rsmotion_drain_tick = () => {
    let didWork = false;
    const immCount = __immediateQueue.length;
    if (immCount > 0) {
      const batch = __immediateQueue.splice(0, immCount);
      for (const t of batch) {
        if (!t.cancelled) {
          didWork = true;
          t.fn(...t.args);
        }
      }
    }
    const rafCount = __rafQueue.length;
    if (rafCount > 0) {
      const batch = __rafQueue.splice(0, rafCount);
      const ts = Date.now() - START_MS;
      for (const item of batch) {
        if (!item.cancelled) {
          didWork = true;
          item.cb(ts);
        }
      }
    }
    const now = Date.now();
    for (const [id, t] of [...__longTimers.entries()]) {
      if (now >= t.dueAt) {
        didWork = true;
        if (t.interval !== null) {
          t.dueAt = now + t.interval;
        } else {
          __longTimers.delete(id);
        }
        t.fn(...t.args);
      }
    }
    return didWork;
  };

  globalThis.__rsmotion_has_pending_short_work = () =>
    __immediateQueue.some((i) => !i.cancelled) || __rafQueue.some((i) => !i.cancelled);

  // --- FontFace & document.fonts ---
  function extractFontUrl(source) {
    const s = String(source || "");
    const m = s.match(/url\s*\(\s*(['"]?)(.*?)\1\s*\)/i);
    return m ? m[2] : s.trim();
  }

  class FontFace {
    constructor(family, source, descriptors = {}) {
      this.family = String(family).replace(/^['"]|['"]$/g, "");
      this._bytes = null;
      if (source instanceof ArrayBuffer) {
        this._bytes = new Uint8Array(source);
        this.source = "arraybuffer";
        this.url = "";
      } else if (typeof ArrayBuffer !== "undefined" && ArrayBuffer.isView(source)) {
        this._bytes = new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
        this.source = "arraybuffer";
        this.url = "";
      } else {
        this.source = String(source);
        this.url = extractFontUrl(this.source);
      }
      this.weight = descriptors && descriptors.weight ? String(descriptors.weight) : "400";
      this.style = descriptors && descriptors.style ? String(descriptors.style) : "normal";
      this.unicodeRange = descriptors && descriptors.unicodeRange ? String(descriptors.unicodeRange) : "";
      this.status = "unloaded";
      this.loaded = Promise.resolve(this);
    }
    load() {
      this.status = "loading";
      try {
        if (this._bytes) {
          ops.op_register_font_bytes(this.family, this._bytes, this.weight, this.style, this.unicodeRange);
        } else {
          ops.op_register_font(this.family, this.url, this.weight, this.style, this.unicodeRange);
        }
        this.status = "loaded";
        this.loaded = Promise.resolve(this);
        return this.loaded;
      } catch (err) {
        this.status = "error";
        this.loaded = Promise.reject(err);
        return this.loaded;
      }
    }
  }
  globalThis.FontFace = FontFace;

  const fontSet = new Set();
  const documentFonts = {
    add(face) {
      fontSet.add(face);
      if (face && face.status !== "loaded" && typeof face.load === "function") {
        face.load();
      }
      return this;
    },
    delete(face) { return fontSet.delete(face); },
    has(face) { return fontSet.has(face); },
    clear() { fontSet.clear(); },
    forEach(cb, thisArg) { fontSet.forEach((f) => cb.call(thisArg, f, f, this)); },
    get status() { return "loaded"; },
    get ready() { return Promise.resolve(documentFonts); },
  };

  // --- CSSStyleDeclaration Proxy ---
  const CSS_NUM_PX = new Set([
    "top", "right", "bottom", "left", "width", "height", "min-width", "min-height",
    "max-width", "max-height", "font-size", "letter-spacing", "perspective",
    "border-radius", "border-top-left-radius", "border-top-right-radius",
    "border-bottom-right-radius", "border-bottom-left-radius",
    "border-width", "border-top-width", "border-right-width", "border-bottom-width",
    "border-left-width", "gap", "row-gap", "column-gap", "padding", "padding-top",
    "padding-right", "padding-bottom", "padding-left", "margin", "margin-top",
    "margin-right", "margin-bottom", "margin-left",
  ]);

  function toKebabCase(prop) {
    if (prop.startsWith("--")) return prop;
    if (prop.startsWith("webkit") && prop.length > 6 && prop[6] === prop[6].toUpperCase()) {
      prop = "W" + prop.slice(1);
    }
    return prop.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase());
  }

  function createStyleObject() {
    const map = new Map();
    const target = {
      _map: map,
      getPropertyValue(prop) {
        return map.get(toKebabCase(String(prop))) || "";
      },
      setProperty(prop, value) {
        const k = toKebabCase(String(prop));
        if (value === null || value === undefined || value === "" || value === false) {
          map.delete(k);
          return;
        }
        let v = String(value);
        if (typeof value === "number" && CSS_NUM_PX.has(k) && value !== 0) {
          v = `${value}px`;
        }
        map.delete(k);
        map.set(k, v);
      },
      removeProperty(prop) {
        const k = toKebabCase(String(prop));
        const prev = map.get(k) || "";
        map.delete(k);
        return prev;
      },
      get cssText() {
        const parts = [];
        for (const [k, v] of map.entries()) parts.push(`${k}: ${v};`);
        return parts.join(" ");
      },
      set cssText(val) {
        map.clear();
        const s = String(val || "");
        for (const decl of s.split(";")) {
          const idx = decl.indexOf(":");
          if (idx <= 0) continue;
          const k = decl.slice(0, idx).trim();
          const v = decl.slice(idx + 1).trim();
          if (k && v) map.set(toKebabCase(k), v);
        }
      },
      get length() { return map.size; },
    };

    return new Proxy(target, {
      get(obj, prop) {
        if (prop in obj) {
          const v = obj[prop];
          return typeof v === "function" ? v.bind(obj) : v;
        }
        if (typeof prop === "string") {
          return obj._map.get(toKebabCase(prop)) || "";
        }
        return undefined;
      },
      set(obj, prop, value) {
        if (prop === "cssText") {
          obj.cssText = value;
          return true;
        }
        if (typeof prop === "string" && prop !== "_map") {
          obj.setProperty(prop, value);
          return true;
        }
        obj[prop] = value;
        return true;
      },
    });
  }

  // --- Virtual DOM Nodes ---
  const _winListeners = new EventTarget();
  globalThis.addEventListener = (t, cb) => _winListeners.addEventListener(t, cb);
  globalThis.removeEventListener = (t, cb) => _winListeners.removeEventListener(t, cb);
  globalThis.dispatchEvent = (e) => _winListeners.dispatchEvent(e);

  let docRoot = null;

  class VNode extends EventTarget {
    constructor(nodeType, nodeName) {
      super();
      this.nodeType = nodeType;
      this.nodeName = nodeName;
      this.parentNode = null;
      this.childNodes = [];
      this.ownerDocument = null;
    }
    get parentElement() {
      return this.parentNode && this.parentNode.nodeType === 1 ? this.parentNode : null;
    }
    get firstChild() { return this.childNodes[0] || null; }
    get lastChild() { return this.childNodes[this.childNodes.length - 1] || null; }
    get nextSibling() {
      if (!this.parentNode) return null;
      const idx = this.parentNode.childNodes.indexOf(this);
      return idx >= 0 && idx + 1 < this.parentNode.childNodes.length
        ? this.parentNode.childNodes[idx + 1]
        : null;
    }
    get previousSibling() {
      if (!this.parentNode) return null;
      const idx = this.parentNode.childNodes.indexOf(this);
      return idx > 0 ? this.parentNode.childNodes[idx - 1] : null;
    }
    get children() {
      return this.childNodes.filter((c) => c.nodeType === 1);
    }
    get isConnected() {
      let cur = this;
      while (cur) {
        if (cur === docRoot) return true;
        cur = cur.parentNode;
      }
      return false;
    }
    hasChildNodes() { return this.childNodes.length > 0; }
    contains(other) {
      let cur = other;
      while (cur) {
        if (cur === this) return true;
        cur = cur.parentNode;
      }
      return false;
    }
    appendChild(child) {
      return this.insertBefore(child, null);
    }
    _isConnectedToDoc() {
      let cur = this;
      while (cur) {
        if (cur === docRoot) return true;
        cur = cur.parentNode;
      }
      return false;
    }
    insertBefore(newChild, refChild) {
      if (!newChild) return newChild;
      if (newChild.nodeType === 11) {
        for (const ch of [...newChild.childNodes]) {
          this.insertBefore(ch, refChild);
        }
        return newChild;
      }
      if (newChild.parentNode) {
        newChild.parentNode.removeChild(newChild);
      }
      newChild.parentNode = this;
      newChild.ownerDocument = this.ownerDocument || docRoot;
      if (refChild === null || refChild === undefined) {
        this.childNodes.push(newChild);
      } else {
        const idx = this.childNodes.indexOf(refChild);
        if (idx >= 0) {
          this.childNodes.splice(idx, 0, newChild);
        } else {
          this.childNodes.push(newChild);
        }
      }
      if (typeof HTMLScriptElement !== "undefined" && newChild instanceof HTMLScriptElement && this._isConnectedToDoc()) {
        newChild._triggerLoad();
      }
      return newChild;
    }
    removeChild(child) {
      const idx = this.childNodes.indexOf(child);
      if (idx >= 0) {
        this.childNodes.splice(idx, 1);
        child.parentNode = null;
      }
      return child;
    }
    replaceChild(newChild, oldChild) {
      this.insertBefore(newChild, oldChild);
      return this.removeChild(oldChild);
    }
    prepend(...nodes) {
      const ref = this.firstChild;
      for (const n of nodes) {
        const node = typeof n === "string" ? docRoot.createTextNode(n) : n;
        this.insertBefore(node, ref);
      }
    }
    append(...nodes) {
      for (const n of nodes) {
        const node = typeof n === "string" ? docRoot.createTextNode(n) : n;
        this.appendChild(node);
      }
    }
    remove() {
      if (this.parentNode) this.parentNode.removeChild(this);
    }
    get textContent() {
      if (this.nodeType === 3 || this.nodeType === 8) return this._data;
      return this.childNodes.map((c) => c.textContent).join("");
    }
    set textContent(v) {
      if (this.nodeType === 3 || this.nodeType === 8) {
        this._data = String(v ?? "");
        return;
      }
      for (const ch of this.childNodes) ch.parentNode = null;
      this.childNodes = [];
      const str = String(v ?? "");
      if (str.length > 0) {
        this.appendChild(docRoot.createTextNode(str));
      }
    }
    cloneNode(deep = false) {
      if (this.nodeType === 3) return docRoot.createTextNode(this._data);
      if (this.nodeType === 8) return docRoot.createComment(this._data);
      const copy = new VDocumentFragment();
      if (deep) {
        for (const ch of this.childNodes) copy.appendChild(ch.cloneNode(true));
      }
      return copy;
    }
  }

  class VText extends VNode {
    constructor(data) {
      super(3, "#text");
      this._data = String(data ?? "");
    }
    get nodeValue() { return this._data; }
    set nodeValue(v) { this._data = String(v ?? ""); }
    get data() { return this._data; }
    set data(v) { this._data = String(v ?? ""); }
  }

  class VComment extends VNode {
    constructor(data) {
      super(8, "#comment");
      this._data = String(data ?? "");
    }
    get nodeValue() { return this._data; }
    set nodeValue(v) { this._data = String(v ?? ""); }
    get data() { return this._data; }
    set data(v) { this._data = String(v ?? ""); }
  }

  class VDocumentFragment extends VNode {
    constructor() {
      super(11, "#document-fragment");
    }
  }

  class VElement extends VNode {
    constructor(tagName, namespaceURI = HTML_NS) {
      const isSvg = namespaceURI === SVG_NS;
      const local = isSvg ? String(tagName) : String(tagName).toLowerCase();
      super(1, isSvg ? local : local.toUpperCase());
      this.localName = local;
      this.namespaceURI = namespaceURI;
      this._attrs = new Map();
      this._style = createStyleObject();
      this._isSvg = isSvg;
      this.dataset = new Proxy({}, {
        get: (_, p) => this.getAttribute("data-" + toKebabCase(String(p))) ?? undefined,
        set: (_, p, v) => { this.setAttribute("data-" + toKebabCase(String(p)), v); return true; },
        deleteProperty: (_, p) => { this.removeAttribute("data-" + toKebabCase(String(p))); return true; },
      });
      this.classList = {
        add: (...cls) => {
          const cur = new Set(this.className.split(/\s+/).filter(Boolean));
          for (const c of cls) cur.add(c);
          this.className = [...cur].join(" ");
        },
        remove: (...cls) => {
          const cur = new Set(this.className.split(/\s+/).filter(Boolean));
          for (const c of cls) cur.delete(c);
          this.className = [...cur].join(" ");
        },
        contains: (c) => this.className.split(/\s+/).filter(Boolean).includes(c),
        toggle: (c) => {
          if (this.classList.contains(c)) { this.classList.remove(c); return false; }
          this.classList.add(c); return true;
        },
      };
    }
    get tagName() { return this.nodeName; }
    get style() { return this._style; }
    set style(val) { this._style.cssText = val; }
    get id() { return this.getAttribute("id") || ""; }
    set id(v) { this.setAttribute("id", v); }
    get className() { return this.getAttribute("class") || ""; }
    set className(v) {
      const s = String(v ?? "");
      if (s === "") this.removeAttribute("class");
      else this.setAttribute("class", s);
    }
    _normAttr(name) {
      const n = String(name);
      return this._isSvg ? n : n.toLowerCase();
    }
    setAttribute(name, value) {
      const k = this._normAttr(name);
      const v = String(value);
      if (k === "style" && !this._isSvg) {
        this._style.cssText = v;
        return;
      }
      this._attrs.set(k, v);
      if (k === "src") {
        if (this.localName === "img") this._probeSrc(v);
        else if (this.localName === "video" || this.localName === "audio") this._probeMediaSrc(v);
      } else if (this.localName === "canvas" && (k === "width" || k === "height")) {
        const n = Number(v);
        if (Number.isFinite(n) && n >= 0) {
          if (k === "width") this._width = n | 0;
          else this._height = n | 0;
          this._ops = [];
        }
      }
    }
    setAttributeNS(_ns, qualifiedName, value) {
      const idx = qualifiedName.indexOf(":");
      const k = this._isSvg ? qualifiedName : qualifiedName.toLowerCase();
      if (idx > 0 && k.startsWith("xlink:")) {
        this._attrs.set("href", String(value));
      }
      this.setAttribute(k, value);
    }
    getAttribute(name) {
      const k = this._normAttr(name);
      return this._attrs.has(k) ? this._attrs.get(k) : null;
    }
    hasAttribute(name) { return this._attrs.has(this._normAttr(name)); }
    removeAttribute(name) { this._attrs.delete(this._normAttr(name)); }
    removeAttributeNS(_ns, localName) { this.removeAttribute(localName); }
    set innerHTML(val) {
      for (const ch of this.childNodes) ch.parentNode = null;
      this.childNodes = [];
      if (val) this.appendChild(docRoot.createTextNode(String(val)));
    }
    get innerHTML() { return this.textContent; }
    set innerText(val) {
      this.textContent = val === null || val === undefined ? "" : String(val);
    }
    get innerText() { return this.textContent; }
    focus() {}
    blur() {}
    getBoundingClientRect() {
      const txt = this.textContent;
      if (!txt || txt.length === 0) {
        return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, x: 0, y: 0 };
      }
      const smap = this._style && this._style._map;
      const getS = (k) => (smap ? smap.get(k) || "" : "");
      const fontStyle = getS("font-style");
      const fontWeight = getS("font-weight");
      const rawSize = getS("font-size") || "16px";
      let fontSizePx = 16;
      if (rawSize.endsWith("rem") || rawSize.endsWith("em")) {
        const n = Number.parseFloat(rawSize);
        if (Number.isFinite(n)) fontSizePx = n * 16;
      } else {
        const n = Number.parseFloat(rawSize);
        if (Number.isFinite(n) && n > 0) fontSizePx = n;
      }
      const fontSizeStr = `${fontSizePx}px`;
      const fontFamily = getS("font-family") || "sans-serif";
      const tt = getS("text-transform").toLowerCase();
      let transformed = txt;
      if (tt === "uppercase") transformed = txt.toUpperCase();
      else if (tt === "lowercase") transformed = txt.toLowerCase();
      else if (tt === "capitalize") transformed = txt.replace(/\b\w/g, (c) => c.toUpperCase());

      const fontParts = [];
      if (fontStyle && fontStyle !== "normal") fontParts.push(fontStyle);
      if (fontWeight && fontWeight !== "normal") fontParts.push(fontWeight);
      fontParts.push(fontSizeStr, fontFamily);
      const fontStr = fontParts.join(" ");

      const [rawW, asc, desc] = ops.op_canvas_measure_text(transformed, fontStr);
      let w = rawW;
      const lsRaw = getS("letter-spacing");
      if (lsRaw) {
        const lsVal = Number.parseFloat(lsRaw);
        if (Number.isFinite(lsVal)) {
          const lsPx = lsRaw.endsWith("em") || lsRaw.endsWith("rem") ? lsVal * fontSizePx : lsVal;
          w = Math.max(0, w + lsPx * Math.max(0, transformed.length - 1));
        }
      }
      let lineHeightPx = 0;
      const lhRaw = getS("line-height");
      if (lhRaw && lhRaw !== "normal") {
        const lhVal = Number.parseFloat(lhRaw);
        if (Number.isFinite(lhVal) && lhVal > 0) {
          if (lhRaw.endsWith("px")) lineHeightPx = lhVal;
          else if (lhRaw.endsWith("em") || lhRaw.endsWith("rem")) lineHeightPx = lhVal * fontSizePx;
          else if (lhRaw.endsWith("%")) lineHeightPx = (lhVal / 100) * fontSizePx;
          else lineHeightPx = lhVal * fontSizePx;
        }
      }
      const h = Math.max(asc + desc, lineHeightPx || 0);
      return { top: 0, left: 0, right: w, bottom: h, width: w, height: h, x: 0, y: 0 };
    }
    cloneNode(deep = false) {
      const c = docRoot.createElementNS(this.namespaceURI, this.localName);
      for (const [k, v] of this._attrs.entries()) c._attrs.set(k, v);
      for (const [k, v] of this._style._map.entries()) c._style._map.set(k, v);
      if (deep) for (const ch of this.childNodes) c.appendChild(ch.cloneNode(true));
      return c;
    }
    getElementById(id) {
      if (this.id === id) return this;
      for (const ch of this.childNodes) {
        if (ch.nodeType === 1) {
          const hit = ch.getElementById(id);
          if (hit) return hit;
        }
      }
      return null;
    }
    getElementsByTagName(tag) {
      const want = String(tag).toLowerCase();
      const out = [];
      const walk = (n) => {
        for (const ch of n.childNodes) {
          if (ch.nodeType === 1) {
            if (want === "*" || ch.localName.toLowerCase() === want) out.push(ch);
            walk(ch);
          }
        }
      };
      walk(this);
      return out;
    }
    getElementsByClassName(cls) {
      const want = String(cls).trim();
      const out = [];
      const walk = (n) => {
        for (const ch of n.childNodes) {
          if (ch.nodeType === 1) {
            if (ch.classList.contains(want)) out.push(ch);
            walk(ch);
          }
        }
      };
      walk(this);
      return out;
    }
    querySelectorAll(selector) {
      const sel = String(selector).trim();
      const out = [];
      const match = (el) => {
        if (sel.startsWith("#")) return el.id === sel.slice(1);
        if (sel.startsWith(".")) return el.classList.contains(sel.slice(1));
        return el.localName.toLowerCase() === sel.toLowerCase();
      };
      const walk = (n) => {
        for (const ch of n.childNodes) {
          if (ch.nodeType === 1) {
            if (match(ch)) out.push(ch);
            walk(ch);
          }
        }
      };
      walk(this);
      return out;
    }
    querySelector(selector) {
      return this.querySelectorAll(selector)[0] || null;
    }
  }

  class HTMLImageElement extends VElement {
    constructor() {
      super("img", HTML_NS);
      this.naturalWidth = 0;
      this.naturalHeight = 0;
      this.complete = false;
      this.crossOrigin = null;
      this.decoding = "sync";
    }
    _probeSrc(val) {
      const s = String(val);
      const meta = __blobVideoMeta.get(s);
      if (meta) {
        this.naturalWidth = meta.width || 0;
        this.naturalHeight = meta.height || 0;
        this.complete = this.naturalWidth > 0 && this.naturalHeight > 0;
        return;
      }
      const [w, h] = ops.op_probe_image(s);
      this.naturalWidth = w || 0;
      this.naturalHeight = h || 0;
      this.complete = this.naturalWidth > 0 && this.naturalHeight > 0;
    }
    get src() { return this.getAttribute("src") || ""; }
    set src(v) {
      const val = String(v ?? "");
      if (!val) {
        this.removeAttribute("src");
        this.naturalWidth = 0;
        this.naturalHeight = 0;
        this.complete = false;
      } else {
        this._attrs.set("src", val);
        this._probeSrc(val);
      }
    }
    decode() {
      const s = this.src;
      if (s && this.naturalWidth === 0) this._probeSrc(s);
      if (this.naturalWidth > 0 && this.naturalHeight > 0) {
        return Promise.resolve();
      }
      return Promise.reject(new Error(`cannot decode image: ${s}`));
    }
  }

  function serializeCanvasPaint(p) {
    if (p && typeof p === "object" && Array.isArray(p._stops)) {
      if (p._kind === "linear") {
        return {
          kind: "linearGradient",
          x0: p._x0,
          y0: p._y0,
          x1: p._x1,
          y1: p._y1,
          stops: p._stops.map(([o, c]) => [Number(o) || 0, String(c)]),
        };
      }
      return {
        kind: "radialGradient",
        x0: p._x0,
        y0: p._y0,
        r0: p._r0,
        x1: p._x1,
        y1: p._y1,
        r1: p._r1,
        stops: p._stops.map(([o, c]) => [Number(o) || 0, String(c)]),
      };
    }
    return { kind: "color", value: String(p ?? "#000000") };
  }

  class RSMCanvasGradient {
    constructor(kind, params) {
      this._kind = kind;
      this._x0 = Number(params[0]) || 0;
      this._y0 = Number(params[1]) || 0;
      if (kind === "linear") {
        this._x1 = Number(params[2]) || 0;
        this._y1 = Number(params[3]) || 0;
        this._r0 = 0;
        this._r1 = 0;
      } else {
        this._r0 = Number(params[2]) || 0;
        this._x1 = Number(params[3]) || 0;
        this._y1 = Number(params[4]) || 0;
        this._r1 = Number(params[5]) || 0;
      }
      this._stops = [];
    }
    addColorStop(offset, color) {
      const o = Number(offset);
      if (Number.isFinite(o) && o >= 0 && o <= 1) {
        this._stops.push([o, String(color)]);
      }
    }
  }

  class RSMPath2D {
    constructor(init) {
      this._ops = [];
      if (init instanceof RSMPath2D) {
        this._ops = init._ops.slice();
      }
    }
    addPath(other) {
      if (other instanceof RSMPath2D) {
        for (const op of other._ops) this._ops.push(op);
      }
    }
    closePath() { this._ops.push({ op: "closePath" }); }
    moveTo(x, y) { this._ops.push({ op: "moveTo", x: Number(x) || 0, y: Number(y) || 0 }); }
    lineTo(x, y) { this._ops.push({ op: "lineTo", x: Number(x) || 0, y: Number(y) || 0 }); }
    bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y) {
      this._ops.push({
        op: "bezierCurveTo",
        cp1x: Number(cp1x) || 0, cp1y: Number(cp1y) || 0,
        cp2x: Number(cp2x) || 0, cp2y: Number(cp2y) || 0,
        x: Number(x) || 0, y: Number(y) || 0,
      });
    }
    quadraticCurveTo(cpx, cpy, x, y) {
      this._ops.push({
        op: "quadraticCurveTo",
        cpx: Number(cpx) || 0, cpy: Number(cpy) || 0,
        x: Number(x) || 0, y: Number(y) || 0,
      });
    }
    arc(x, y, radius, startAngle, endAngle, counterclockwise = false) {
      this._ops.push({
        op: "arc",
        x: Number(x) || 0, y: Number(y) || 0, radius: Number(radius) || 0,
        startAngle: Number(startAngle) || 0, endAngle: Number(endAngle) || 0,
        counterclockwise: Boolean(counterclockwise),
      });
    }
    arcTo(x1, y1, x2, y2, radius) {
      this._ops.push({
        op: "arcTo",
        x1: Number(x1) || 0, y1: Number(y1) || 0,
        x2: Number(x2) || 0, y2: Number(y2) || 0,
        radius: Number(radius) || 0,
      });
    }
    ellipse(x, y, radiusX, radiusY, rotation, startAngle, endAngle, counterclockwise = false) {
      this._ops.push({
        op: "ellipse",
        x: Number(x) || 0, y: Number(y) || 0,
        radiusX: Number(radiusX) || 0, radiusY: Number(radiusY) || 0,
        rotation: Number(rotation) || 0,
        startAngle: Number(startAngle) || 0, endAngle: Number(endAngle) || 0,
        counterclockwise: Boolean(counterclockwise),
      });
    }
    rect(x, y, w, h) {
      this._ops.push({ op: "rect", x: Number(x) || 0, y: Number(y) || 0, w: Number(w) || 0, h: Number(h) || 0 });
    }
    roundRect(x, y, w, h, radii = 0) {
      let arr = [0, 0, 0, 0];
      if (typeof radii === "number") {
        arr = [radii, radii, radii, radii];
      } else if (Array.isArray(radii)) {
        if (radii.length === 1) arr = [radii[0], radii[0], radii[0], radii[0]];
        else if (radii.length === 2) arr = [radii[0], radii[1], radii[0], radii[1]];
        else if (radii.length === 3) arr = [radii[0], radii[1], radii[2], radii[1]];
        else if (radii.length >= 4) arr = [radii[0], radii[1], radii[2], radii[3]];
      }
      this._ops.push({
        op: "roundRect",
        x: Number(x) || 0, y: Number(y) || 0, w: Number(w) || 0, h: Number(h) || 0,
        radii: arr.map((v) => Number(v) || 0),
      });
    }
  }
  globalThis.Path2D = RSMPath2D;

  class HTMLCanvasElement extends VElement {
    constructor() {
      super("canvas", HTML_NS);
      this._width = 300;
      this._height = 150;
      this._ops = [];
      this._ctx2d = null;
      this._unsupportedCanvasKind = null;
    }
    get width() { return this._width; }
    set width(v) {
      const n = Number(v);
      this._width = Number.isFinite(n) && n >= 0 ? (n | 0) : 300;
      this._attrs.set("width", String(this._width));
      this._ops = [];
      if (this._ctx2d) this._ctx2d._resetState();
    }
    get height() { return this._height; }
    set height(v) {
      const n = Number(v);
      this._height = Number.isFinite(n) && n >= 0 ? (n | 0) : 150;
      this._attrs.set("height", String(this._height));
      this._ops = [];
      if (this._ctx2d) this._ctx2d._resetState();
    }
    getContext(kind) {
      const k = String(kind || "2d").toLowerCase();
      if (k !== "2d") {
        this._unsupportedCanvasKind = k;
        return null;
      }
      if (this._ctx2d) return this._ctx2d;
      const canvasEl = this;
      const defaultState = () => ({
        fillStyle: "#000000",
        strokeStyle: "#000000",
        lineWidth: 1,
        lineCap: "butt",
        lineJoin: "miter",
        miterLimit: 10,
        lineDash: [],
        lineDashOffset: 0,
        globalAlpha: 1,
        globalCompositeOperation: "source-over",
        shadowBlur: 0,
        shadowColor: "rgba(0, 0, 0, 0)",
        shadowOffsetX: 0,
        shadowOffsetY: 0,
        font: "10px sans-serif",
        textAlign: "start",
        textBaseline: "alphabetic",
        filter: "none",
        dirtyMatrix: false,
        clipped: false,
      });
      let st = defaultState();
      const stack = [];
      const pushOp = (op) => canvasEl._ops.push(op);
      const extractPathArg = (arg0, arg1) => {
        if (arg0 instanceof RSMPath2D) {
          return { rule: typeof arg1 === "string" ? arg1 : "nonzero", pathOps: arg0._ops.slice() };
        }
        return { rule: typeof arg0 === "string" ? arg0 : "nonzero", pathOps: null };
      };
      const emitShadow = () => pushOp({
        op: "setShadow",
        blur: Number(st.shadowBlur) || 0,
        color: String(st.shadowColor || "rgba(0, 0, 0, 0)"),
        offsetX: Number(st.shadowOffsetX) || 0,
        offsetY: Number(st.shadowOffsetY) || 0,
      });

      const ctx = {
        canvas: canvasEl,
        _resetState() {
          st = defaultState();
          stack.length = 0;
        },
        get fillStyle() { return st.fillStyle; },
        set fillStyle(v) { st.fillStyle = v; pushOp({ op: "setFillStyle", paint: serializeCanvasPaint(v) }); },
        get strokeStyle() { return st.strokeStyle; },
        set strokeStyle(v) { st.strokeStyle = v; pushOp({ op: "setStrokeStyle", paint: serializeCanvasPaint(v) }); },
        get lineWidth() { return st.lineWidth; },
        set lineWidth(v) { const n = Number(v); if (Number.isFinite(n) && n > 0) { st.lineWidth = n; pushOp({ op: "setLineWidth", width: n }); } },
        get lineCap() { return st.lineCap; },
        set lineCap(v) { st.lineCap = String(v); pushOp({ op: "setLineCap", cap: st.lineCap }); },
        get lineJoin() { return st.lineJoin; },
        set lineJoin(v) { st.lineJoin = String(v); pushOp({ op: "setLineJoin", join: st.lineJoin }); },
        get miterLimit() { return st.miterLimit; },
        set miterLimit(v) { const n = Number(v); if (Number.isFinite(n) && n > 0) { st.miterLimit = n; pushOp({ op: "setMiterLimit", limit: n }); } },
        get lineDashOffset() { return st.lineDashOffset; },
        set lineDashOffset(v) { const n = Number(v); if (Number.isFinite(n)) { st.lineDashOffset = n; pushOp({ op: "setLineDashOffset", offset: n }); } },
        getLineDash() { return st.lineDash.slice(); },
        setLineDash(segs) {
          st.lineDash = Array.isArray(segs) ? segs.map((v) => Math.max(0, Number(v) || 0)) : [];
          pushOp({ op: "setLineDash", segments: st.lineDash.slice() });
        },
        get globalAlpha() { return st.globalAlpha; },
        set globalAlpha(v) { const n = Number(v); if (Number.isFinite(n) && n >= 0 && n <= 1) { st.globalAlpha = n; pushOp({ op: "setGlobalAlpha", alpha: n }); } },
        get globalCompositeOperation() { return st.globalCompositeOperation; },
        set globalCompositeOperation(v) { st.globalCompositeOperation = String(v); pushOp({ op: "setGlobalCompositeOperation", mode: st.globalCompositeOperation }); },
        get shadowBlur() { return st.shadowBlur; },
        set shadowBlur(v) { const n = Number(v); if (Number.isFinite(n) && n >= 0) { st.shadowBlur = n; emitShadow(); } },
        get shadowColor() { return st.shadowColor; },
        set shadowColor(v) { st.shadowColor = String(v); emitShadow(); },
        get shadowOffsetX() { return st.shadowOffsetX; },
        set shadowOffsetX(v) { const n = Number(v); if (Number.isFinite(n)) { st.shadowOffsetX = n; emitShadow(); } },
        get shadowOffsetY() { return st.shadowOffsetY; },
        set shadowOffsetY(v) { const n = Number(v); if (Number.isFinite(n)) { st.shadowOffsetY = n; emitShadow(); } },
        get font() { return st.font; },
        set font(v) { st.font = String(v); pushOp({ op: "setFont", font: st.font }); },
        get textAlign() { return st.textAlign; },
        set textAlign(v) { st.textAlign = String(v); pushOp({ op: "setTextAlign", align: st.textAlign }); },
        get textBaseline() { return st.textBaseline; },
        set textBaseline(v) { st.textBaseline = String(v); pushOp({ op: "setTextBaseline", baseline: st.textBaseline }); },
        get filter() { return st.filter; },
        set filter(v) { st.filter = String(v); pushOp({ op: "setFilter", filter: st.filter }); },
        save() {
          stack.push({ ...st, lineDash: st.lineDash.slice() });
          pushOp({ op: "save" });
        },
        restore() {
          if (stack.length > 0) st = stack.pop();
          pushOp({ op: "restore" });
        },
        translate(x, y) { st.dirtyMatrix = true; pushOp({ op: "translate", x: Number(x) || 0, y: Number(y) || 0 }); },
        rotate(a) { st.dirtyMatrix = true; pushOp({ op: "rotate", angle: Number(a) || 0 }); },
        scale(x, y) { st.dirtyMatrix = true; pushOp({ op: "scale", x: Number(x) || 0, y: Number(y) || 0 }); },
        transform(a, b, c, d, e, f) {
          st.dirtyMatrix = true;
          pushOp({ op: "transform", matrix: [Number(a) || 0, Number(b) || 0, Number(c) || 0, Number(d) || 0, Number(e) || 0, Number(f) || 0] });
        },
        setTransform(a, b, c, d, e, f) {
          if (a && typeof a === "object") {
            const m = a;
            a = m.a ?? m.m11 ?? 1; b = m.b ?? m.m12 ?? 0;
            c = m.c ?? m.m21 ?? 0; d = m.d ?? m.m22 ?? 1;
            e = m.e ?? m.m41 ?? 0; f = m.f ?? m.m42 ?? 0;
          }
          st.dirtyMatrix = true;
          pushOp({ op: "setTransform", matrix: [Number(a) || 0, Number(b) || 0, Number(c) || 0, Number(d) || 0, Number(e) || 0, Number(f) || 0] });
        },
        resetTransform() { st.dirtyMatrix = false; pushOp({ op: "resetTransform" }); },
        clearRect(x, y, w, h) {
          const nx = Number(x) || 0, ny = Number(y) || 0, nw = Number(w) || 0, nh = Number(h) || 0;
          if (!st.dirtyMatrix && !st.clipped && nx <= 0 && ny <= 0 && nw >= canvasEl._width && nh >= canvasEl._height) {
            // Preserve current non-matrix drawing styles when resetting full backing-store command buffer.
            const keepOps = [];
            if (st.fillStyle !== "#000000") keepOps.push({ op: "setFillStyle", paint: serializeCanvasPaint(st.fillStyle) });
            if (st.strokeStyle !== "#000000") keepOps.push({ op: "setStrokeStyle", paint: serializeCanvasPaint(st.strokeStyle) });
            if (st.lineWidth !== 1) keepOps.push({ op: "setLineWidth", width: st.lineWidth });
            if (st.lineCap !== "butt") keepOps.push({ op: "setLineCap", cap: st.lineCap });
            if (st.lineJoin !== "miter") keepOps.push({ op: "setLineJoin", join: st.lineJoin });
            if (st.miterLimit !== 10) keepOps.push({ op: "setMiterLimit", limit: st.miterLimit });
            if (st.lineDash.length > 0) keepOps.push({ op: "setLineDash", segments: st.lineDash.slice() });
            if (st.lineDashOffset !== 0) keepOps.push({ op: "setLineDashOffset", offset: st.lineDashOffset });
            if (st.globalAlpha !== 1) keepOps.push({ op: "setGlobalAlpha", alpha: st.globalAlpha });
            if (st.globalCompositeOperation !== "source-over") keepOps.push({ op: "setGlobalCompositeOperation", mode: st.globalCompositeOperation });
            if (st.shadowBlur > 0 || st.shadowOffsetX !== 0 || st.shadowOffsetY !== 0) {
              keepOps.push({ op: "setShadow", blur: st.shadowBlur, color: st.shadowColor, offsetX: st.shadowOffsetX, offsetY: st.shadowOffsetY });
            }
            if (st.font !== "10px sans-serif") keepOps.push({ op: "setFont", font: st.font });
            if (st.textAlign !== "start") keepOps.push({ op: "setTextAlign", align: st.textAlign });
            if (st.textBaseline !== "alphabetic") keepOps.push({ op: "setTextBaseline", baseline: st.textBaseline });
            if (st.filter !== "none") keepOps.push({ op: "setFilter", filter: st.filter });
            canvasEl._ops = keepOps;
            return;
          }
          pushOp({ op: "clearRect", x: nx, y: ny, w: nw, h: nh });
        },
        fillRect(x, y, w, h) { pushOp({ op: "fillRect", x: Number(x) || 0, y: Number(y) || 0, w: Number(w) || 0, h: Number(h) || 0 }); },
        strokeRect(x, y, w, h) { pushOp({ op: "strokeRect", x: Number(x) || 0, y: Number(y) || 0, w: Number(w) || 0, h: Number(h) || 0 }); },
        beginPath() { pushOp({ op: "beginPath" }); },
        closePath() { pushOp({ op: "closePath" }); },
        moveTo(x, y) { pushOp({ op: "moveTo", x: Number(x) || 0, y: Number(y) || 0 }); },
        lineTo(x, y) { pushOp({ op: "lineTo", x: Number(x) || 0, y: Number(y) || 0 }); },
        bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y) {
          pushOp({
            op: "bezierCurveTo",
            cp1x: Number(cp1x) || 0, cp1y: Number(cp1y) || 0,
            cp2x: Number(cp2x) || 0, cp2y: Number(cp2y) || 0,
            x: Number(x) || 0, y: Number(y) || 0,
          });
        },
        quadraticCurveTo(cpx, cpy, x, y) {
          pushOp({ op: "quadraticCurveTo", cpx: Number(cpx) || 0, cpy: Number(cpy) || 0, x: Number(x) || 0, y: Number(y) || 0 });
        },
        arc(x, y, radius, startAngle, endAngle, counterclockwise = false) {
          pushOp({
            op: "arc",
            x: Number(x) || 0, y: Number(y) || 0, radius: Number(radius) || 0,
            startAngle: Number(startAngle) || 0, endAngle: Number(endAngle) || 0,
            counterclockwise: Boolean(counterclockwise),
          });
        },
        arcTo(x1, y1, x2, y2, radius) {
          pushOp({ op: "arcTo", x1: Number(x1) || 0, y1: Number(y1) || 0, x2: Number(x2) || 0, y2: Number(y2) || 0, radius: Number(radius) || 0 });
        },
        ellipse(x, y, radiusX, radiusY, rotation, startAngle, endAngle, counterclockwise = false) {
          pushOp({
            op: "ellipse",
            x: Number(x) || 0, y: Number(y) || 0,
            radiusX: Number(radiusX) || 0, radiusY: Number(radiusY) || 0,
            rotation: Number(rotation) || 0,
            startAngle: Number(startAngle) || 0, endAngle: Number(endAngle) || 0,
            counterclockwise: Boolean(counterclockwise),
          });
        },
        rect(x, y, w, h) { pushOp({ op: "rect", x: Number(x) || 0, y: Number(y) || 0, w: Number(w) || 0, h: Number(h) || 0 }); },
        roundRect(x, y, w, h, radii = 0) {
          const p = new RSMPath2D();
          p.roundRect(x, y, w, h, radii);
          pushOp(p._ops[0]);
        },
        fill(arg0, arg1) {
          const { rule, pathOps } = extractPathArg(arg0, arg1);
          pushOp({ op: "fill", rule, pathOps });
        },
        stroke(arg0) {
          const pathOps = arg0 instanceof RSMPath2D ? arg0._ops.slice() : null;
          pushOp({ op: "stroke", pathOps });
        },
        clip(arg0, arg1) {
          st.clipped = true;
          const { rule, pathOps } = extractPathArg(arg0, arg1);
          pushOp({ op: "clip", rule, pathOps });
        },
        fillText(text, x, y, maxWidth) {
          pushOp({
            op: "fillText",
            text: String(text ?? ""),
            x: Number(x) || 0,
            y: Number(y) || 0,
            maxWidth: maxWidth !== undefined ? Number(maxWidth) : null,
          });
        },
        strokeText(text, x, y, maxWidth) {
          pushOp({
            op: "strokeText",
            text: String(text ?? ""),
            x: Number(x) || 0,
            y: Number(y) || 0,
            maxWidth: maxWidth !== undefined ? Number(maxWidth) : null,
          });
        },
        drawImage(img, ...args) {
          const src = img && typeof img.src === "string" ? img.src : "";
          if (!src) return;
          if (args.length === 2) {
            pushOp({ op: "drawImage", src, dx: Number(args[0]) || 0, dy: Number(args[1]) || 0 });
          } else if (args.length === 4) {
            pushOp({
              op: "drawImage",
              src,
              dx: Number(args[0]) || 0, dy: Number(args[1]) || 0,
              dw: Number(args[2]) || 0, dh: Number(args[3]) || 0,
            });
          } else if (args.length >= 8) {
            pushOp({
              op: "drawImage",
              src,
              sx: Number(args[0]) || 0, sy: Number(args[1]) || 0,
              sw: Number(args[2]) || 0, sh: Number(args[3]) || 0,
              dx: Number(args[4]) || 0, dy: Number(args[5]) || 0,
              dw: Number(args[6]) || 0, dh: Number(args[7]) || 0,
            });
          }
        },
        measureText(t) {
          const [w, asc, desc] = ops.op_canvas_measure_text(String(t ?? ""), st.font);
          return {
            width: w,
            actualBoundingBoxLeft: 0,
            actualBoundingBoxRight: w,
            actualBoundingBoxAscent: asc,
            actualBoundingBoxDescent: desc,
            fontBoundingBoxAscent: asc,
            fontBoundingBoxDescent: desc,
          };
        },
        getImageData(_x, _y, w, h) { return { data: new Uint8ClampedArray(((w | 0) * (h | 0) * 4) || 0) }; },
        putImageData() {},
        createLinearGradient(x0, y0, x1, y1) { return new RSMCanvasGradient("linear", [x0, y0, x1, y1]); },
        createRadialGradient(x0, y0, r0, x1, y1, r1) { return new RSMCanvasGradient("radial", [x0, y0, r0, x1, y1, r1]); },
      };
      this._ctx2d = ctx;
      return ctx;
    }
    toDataURL() { return "data:image/png;base64,"; }
  }

  class HTMLMediaElement extends VElement {
    constructor(tag) {
      super(tag, HTML_NS);
      this.paused = true;
      this.ended = false;
      this._currentTime = 0;
      this.duration = 0;
      this.videoWidth = 0;
      this.videoHeight = 0;
      this.readyState = 0;
      this.playbackRate = 1;
      this.volume = 1;
      this.muted = false;
      this._rvfcId = 1;
      this._rvfcMap = new Map();
      this._seekScheduled = false;
    }
    _probeMediaSrc(val) {
      const s = String(val || "");
      if (!s) {
        this.videoWidth = 0;
        this.videoHeight = 0;
        this.duration = 0;
        this.readyState = 0;
        return;
      }
      const [w, h, dur, hasVideo, hasAudio] = ops.op_probe_media(s);
      this.videoWidth = w || 0;
      this.videoHeight = h || 0;
      this.duration = dur || 0;
      this.readyState = (hasVideo || hasAudio) ? 4 : 0;
      if (hasVideo || hasAudio) {
        queueMicrotask(() => {
          this.dispatchEvent(new Event("loadedmetadata"));
          this.dispatchEvent(new Event("loadeddata"));
        });
      }
    }
    get src() { return this.getAttribute("src") || ""; }
    set src(v) {
      const val = String(v ?? "");
      if (!val) {
        this.removeAttribute("src");
        this._probeMediaSrc("");
      } else {
        this._attrs.set("src", val);
        this._probeMediaSrc(val);
      }
    }
    get currentTime() { return this._currentTime; }
    set currentTime(t) {
      const next = Number(t);
      this._currentTime = Number.isFinite(next) ? Math.max(0, next) : 0;
      this.ended = this.duration > 0 && this._currentTime >= this.duration;
      if (!this._seekScheduled) {
        this._seekScheduled = true;
        queueMicrotask(() => {
          this._seekScheduled = false;
          const now = Date.now() - START_MS;
          if (this._rvfcMap.size > 0) {
            const entries = [...this._rvfcMap.values()];
            this._rvfcMap.clear();
            for (const cb of entries) {
              cb(now, { mediaTime: this._currentTime, expectedDisplayTime: now });
            }
          }
          this.dispatchEvent(new Event("seeked"));
        });
      }
    }
    requestVideoFrameCallback(cb) {
      if (typeof cb !== "function") return 0;
      const id = this._rvfcId++;
      this._rvfcMap.set(id, cb);
      return id;
    }
    cancelVideoFrameCallback(id) {
      this._rvfcMap.delete(id);
    }
    play() { return Promise.resolve(); }
    pause() {}
    load() {}
    canPlayType() { return "probably"; }
  }

  class HTMLIFrameElement extends VElement {
    constructor() { super("iframe", HTML_NS); }
  }
  class HTMLScriptElement extends VElement {
    constructor() {
      super("script", HTML_NS);
      this.async = true;
      this.defer = false;
      this.type = "";
      this.charset = "";
      this.crossOrigin = null;
      this.onload = null;
      this.onerror = null;
      this._rsmotionExecuted = false;
    }
    get src() {
      const raw = this.getAttribute("src") || "";
      if (!raw) return "";
      try {
        return new RSMURL(raw, "http://127.0.0.1/").href;
      } catch (_) {
        return raw;
      }
    }
    set src(v) {
      this._attrs.set("src", String(v ?? ""));
    }
    _triggerLoad() {
      if (this._rsmotionExecuted) return;
      const target = this.src;
      if (!target) return;
      this._rsmotionExecuted = true;
      setTimeout(() => {
        const code = ops.op_read_script_asset(target);
        let ok = false;
        if (typeof code === "string") {
          const prev = docRoot.currentScript;
          docRoot.currentScript = this;
          try {
            (0, eval)(`${code}\n//# sourceURL=${target}`);
            ok = true;
          } catch (err) {
            console.error("HTMLScriptElement chunk eval failed:", target, err);
            ok = false;
          } finally {
            docRoot.currentScript = prev;
          }
        }
        if (ok) {
          const evt = new Event("load");
          evt.target = this;
          if (typeof this.onload === "function") this.onload(evt);
          this.dispatchEvent(evt);
        } else {
          const evt = new Event("error");
          evt.target = this;
          if (typeof this.onerror === "function") this.onerror(evt);
          this.dispatchEvent(evt);
        }
      }, 0);
    }
  }
  class SVGElement extends VElement {
    constructor(tag) { super(tag || "svg", SVG_NS); }
  }

  // --- Document Setup ---
  class VDocument extends VNode {
    constructor() {
      super(9, "#document");
      docRoot = this;
      this.readyState = "complete";
      this.baseURI = "http://127.0.0.1/";
      this.defaultView = globalThis;
      this.fonts = documentFonts;
      const curScript = this.createElement("script");
      curScript._attrs.set("src", "./bundle.js");
      curScript._rsmotionExecuted = true;
      this.currentScript = curScript;

      this.documentElement = this.createElement("html");
      this.head = this.createElement("head");
      this.body = this.createElement("body");
      this.appendChild(this.documentElement);
      this.documentElement.appendChild(this.head);
      this.documentElement.appendChild(this.body);
      this.activeElement = this.body;

      for (const id of [
        "video-container",
        "__remotion-studio-container",
        "remotion-error-overlay",
        "server-disconnected-overlay",
        "menuportal-0",
        "menuportal-1",
        "menuportal-2",
        "menuportal-3",
        "menuportal-4",
        "menuportal-5",
      ]) {
        const div = this.createElement("div");
        div.id = id;
        this.body.appendChild(div);
      }
      this.body.appendChild(curScript);
    }
    createElement(tag) {
      const lower = String(tag).toLowerCase();
      let el;
      if (lower === "img") el = new HTMLImageElement();
      else if (lower === "canvas") el = new HTMLCanvasElement();
      else if (lower === "video" || lower === "audio") el = new HTMLMediaElement(lower);
      else if (lower === "iframe") el = new HTMLIFrameElement();
      else if (lower === "script") el = new HTMLScriptElement();
      else el = new VElement(lower, HTML_NS);
      el.ownerDocument = this;
      return el;
    }
    createElementNS(ns, tag) {
      if (!ns || ns === HTML_NS) return this.createElement(tag);
      const el = new VElement(tag, ns);
      el.ownerDocument = this;
      return el;
    }
    createTextNode(data) {
      const n = new VText(data);
      n.ownerDocument = this;
      return n;
    }
    createComment(data) {
      const n = new VComment(data);
      n.ownerDocument = this;
      return n;
    }
    createDocumentFragment() {
      const f = new VDocumentFragment();
      f.ownerDocument = this;
      return f;
    }
    getElementById(id) { return this.documentElement.getElementById(id); }
    getElementsByTagName(tag) { return this.documentElement.getElementsByTagName(tag); }
    getElementsByClassName(cls) { return this.documentElement.getElementsByClassName(cls); }
    querySelector(sel) { return this.documentElement.querySelector(sel); }
    querySelectorAll(sel) { return this.documentElement.querySelectorAll(sel); }
  }

  globalThis.Node = VNode;
  globalThis.Text = VText;
  globalThis.Comment = VComment;
  globalThis.Element = VElement;
  globalThis.HTMLElement = VElement;
  globalThis.SVGElement = SVGElement;
  globalThis.HTMLImageElement = HTMLImageElement;
  globalThis.Image = HTMLImageElement;
  globalThis.HTMLCanvasElement = HTMLCanvasElement;
  globalThis.HTMLIFrameElement = HTMLIFrameElement;
  globalThis.HTMLScriptElement = HTMLScriptElement;
  globalThis.HTMLMediaElement = HTMLMediaElement;
  globalThis.document = new VDocument();

  // --- Window Browser Environment Stubs ---
  globalThis.location = {
    href: "http://127.0.0.1/",
    origin: "http://127.0.0.1",
    protocol: "http:",
    host: "127.0.0.1",
    hostname: "127.0.0.1",
    port: "",
    pathname: "/",
    search: "",
    hash: "",
    toString() { return this.href; },
  };
  globalThis.navigator = {
    userAgent: "Mozilla/5.0 rsmotion/0.1",
    language: "en-US",
    languages: ["en-US"],
    platform: "Linux x86_64",
    hardwareConcurrency: 4,
    maxTouchPoints: 0,
  };
  globalThis.performance = {
    now: () => Date.now() - START_MS,
  };
  globalThis.devicePixelRatio = 1;
  globalThis.innerWidth = 1920;
  globalThis.innerHeight = 1080;
  globalThis.scrollX = 0;
  globalThis.scrollY = 0;
  globalThis.scrollTo = () => {};
  globalThis.matchMedia = (q) => ({
    matches: false,
    media: String(q),
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
  });
  globalThis.getComputedStyle = (el) => (el && el.style ? el.style : createStyleObject());
  globalThis.ResizeObserver = class ResizeObserver { observe() {} unobserve() {} disconnect() {} };
  globalThis.IntersectionObserver = class IntersectionObserver { observe() {} unobserve() {} disconnect() {} };
  globalThis.MutationObserver = class MutationObserver { observe() {} disconnect() {} takeRecords() { return []; } };
  const makeStorage = () => {
    const store = new Map();
    return {
      getItem: (k) => (store.has(String(k)) ? store.get(String(k)) : null),
      setItem: (k, v) => { store.set(String(k), String(v)); },
      removeItem: (k) => { store.delete(String(k)); },
      clear: () => { store.clear(); },
      key: (i) => [...store.keys()][i] ?? null,
      get length() { return store.size; },
    };
  };
  globalThis.localStorage = makeStorage();
  globalThis.sessionStorage = makeStorage();
  globalThis.history = {
    length: 1,
    state: null,
    pushState() {},
    replaceState() {},
    back() {},
    forward() {},
    go() {},
  };
  globalThis.screen = { width: 1920, height: 1080, availWidth: 1920, availHeight: 1080, colorDepth: 24, pixelDepth: 24 };
  class RSMHeaders {
    constructor(init) {
      this._map = new Map();
      if (init instanceof RSMHeaders) {
        for (const [k, v] of init._map.entries()) this._map.set(k, v);
      } else if (Array.isArray(init)) {
        for (const pair of init) {
          if (pair && pair.length >= 2) this.set(pair[0], pair[1]);
        }
      } else if (init && typeof init === "object") {
        for (const [k, v] of Object.entries(init)) this.set(k, v);
      }
    }
    _norm(k) { return String(k).toLowerCase(); }
    has(k) { return this._map.has(this._norm(k)); }
    get(k) { const nk = this._norm(k); return this._map.has(nk) ? this._map.get(nk) : null; }
    set(k, v) { this._map.set(this._norm(k), String(v)); }
    append(k, v) {
      const nk = this._norm(k);
      const prev = this._map.get(nk);
      this._map.set(nk, prev ? `${prev}, ${v}` : String(v));
    }
    delete(k) { this._map.delete(this._norm(k)); }
    entries() { return this._map.entries(); }
    forEach(cb, thisArg) { this._map.forEach((v, k) => cb.call(thisArg, v, k, this)); }
    [Symbol.iterator]() { return this._map.entries(); }
  }
  globalThis.Headers = RSMHeaders;

  if (typeof globalThis.DOMException === "undefined") {
    class RSMDOMException extends Error {
      constructor(message = "", name = "Error") {
        super(String(message));
        this.name = String(name);
      }
    }
    globalThis.DOMException = RSMDOMException;
  }

  class RSMRequest {
    constructor(input, init = {}) {
      const fromReq = input instanceof RSMRequest ? input : null;
      const rawUrl = fromReq
        ? fromReq.url
        : input && typeof input === "object" && "href" in input
          ? String(input.href)
          : String(input);
      this.url = new RSMURL(rawUrl, "http://127.0.0.1/").href;
      this.method = String(init.method || (fromReq ? fromReq.method : "GET")).toUpperCase();
      this.headers = new RSMHeaders(init.headers !== undefined ? init.headers : (fromReq ? fromReq.headers : undefined));
      this.signal = init.signal !== undefined ? init.signal : (fromReq ? fromReq.signal : null);
      this.cache = init.cache !== undefined ? init.cache : (fromReq ? fromReq.cache : "default");
      this.credentials = init.credentials !== undefined ? init.credentials : (fromReq ? fromReq.credentials : "same-origin");
      this.mode = init.mode !== undefined ? init.mode : (fromReq ? fromReq.mode : "cors");
      this.redirect = init.redirect !== undefined ? init.redirect : (fromReq ? fromReq.redirect : "follow");
      this.referrer = init.referrer !== undefined ? init.referrer : (fromReq ? fromReq.referrer : "about:client");
      this.referrerPolicy = init.referrerPolicy !== undefined ? init.referrerPolicy : (fromReq ? fromReq.referrerPolicy : "");
      this.bodyUsed = false;
    }
    clone() {
      return new RSMRequest(this);
    }
  }
  globalThis.Request = RSMRequest;

  class RSMResponse {
    constructor(body = null, init = {}) {
      this.status = init.status !== undefined ? Number(init.status) : 200;
      this.statusText = init.statusText !== undefined ? String(init.statusText) : (this.status === 200 ? "OK" : "");
      this.ok = this.status >= 200 && this.status < 300;
      this.headers = new RSMHeaders(init.headers);
      this.url = init.url !== undefined ? String(init.url) : "";
      this.type = init.type || "basic";
      this.redirected = Boolean(init.redirected);
      this.bodyUsed = false;
      if (body === null || body === undefined) {
        this._rawBytes = new Uint8Array(0);
        this.body = null;
      } else if (body instanceof RSMReadableStream) {
        this._rawBytes = null;
        this.body = body;
      } else {
        const u8 = body instanceof ArrayBuffer
          ? new Uint8Array(body)
          : ArrayBuffer.isView(body)
            ? new Uint8Array(body.buffer, body.byteOffset, body.byteLength)
            : new TextEncoder().encode(String(body));
        this._rawBytes = u8;
        this.body = makeChunkedAssetStreamFromBytes(u8);
      }
    }
    async arrayBuffer() {
      const b = await this.bytes();
      return b.buffer;
    }
    async bytes() {
      if (this._rawBytes) return this._rawBytes.slice();
      if (!this.body) return new Uint8Array(0);
      const reader = this.body.getReader();
      const chunks = [];
      let total = 0;
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        const u8 = value instanceof Uint8Array ? value : new Uint8Array(value);
        chunks.push(u8);
        total += u8.byteLength;
      }
      const out = new Uint8Array(total);
      let off = 0;
      for (const c of chunks) {
        out.set(c, off);
        off += c.byteLength;
      }
      return out;
    }
    async text() {
      return new TextDecoder().decode(await this.bytes());
    }
    async json() {
      return JSON.parse(await this.text());
    }
  }
  globalThis.Response = RSMResponse;

  class RSMReadableStream {
    constructor(underlying = {}) {
      this._underlying = underlying;
      this._queue = [];
      this._closed = false;
      this._errored = null;
      this._locked = false;
      const controller = {
        enqueue: (chunk) => { if (!this._closed) this._queue.push(chunk); },
        close: () => { this._closed = true; },
        error: (err) => { this._errored = err || new Error("stream error"); this._closed = true; },
        get desiredSize() { return 1; },
      };
      this._controller = controller;
      if (underlying && typeof underlying.start === "function") {
        Promise.resolve().then(() => underlying.start(controller)).catch((e) => controller.error(e));
      }
    }
    get locked() { return this._locked; }
    cancel(reason) {
      this._closed = true;
      this._queue.length = 0;
      if (this._underlying && typeof this._underlying.cancel === "function") {
        return Promise.resolve(this._underlying.cancel(reason));
      }
      return Promise.resolve();
    }
    getReader() {
      this._locked = true;
      const stream = this;
      return {
        read() {
          if (stream._errored) return Promise.reject(stream._errored);
          if (stream._queue.length > 0) {
            return Promise.resolve({ value: stream._queue.shift(), done: false });
          }
          if (stream._closed) {
            return Promise.resolve({ value: undefined, done: true });
          }
          if (stream._underlying && typeof stream._underlying.pull === "function") {
            return Promise.resolve(stream._underlying.pull(stream._controller)).then(() => {
              if (stream._errored) return Promise.reject(stream._errored);
              if (stream._queue.length > 0) {
                return { value: stream._queue.shift(), done: false };
              }
              return { value: undefined, done: true };
            });
          }
          return Promise.resolve({ value: undefined, done: true });
        },
        cancel(reason) {
          stream._locked = false;
          return stream.cancel(reason);
        },
        releaseLock() { stream._locked = false; },
      };
    }
    async *[Symbol.asyncIterator]() {
      const reader = this.getReader();
      try {
        while (true) {
          const { value, done } = await reader.read();
          if (done) break;
          yield value;
        }
      } finally {
        reader.releaseLock();
      }
    }
  }
  globalThis.ReadableStream = RSMReadableStream;

  function makeChunkedAssetStream(assetUrl, rangeStart, rangeEnd) {
    const CHUNK = 65536;
    let pos = rangeStart;
    return new RSMReadableStream({
      pull(controller) {
        if (pos > rangeEnd) {
          controller.close();
          return;
        }
        const sliceEnd = Math.min(rangeEnd, pos + CHUNK - 1);
        const bytes = new Uint8Array(ops.op_read_asset_slice(assetUrl, pos, sliceEnd));
        pos = sliceEnd + 1;
        if (bytes.byteLength > 0) {
          controller.enqueue(bytes);
        }
        if (pos > rangeEnd || bytes.byteLength === 0) {
          controller.close();
        }
      },
    });
  }

  globalThis.fetch = (url, init = {}) => {
    if (init && init.signal && init.signal.aborted) {
      return Promise.reject(new Error("The operation was aborted"));
    }
    const rawInput = (url && typeof url === "object" && "url" in url) ? String(url.url) : String(url);
    let parsed;
    try {
      parsed = new RSMURL(rawInput, "http://127.0.0.1/");
    } catch (e) {
      return Promise.reject(e);
    }
    if (parsed.pathname === "/proxy") {
      const src = parsed.searchParams.get("src") || "";
      const time = Number(parsed.searchParams.get("time") || "0") || 0;
      const transparent = parsed.searchParams.get("transparent") === "true";
      const toneMapped = parsed.searchParams.get("toneMapped") === "true";
      const [w, h, _dur, hasVideo] = ops.op_probe_media(src);
      if (!hasVideo || w === 0 || h === 0) {
        const errJson = JSON.stringify({ error: `Cannot resolve video source: ${src}` });
        const errBytes = new TextEncoder().encode(errJson);
        return Promise.resolve({
          ok: false,
          status: 500,
          statusText: "Internal Server Error",
          url: parsed.href,
          type: "basic",
          redirected: false,
          headers: new RSMHeaders({ "content-type": "application/json" }),
          body: makeChunkedAssetStreamFromBytes(errBytes),
          json: () => Promise.resolve(JSON.parse(errJson)),
          text: () => Promise.resolve(errJson),
          arrayBuffer: () => Promise.resolve(errBytes.slice().buffer),
          bytes: () => Promise.resolve(errBytes.slice()),
        });
      }
      const payload = JSON.stringify({ src, time, transparent, toneMapped, width: w, height: h });
      const bytes = new TextEncoder().encode(payload);
      return Promise.resolve({
        ok: true,
        status: 200,
        statusText: "OK",
        url: parsed.href,
        type: "basic",
        redirected: false,
        headers: new RSMHeaders({ "content-type": VIDEO_FRAME_MIME }),
        body: makeChunkedAssetStreamFromBytes(bytes),
        arrayBuffer: () => Promise.resolve(bytes.slice().buffer),
        bytes: () => Promise.resolve(bytes.slice()),
        text: () => Promise.resolve(payload),
        json: () => Promise.resolve(JSON.parse(payload)),
      });
    }

    const assetTarget = rawInput.startsWith("http://") || rawInput.startsWith("https://") || rawInput.startsWith("//")
      ? rawInput
      : parsed.href;
    const [exists, totalLen, mime] = ops.op_stat_asset(assetTarget);
    if (!exists) {
      return Promise.resolve({
        ok: false,
        status: 404,
        statusText: "Not Found",
        url: parsed.href,
        type: "basic",
        redirected: false,
        headers: new RSMHeaders({ "content-type": "text/plain" }),
        body: makeChunkedAssetStreamFromBytes(new Uint8Array(0)),
        arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
        bytes: () => Promise.resolve(new Uint8Array(0)),
        text: () => Promise.resolve(""),
        json: () => Promise.reject(new Error(`404 Not Found: ${assetTarget}`)),
      });
    }

    const reqHeaders = new RSMHeaders(init && init.headers);
    const rangeHdr = reqHeaders.get("range");
    let status = 200;
    let statusText = "OK";
    let start = 0;
    let end = totalLen > 0 ? totalLen - 1 : -1;

    if (rangeHdr) {
      const m = /^bytes=(\d+)-(\d*)$/i.exec(rangeHdr.trim());
      if (m) {
        const s = Number(m[1]);
        const hasEnd = m[2] !== "";
        const e = hasEnd ? Math.min(Number(m[2]), totalLen - 1) : (totalLen - 1);
        if (s >= totalLen || (hasEnd && e < s)) {
          return Promise.resolve({
            ok: false,
            status: 416,
            statusText: "Range Not Satisfiable",
            url: parsed.href,
            type: "basic",
            redirected: false,
            headers: new RSMHeaders({
              "content-range": `bytes */${totalLen}`,
              "accept-ranges": "bytes",
            }),
            body: makeChunkedAssetStreamFromBytes(new Uint8Array(0)),
            arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
            bytes: () => Promise.resolve(new Uint8Array(0)),
            text: () => Promise.resolve(""),
            json: () => Promise.reject(new Error("416 Range Not Satisfiable")),
          });
        }
        status = 206;
        statusText = "Partial Content";
        start = s;
        end = e;
      }
    }

    const sliceLen = end >= start ? (end - start + 1) : 0;
    const respHeaders = new RSMHeaders({
      "content-type": mime || "application/octet-stream",
      "content-length": String(sliceLen),
      "accept-ranges": "bytes",
    });
    if (status === 206) {
      respHeaders.set("content-range", `bytes ${start}-${end}/${totalLen}`);
    }
    const readFullSlice = () => (sliceLen > 0 ? new Uint8Array(ops.op_read_asset_slice(assetTarget, start, end)) : new Uint8Array(0));
    return Promise.resolve({
      ok: true,
      status,
      statusText,
      url: parsed.href,
      type: "basic",
      redirected: false,
      headers: respHeaders,
      body: makeChunkedAssetStream(assetTarget, start, end),
      arrayBuffer: () => Promise.resolve(readFullSlice().buffer),
      bytes: () => Promise.resolve(readFullSlice()),
      text: () => Promise.resolve(new TextDecoder().decode(readFullSlice())),
      json: () => Promise.resolve(JSON.parse(new TextDecoder().decode(readFullSlice()))),
    });
  };

  function makeChunkedAssetStreamFromBytes(u8) {
    let sent = false;
    return new RSMReadableStream({
      pull(controller) {
        if (!sent && u8.byteLength > 0) {
          sent = true;
          controller.enqueue(u8);
        }
        controller.close();
      },
    });
  }

  // --- Web Audio API + WebCodecs AudioDecoder ---
  class RSMAudioBuffer {
    constructor({ numberOfChannels = 2, length = 0, sampleRate = 48000 } = {}) {
      this.numberOfChannels = Math.max(1, Number(numberOfChannels) || 1);
      this.length = Math.max(0, Number(length) || 0);
      this.sampleRate = Math.max(8000, Number(sampleRate) || 48000);
      this.duration = this.length / this.sampleRate;
      this._channels = new Array(this.numberOfChannels);
      for (let i = 0; i < this.numberOfChannels; i++) {
        this._channels[i] = new Float32Array(this.length);
      }
    }
    getChannelData(channel) {
      if (channel < 0 || channel >= this.numberOfChannels) {
        throw new RangeError(`Invalid channel index ${channel}`);
      }
      return this._channels[channel];
    }
    copyToChannel(source, channelNumber, startInChannel = 0) {
      const ch = this.getChannelData(channelNumber);
      const offset = Math.max(0, startInChannel | 0);
      const avail = Math.max(0, ch.length - offset);
      ch.set(source.subarray(0, avail), offset);
    }
    copyFromChannel(destination, channelNumber, startInChannel = 0) {
      const ch = this.getChannelData(channelNumber);
      const offset = Math.max(0, startInChannel | 0);
      const count = Math.min(destination.length, Math.max(0, ch.length - offset));
      destination.set(ch.subarray(offset, offset + count), 0);
    }
  }
  globalThis.AudioBuffer = RSMAudioBuffer;

  function pcmInterleavedToPlanarFloat32(pcmU8, channels) {
    const f32 = pcmU8.byteOffset % 4 === 0
      ? new Float32Array(pcmU8.buffer, pcmU8.byteOffset, pcmU8.byteLength >> 2)
      : new Float32Array(pcmU8.slice().buffer);
    const totalFrames = Math.floor(f32.length / channels);
    const planar = new Float32Array(totalFrames * channels);
    for (let f = 0; f < totalFrames; f++) {
      const base = f * channels;
      for (let c = 0; c < channels; c++) {
        planar[c * totalFrames + f] = f32[base + c];
      }
    }
    return { planar, totalFrames };
  }

  class RSMAudioContext {
    constructor(opts = {}) {
      this.sampleRate = Math.max(8000, Number(opts.sampleRate) || 48000);
      this.state = "running";
    }
    decodeAudioData(buffer) {
      const u8 = buffer instanceof ArrayBuffer
        ? new Uint8Array(buffer)
        : new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
      const [ok, outSr, outCh] = ops.op_decode_audio_bytes(u8, this.sampleRate);
      if (!ok) {
        return Promise.reject(new Error("decodeAudioData failed"));
      }
      const pcmU8 = new Uint8Array(ops.op_take_decoded_audio_pcm());
      const { planar, totalFrames } = pcmInterleavedToPlanarFloat32(pcmU8, outCh);
      const ab = new RSMAudioBuffer({ numberOfChannels: outCh, length: totalFrames, sampleRate: outSr });
      for (let c = 0; c < outCh; c++) {
        ab.copyToChannel(planar.subarray(c * totalFrames, (c + 1) * totalFrames), c, 0);
      }
      return Promise.resolve(ab);
    }
    close() {
      this.state = "closed";
      return Promise.resolve();
    }
  }
  globalThis.AudioContext = RSMAudioContext;
  globalThis.webkitAudioContext = RSMAudioContext;

  function toF32PlanarArray(init) {
    const frames = Number(init.numberOfFrames) || 0;
    const ch = Math.max(1, Number(init.numberOfChannels) || 1);
    const raw = init.data;
    const u8 = raw instanceof ArrayBuffer
      ? new Uint8Array(raw)
      : new Uint8Array(raw.buffer, raw.byteOffset, raw.byteLength);
    const fmt = String(init.format || "f32-planar").toLowerCase();
    const out = new Float32Array(frames * ch);
    if (fmt === "f32-planar") {
      const src = u8.byteOffset % 4 === 0
        ? new Float32Array(u8.buffer, u8.byteOffset, u8.byteLength >> 2)
        : new Float32Array(u8.slice().buffer);
      out.set(src.subarray(0, out.length));
    } else if (fmt === "f32") {
      const src = u8.byteOffset % 4 === 0
        ? new Float32Array(u8.buffer, u8.byteOffset, u8.byteLength >> 2)
        : new Float32Array(u8.slice().buffer);
      for (let f = 0; f < frames; f++) {
        for (let c = 0; c < ch; c++) out[c * frames + f] = src[f * ch + c] || 0;
      }
    } else if (fmt === "s16" || fmt === "s16-planar") {
      const src = u8.byteOffset % 2 === 0
        ? new Int16Array(u8.buffer, u8.byteOffset, u8.byteLength >> 1)
        : new Int16Array(u8.slice().buffer);
      if (fmt === "s16-planar") {
        for (let i = 0; i < Math.min(out.length, src.length); i++) out[i] = src[i] / 32768;
      } else {
        for (let f = 0; f < frames; f++) {
          for (let c = 0; c < ch; c++) out[c * frames + f] = (src[f * ch + c] || 0) / 32768;
        }
      }
    } else if (fmt === "s32" || fmt === "s32-planar") {
      const src = u8.byteOffset % 4 === 0
        ? new Int32Array(u8.buffer, u8.byteOffset, u8.byteLength >> 2)
        : new Int32Array(u8.slice().buffer);
      if (fmt === "s32-planar") {
        for (let i = 0; i < Math.min(out.length, src.length); i++) out[i] = src[i] / 2147483648;
      } else {
        for (let f = 0; f < frames; f++) {
          for (let c = 0; c < ch; c++) out[c * frames + f] = (src[f * ch + c] || 0) / 2147483648;
        }
      }
    } else if (fmt === "u8" || fmt === "u8-planar") {
      if (fmt === "u8-planar") {
        for (let i = 0; i < Math.min(out.length, u8.length); i++) out[i] = (u8[i] - 128) / 128;
      } else {
        for (let f = 0; f < frames; f++) {
          for (let c = 0; c < ch; c++) out[c * frames + f] = ((u8[f * ch + c] ?? 128) - 128) / 128;
        }
      }
    }
    return out;
  }

  class RSMAudioData {
    constructor(init) {
      this.format = "f32-planar";
      this.sampleRate = Number(init.sampleRate) || 48000;
      this.numberOfFrames = Number(init.numberOfFrames) || 0;
      this.numberOfChannels = Math.max(1, Number(init.numberOfChannels) || 1);
      this.timestamp = Number(init.timestamp) || 0;
      this.duration = Math.round((this.numberOfFrames / this.sampleRate) * 1_000_000);
      this._planar = toF32PlanarArray(init);
      this._closed = false;
    }
    allocationSize(options = {}) {
      const frameCount = options.frameCount !== undefined
        ? Number(options.frameCount)
        : (this.numberOfFrames - (Number(options.frameOffset) || 0));
      const fmt = String(options.format || this.format).toLowerCase();
      const mult = fmt.includes("planar") ? 1 : this.numberOfChannels;
      const bps = fmt.startsWith("s16") ? 2 : fmt.startsWith("u8") ? 1 : 4;
      return Math.max(0, frameCount) * mult * bps;
    }
    copyTo(destination, options = {}) {
      const planeIndex = Number(options.planeIndex) || 0;
      const frameOffset = Number(options.frameOffset) || 0;
      const frameCount = options.frameCount !== undefined
        ? Number(options.frameCount)
        : Math.max(0, this.numberOfFrames - frameOffset);
      const fmt = String(options.format || this.format).toLowerCase();
      const dstBytes = destination instanceof ArrayBuffer
        ? new Uint8Array(destination)
        : new Uint8Array(destination.buffer, destination.byteOffset, destination.byteLength);
      if (fmt === "f32-planar") {
        const start = planeIndex * this.numberOfFrames + frameOffset;
        const srcBytes = new Uint8Array(this._planar.buffer, start * 4, frameCount * 4);
        dstBytes.set(srcBytes);
      } else if (fmt === "s16-planar") {
        const tmp = new Int16Array(frameCount);
        const start = planeIndex * this.numberOfFrames + frameOffset;
        for (let f = 0; f < frameCount; f++) {
          const v = this._planar[start + f] || 0;
          tmp[f] = Math.max(-32768, Math.min(32767, Math.round(v * 32767)));
        }
        dstBytes.set(new Uint8Array(tmp.buffer));
      } else if (fmt === "s16") {
        const tmp = new Int16Array(frameCount * this.numberOfChannels);
        for (let f = 0; f < frameCount; f++) {
          for (let c = 0; c < this.numberOfChannels; c++) {
            const v = this._planar[c * this.numberOfFrames + frameOffset + f] || 0;
            tmp[f * this.numberOfChannels + c] = Math.max(-32768, Math.min(32767, Math.round(v * 32767)));
          }
        }
        dstBytes.set(new Uint8Array(tmp.buffer));
      } else {
        const tmp = new Float32Array(frameCount * this.numberOfChannels);
        for (let f = 0; f < frameCount; f++) {
          for (let c = 0; c < this.numberOfChannels; c++) {
            tmp[f * this.numberOfChannels + c] = this._planar[c * this.numberOfFrames + frameOffset + f];
          }
        }
        dstBytes.set(new Uint8Array(tmp.buffer));
      }
    }
    clone() {
      return new RSMAudioData({
        format: this.format,
        sampleRate: this.sampleRate,
        numberOfFrames: this.numberOfFrames,
        numberOfChannels: this.numberOfChannels,
        timestamp: this.timestamp,
        data: this._planar.slice(),
      });
    }
    close() {
      this._closed = true;
      this._planar = new Float32Array(0);
    }
  }
  globalThis.AudioData = RSMAudioData;

  class RSMEncodedAudioChunk {
    constructor(init) {
      this.type = init.type || "key";
      this.timestamp = Number(init.timestamp) || 0;
      this.duration = init.duration !== undefined ? Number(init.duration) : null;
      const d = init.data;
      this._bytes = d instanceof ArrayBuffer
        ? new Uint8Array(d.slice(0))
        : new Uint8Array(d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength));
      this.byteLength = this._bytes.byteLength;
    }
    copyTo(destination) {
      const dst = destination instanceof ArrayBuffer
        ? new Uint8Array(destination)
        : new Uint8Array(destination.buffer, destination.byteOffset, destination.byteLength);
      dst.set(this._bytes);
    }
  }
  globalThis.EncodedAudioChunk = RSMEncodedAudioChunk;

  class RSMEncodedVideoChunk {
    constructor(init) {
      this.type = init.type || "key";
      this.timestamp = Number(init.timestamp) || 0;
      this.duration = init.duration !== undefined ? Number(init.duration) : null;
      const d = init.data;
      this._bytes = d instanceof ArrayBuffer
        ? new Uint8Array(d.slice(0))
        : new Uint8Array(d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength));
      this.byteLength = this._bytes.byteLength;
    }
    copyTo(destination) {
      const dst = destination instanceof ArrayBuffer
        ? new Uint8Array(destination)
        : new Uint8Array(destination.buffer, destination.byteOffset, destination.byteLength);
      dst.set(this._bytes);
    }
  }
  globalThis.EncodedVideoChunk = RSMEncodedVideoChunk;

  class RSMAudioDecoder extends EventTarget {
    static isConfigSupported(config) {
      return Promise.resolve({ supported: true, config });
    }
    constructor(init = {}) {
      super();
      this._output = init.output;
      this._error = init.error;
      this.state = "unconfigured";
      this.decodeQueueSize = 0;
      this._config = null;
      this._decoderId = 0;
      this._pending = [];
      this._drainScheduled = false;
    }
    configure(config) {
      if (this._decoderId > 0) {
        ops.op_close_audio_decoder(this._decoderId);
        this._decoderId = 0;
      }
      this._config = { ...config };
      let descBytes = new Uint8Array(0);
      if (this._config.description) {
        const d = this._config.description;
        descBytes = d instanceof ArrayBuffer
          ? new Uint8Array(d)
          : new Uint8Array(d.buffer, d.byteOffset, d.byteLength);
      }
      this._decoderId = ops.op_create_audio_decoder(
        String(this._config.codec || ""),
        Number(this._config.sampleRate || 0) | 0,
        Number(this._config.numberOfChannels || 0) | 0,
        descBytes,
      );
      this.state = "configured";
    }
    _decodeOneBatch(maxCount) {
      if (this.state !== "configured" || this._pending.length === 0) return;
      const batch = this._pending.splice(0, maxCount);
      this.decodeQueueSize = this._pending.length;
      const lengths = [];
      let totalBytes = 0;
      for (const item of batch) {
        lengths.push(item.bytes.byteLength);
        totalBytes += item.bytes.byteLength;
      }
      const combined = new Uint8Array(totalBytes);
      let off = 0;
      for (const item of batch) {
        combined.set(item.bytes, off);
        off += item.bytes.byteLength;
      }
      let descBytes = new Uint8Array(0);
      if (this._decoderId === 0 && this._config && this._config.description) {
        const d = this._config.description;
        descBytes = d instanceof ArrayBuffer
          ? new Uint8Array(d)
          : new Uint8Array(d.buffer, d.byteOffset, d.byteLength);
      }
      const [ok, outSr, outCh] = ops.op_decode_audio_chunks(
        this._decoderId,
        String((this._config && this._config.codec) || ""),
        Number((this._config && this._config.sampleRate) || 0) | 0,
        Number((this._config && this._config.numberOfChannels) || 0) | 0,
        descBytes,
        combined,
        lengths,
      );
      if (!ok) {
        if (typeof this._error === "function") {
          this._error(new Error("AudioDecoder batch decode failed"));
        }
        return;
      }
      const pcmU8 = new Uint8Array(ops.op_take_decoded_audio_pcm());
      const { planar, totalFrames } = pcmInterleavedToPlanarFloat32(pcmU8, outCh);
      if (totalFrames > 0 && typeof this._output === "function") {
        this._output(new RSMAudioData({
          format: "f32-planar",
          sampleRate: outSr,
          numberOfChannels: outCh,
          numberOfFrames: totalFrames,
          timestamp: batch[0].timestamp,
          data: planar,
        }));
      }
    }
    decode(chunk) {
      if (this.state !== "configured") {
        throw new Error("AudioDecoder not configured");
      }
      const bytes = chunk._bytes
        ? chunk._bytes
        : (() => {
            const b = new Uint8Array(chunk.byteLength);
            chunk.copyTo(b);
            return b;
          })();
      this._pending.push({ bytes, timestamp: Number(chunk.timestamp) || 0 });
      this.decodeQueueSize = this._pending.length;
      if (!this._drainScheduled) {
        this._drainScheduled = true;
        setTimeout(() => {
          this._drainScheduled = false;
          if (this.state === "configured" && this._pending.length > 0) {
            while (this.state === "configured" && this._pending.length > 0) {
              this._decodeOneBatch(128);
            }
            this.dispatchEvent(new Event("dequeue"));
          }
        }, 0);
      }
    }
    flush() {
      while (this.state === "configured" && this._pending.length > 0) {
        this._decodeOneBatch(128);
      }
      this.decodeQueueSize = 0;
      this.dispatchEvent(new Event("dequeue"));
      return Promise.resolve();
    }
    reset() {
      if (this._decoderId > 0) {
        ops.op_close_audio_decoder(this._decoderId);
        this._decoderId = 0;
      }
      this.state = "unconfigured";
      this._pending.length = 0;
      this.decodeQueueSize = 0;
    }
    close() {
      if (this._decoderId > 0) {
        ops.op_close_audio_decoder(this._decoderId);
        this._decoderId = 0;
      }
      this.state = "closed";
      this._pending.length = 0;
      this.decodeQueueSize = 0;
    }
  }
  globalThis.AudioDecoder = RSMAudioDecoder;

  // --- Native Lottie (Skottie) Interception ---
  globalThis.__rsmotion_wrap_lottie = (origLoadAnimation) => {
    return function rsmotionLoadAnimation(params) {
      if (!params || !params.container || !params.animationData || typeof params.animationData !== "object") {
        return typeof origLoadAnimation === "function" ? origLoadAnimation.call(this, params) : {};
      }
      const container = params.container;
      const assetsPath = params.assetsPath ? String(params.assetsPath) : "";
      const preserveAspectRatio =
        (params.rendererSettings && params.rendererSettings.preserveAspectRatio) || null;
      const meta = ops.op_register_lottie(JSON.stringify(params.animationData), assetsPath);
      const listeners = new Map();
      const emit = (type) => {
        const list = listeners.get(type);
        if (!list) return;
        for (const fn of [...list]) {
          try { fn({ type }); } catch (_e) {}
        }
      };

      if (!meta || !meta.ok) {
        container._lottieFailed = "Lottie animation unsupported by native Skottie";
        const dummy = {
          totalFrames: 1,
          currentFrame: 0,
          playDirection: 1,
          isLoaded: false,
          addEventListener(type, fn) {
            if (!listeners.has(type)) listeners.set(type, []);
            listeners.get(type).push(fn);
          },
          removeEventListener(type, fn) {
            const list = listeners.get(type);
            if (!list) return;
            const i = list.indexOf(fn);
            if (i >= 0) list.splice(i, 1);
          },
          setDirection() {},
          goToAndStop() {},
          goToAndPlay() {},
          play() {},
          pause() {},
          stop() {},
          setSpeed() {},
          resize() {},
          destroy() { delete container._lottieFailed; },
        };
        queueMicrotask(() => emit("DOMLoaded"));
        return dummy;
      }

      const lottieState = {
        id: meta.id,
        frame: 0,
        width: Number(meta.width) || 100,
        height: Number(meta.height) || 100,
        preserveAspectRatio,
      };
      container._lottie = lottieState;

      const animItem = {
        totalFrames: Number(meta.totalFrames) || 1,
        currentFrame: 0,
        playDirection: 1,
        isLoaded: true,
        addEventListener(type, fn) {
          if (!listeners.has(type)) listeners.set(type, []);
          listeners.get(type).push(fn);
        },
        removeEventListener(type, fn) {
          const list = listeners.get(type);
          if (!list) return;
          const i = list.indexOf(fn);
          if (i >= 0) list.splice(i, 1);
        },
        setDirection(d) {
          this.playDirection = Number(d) < 0 ? -1 : 1;
        },
        goToAndStop(val, _isFrame) {
          const f = Number(val);
          this.currentFrame = Number.isFinite(f) ? f : 0;
          lottieState.frame = this.currentFrame;
        },
        goToAndPlay(val, isFrame) {
          this.goToAndStop(val, isFrame);
        },
        play() {},
        pause() {},
        stop() {},
        setSpeed() {},
        resize() {},
        destroy() {
          if (container._lottie === lottieState) {
            delete container._lottie;
          }
          listeners.clear();
        },
      };
      queueMicrotask(() => emit("DOMLoaded"));
      return animItem;
    };
  };

  // --- SVG XML Serialization & SceneTree Snapshot ---
  function escapeXmlAttr(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }
  function escapeXmlText(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  function sanitizeSvgFontFamily(raw) {
    return String(raw).replace(/["']/g, "").trim();
  }

  function serializeSvgNode(node, isRoot, inheritedFont, inheritedColor, unsupported) {
    if (node.nodeType === 3) return escapeXmlText(node._data);
    if (node.nodeType !== 1) return "";
    const tag = node.localName;
    if (tag.toLowerCase() === "foreignobject") {
      unsupported.push({ feature: "foreignObject", tag, detail: "SVG <foreignObject> requires Tier 2" });
    }
    let out = `<${tag}`;
    const hasXmlns = node._attrs.has("xmlns");
    if (isRoot && !hasXmlns) {
      out += ` xmlns="http://www.w3.org/2000/svg"`;
    }
    let hasFont = node._attrs.has("font-family") || node._style._map.has("font-family");
    let hasColor = node._attrs.has("color") || node._style._map.has("color");

    for (const [k, v] of node._attrs.entries()) {
      const val = k === "font-family" ? sanitizeSvgFontFamily(v) : v;
      out += ` ${k}="${escapeXmlAttr(val)}"`;
    }
    if (isRoot && !hasFont && inheritedFont) {
      out += ` font-family="${escapeXmlAttr(sanitizeSvgFontFamily(inheritedFont))}"`;
    }
    if (isRoot && !hasColor && inheritedColor) {
      out += ` color="${escapeXmlAttr(inheritedColor)}"`;
    }
    if (node._style._map.size > 0) {
      out += ` style="${escapeXmlAttr(node._style.cssText)}"`;
    }
    if (node.childNodes.length === 0) {
      return out + "/>";
    }
    out += ">";
    for (const ch of node.childNodes) {
      out += serializeSvgNode(ch, false, null, null, unsupported);
    }
    out += `</${tag}>`;
    return out;
  }

  function parseOptionalPx(val) {
    if (val === null || val === undefined || val === "") return null;
    const n = Number.parseFloat(String(val));
    return Number.isFinite(n) ? n : null;
  }

  function parseViewBox(raw) {
    if (!raw) return null;
    const parts = String(raw).trim().split(/[\s,]+/).map(Number);
    if (parts.length === 4 && parts.every(Number.isFinite)) {
      return [parts[0], parts[1], parts[2], parts[3]];
    }
    return null;
  }

  const SKIP_TAGS = new Set(["script", "style", "link", "meta", "title"]);
  const UNSUPPORTED_TAGS = new Set(["iframe"]);
  const expandedInlineCssCache = new Map();

  globalThis.__rsmotion_snapshot = (width, height, frame) => {
    const rootEl =
      document.getElementById("remotion-canvas") ||
      document.getElementById("video-container") ||
      document.body;

    const nodes = [];
    const unsupported = [];
    const unsupportedSeen = new Set();
    const recordUnsupported = (feature, tag, detail) => {
      const key = `${feature}:${tag}:${detail}`;
      if (!unsupportedSeen.has(key)) {
        unsupportedSeen.add(key);
        unsupported.push({ feature, tag, detail });
      }
    };

    const visit = (domNode, inheritedFont, inheritedColor) => {
      if (domNode.nodeType === 8) return null;
      if (domNode.nodeType === 3) {
        const id = nodes.length;
        nodes.push({ id, kind: "text", data: domNode._data });
        return id;
      }
      if (domNode.nodeType !== 1) return null;

      const tagLower = domNode.localName.toLowerCase();
      if (SKIP_TAGS.has(tagLower)) return null;

      const nextFont = domNode._style._map.get("font-family") || inheritedFont;
      const nextColor = domNode._style._map.get("color") || inheritedColor;

      const stylePairs = [...domNode._style._map.entries()];
      const classList = domNode.className ? domNode.className.split(/\s+/).filter(Boolean) : [];
      const rawAttrs = [...domNode._attrs.entries()];

      if (UNSUPPORTED_TAGS.has(tagLower)) {
        recordUnsupported(tagLower, tagLower, `<${tagLower}> element`);
      }

      if (domNode._lottieFailed) {
        recordUnsupported("lottie-unsupported", tagLower, domNode._lottieFailed);
      }

      if (domNode._lottie) {
        const id = nodes.length;
        nodes.push({
          id,
          kind: "lottie",
          data: {
            animationId: domNode._lottie.id,
            frame: domNode._lottie.frame,
            width: domNode._lottie.width,
            height: domNode._lottie.height,
            preserveAspectRatio: domNode._lottie.preserveAspectRatio || null,
            classes: classList,
            attrs: rawAttrs,
            inlineStyle: stylePairs,
          },
        });
        return id;
      }

      if (tagLower === "canvas") {
        if (domNode._unsupportedCanvasKind) {
          recordUnsupported("canvas-webgl", "canvas", `<canvas getContext("${domNode._unsupportedCanvasKind}")>`);
        } else {
          const id = nodes.length;
          nodes.push({
            id,
            kind: "canvas2d",
            data: {
              width: domNode.width | 0,
              height: domNode.height | 0,
              classes: classList,
              attrs: rawAttrs,
              inlineStyle: stylePairs,
              ops: (domNode._ops || []).slice(),
            },
          });
          return id;
        }
      }

      if (tagLower === "svg") {
        const id = nodes.length;
        const svgUnsupported = [];
        const xml = serializeSvgNode(domNode, true, nextFont, nextColor, svgUnsupported);
        for (const u of svgUnsupported) recordUnsupported(u.feature, u.tag, u.detail);
        nodes.push({
          id,
          kind: "svg",
          data: {
            xml,
            classes: classList,
            attrs: rawAttrs,
            inlineStyle: stylePairs,
            widthAttr: parseOptionalPx(domNode.getAttribute("width")),
            heightAttr: parseOptionalPx(domNode.getAttribute("height")),
            viewBox: parseViewBox(domNode.getAttribute("viewBox")),
          },
        });
        return id;
      }

      let attrs = rawAttrs;
      if (tagLower === "img") {
        attrs = rawAttrs.map(([k, v]) => {
          if (k === "src" && __blobVideoMeta.has(v)) {
            const m = __blobVideoMeta.get(v);
            const uri = `rsmotion-video://frame?src=${encodeURIComponent(m.src)}&time=${encodeURIComponent(m.time)}&transparent=${m.transparent ? "true" : "false"}`;
            return [k, uri];
          }
          return [k, v];
        });
      } else if (tagLower === "video") {
        const vSrc = domNode.src;
        if (domNode.videoWidth > 0 && vSrc) {
          const t = Math.max(0, Number(domNode.currentTime) || 0);
          const uri = `rsmotion-video://frame?src=${encodeURIComponent(vSrc)}&time=${encodeURIComponent(t)}&transparent=false`;
          let replaced = false;
          attrs = rawAttrs.map(([k, v]) => {
            if (k === "src") {
              replaced = true;
              return [k, uri];
            }
            return [k, v];
          });
          if (!replaced) attrs.push(["src", uri]);
        } else if (vSrc) {
          recordUnsupported("remote-video", "video", vSrc);
        }
      }

      const id = nodes.length;
      const childIds = [];
      nodes.push({
        id,
        kind: "element",
        data: {
          tag: tagLower,
          classes: classList,
          inlineStyle: stylePairs,
          attrs,
          children: childIds,
        },
      });

      for (const ch of domNode.childNodes) {
        const cid = visit(ch, nextFont, nextColor);
        if (cid !== null) childIds.push(cid);
      }
      return id;
    };

    const rootIdx = visit(rootEl, '"DejaVu Sans", sans-serif', "#000000") ?? 0;

    const stylesheets = [];
    const collectSheets = (n) => {
      if (!n || n.nodeType !== 1) return;
      const t = n.localName.toLowerCase();
      if (t === "style") {
        const rawCss = (n.textContent || "").trim();
        const isStudioChrome = rawCss.includes("--remotion-studio-");
        if (rawCss && !isStudioChrome) {
          let css = rawCss;
          if (rawCss.toLowerCase().includes("@import")) {
            css = expandedInlineCssCache.get(rawCss);
            if (css === undefined) {
              css = ops.op_expand_css_imports(rawCss);
              expandedInlineCssCache.set(rawCss, css);
            }
          }
          stylesheets.push({
            source: `inline:${stylesheets.length}`,
            css,
          });
        }
      } else if (t === "link") {
        const rel = (n.getAttribute("rel") || "").toLowerCase();
        if (rel === "stylesheet") {
          const href = n.getAttribute("href") || "";
          const css = href ? ops.op_read_text_asset(href) : null;
          if (typeof css === "string") {
            if (css.trim().length > 0) {
              stylesheets.push({ source: href, css });
            }
          } else {
            recordUnsupported("external-stylesheet", "link", href);
          }
        }
      }
      for (const ch of n.childNodes) {
        collectSheets(ch);
      }
    };
    collectSheets(document.documentElement);

    const rawAssets = typeof globalThis.remotion_collectAssets === "function"
      ? globalThis.remotion_collectAssets()
      : [];
    const audioAssets = (rawAssets || [])
      .filter((a) => a && (a.type === "audio" || a.type === "video"))
      .map((a) => ({
        id: String(a.id || ""),
        src: String(a.src || ""),
        frame: Number(a.frame || 0),
        volume: Number(a.volume ?? 1),
        media_frame: Number(a.mediaFrame ?? 0),
        playback_rate: Number(a.playbackRate ?? 1),
        audio_start_frame: Number(a.audioStartFrame ?? 0),
      }));

    return {
      width,
      height,
      frame,
      root: rootIdx,
      nodes,
      fonts: [],
      stylesheets,
      audioAssets,
      unsupportedFeatures: unsupported,
    };
  };
})();
