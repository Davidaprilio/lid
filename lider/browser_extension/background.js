/* LID Integration: hands browser downloads, links and videos to LID through native messaging. */
"use strict";

if (typeof importScripts === "function" && !globalThis.LIDCapture) importScripts("capture.js");

const HOST = "io.github.davidaprilio.lid";
const CONFIG_TTL = 30000;
const YOUTUBE_PAGES = ["*://*.youtube.com/watch*", "*://*.youtube.com/shorts/*"];
// The Chrome Web Store build has no YouTube content script; then the YouTube items are left out too.
const HAS_YOUTUBE = (chrome.runtime.getManifest().content_scripts || []).length > 0;
let config = null;
let configTime = 0;
let lastError = "";
const handled = new Set();

// chrome.* with callbacks works in Chrome 109+ and in Firefox, so one code path serves both.
function call(target, method, ...args) {
  return new Promise((resolve) => {
    try {
      target[method](...args, (value) => {
        void chrome.runtime.lastError;
        resolve(value);
      });
    } catch (e) {
      resolve(undefined);
    }
  });
}

function send(message) {
  return new Promise((resolve) => {
    try {
      chrome.runtime.sendNativeMessage(HOST, message, (reply) => {
        const err = chrome.runtime.lastError;
        resolve(err || !reply ? { ok: false, error: err ? err.message : "no reply from LID" } : reply);
      });
    } catch (e) {
      resolve({ ok: false, error: String(e && e.message ? e.message : e) });
    }
  });
}

async function isEnabled() {
  const stored = await call(chrome.storage.local, "get", "enabled");
  return !stored || stored.enabled !== false;
}

async function getConfig(force) {
  if (!force && config && Date.now() - configTime < CONFIG_TTL) return config;
  const reply = await send({ type: "hello" });
  config = reply.ok ? reply : null;
  lastError = reply.ok ? "" : reply.error;
  configTime = Date.now();
  updateBadge();
  return config;
}

async function updateBadge() {
  const on = await isEnabled();
  const text = !on ? "off" : config ? "" : "!";
  chrome.action.setBadgeText({ text });
  chrome.action.setBadgeBackgroundColor({ color: on ? "#B91C1C" : "#475569" });
  chrome.action.setTitle({ title: !on ? "LID: capture is off" : config ? "LID" : `LID: ${lastError || "not connected"}` });
}

async function cookieHeader(url) {
  const cookies = await call(chrome.cookies, "getAll", { url });
  return (cookies || []).map((c) => `${c.name}=${c.value}`).join("; ");
}

async function handOver(info) {
  const reply = await send({
    type: "download",
    ...info,
    cookies: await cookieHeader(info.url),
    user_agent: navigator.userAgent,
  });
  if (!reply.ok) {
    lastError = reply.error;
    config = null;
    updateBadge();
  }
  return reply;
}

// ---------------------------------------------------------------- taking over downloads
async function captureDownload(item) {
  if (handled.has(item.id) || item.state !== "in_progress") return;
  if (item.byExtensionId && item.byExtensionId === chrome.runtime.id) return;
  if (!(await isEnabled())) return;
  const url = item.finalUrl || item.url;
  const name = String(item.filename || "").split(/[\\/]/).pop();
  if (!LIDCapture.shouldCapture(await getConfig(), url, name)) return;
  handled.add(item.id);
  await call(chrome.downloads, "pause", item.id);
  const reply = await handOver({
    url,
    referer: item.referrer || "",
    filename: name,
    size: item.totalBytes > 0 ? item.totalBytes : undefined,
    mime: item.mime || "",
  });
  if (reply.ok) {
    await call(chrome.downloads, "cancel", item.id);
    await call(chrome.downloads, "erase", { id: item.id });
  } else {
    await call(chrome.downloads, "resume", item.id);
  }
}

// Chrome has onDeterminingFilename, where the server's file name (Content-Disposition) is already
// known. Firefox does not; there, onCreated gives the full target path.
const determining = chrome.downloads["onDeterminingFilename"];
if (determining) {
  determining.addListener((item, suggest) => {
    suggest();
    captureDownload(item);
  });
} else {
  chrome.downloads.onCreated.addListener(captureDownload);
}

// ---------------------------------------------------------------- context menus
function createMenus() {
  chrome.contextMenus.removeAll(() => {
    void chrome.runtime.lastError;
    chrome.contextMenus.create({ id: "lid-link", title: "Download with LID", contexts: ["link", "image", "video", "audio"] });
    chrome.contextMenus.create({ id: "lid-selection", title: "Download selected links with LID", contexts: ["selection"] });
    chrome.contextMenus.create({ id: "lid-all", title: "Download all links with LID", contexts: ["page", "frame"] });
    if (HAS_YOUTUBE) {
      chrome.contextMenus.create({
        id: "lid-video-page",
        title: "Download this video with LID",
        contexts: ["page", "video"],
        documentUrlPatterns: YOUTUBE_PAGES,
      });
    }
  });
}

chrome.runtime.onInstalled.addListener(() => {
  createMenus();
  getConfig(true);
});
chrome.runtime.onStartup.addListener(() => {
  createMenus();
  getConfig(true);
});

// Runs inside the page: the links, images and media of the document (or of the selection).
function collectLinks(onlySelection) {
  const selection = window.getSelection();
  const ranges = [];
  for (let i = 0; selection && i < selection.rangeCount; i++) ranges.push(selection.getRangeAt(i));
  const seen = new Set();
  const out = [];
  const add = (url, text, kind, el) => {
    if (!url || seen.has(url) || !/^(https?|ftp):/i.test(url)) return;
    if (onlySelection && !ranges.some((r) => r.intersectsNode(el))) return;
    seen.add(url);
    out.push({ url, text: String(text || "").trim().replace(/\s+/g, " ").slice(0, 300), kind });
  };
  for (const a of document.querySelectorAll("a[href]")) add(a.href, a.textContent, "a", a);
  for (const img of document.querySelectorAll("img[src]")) add(img.currentSrc || img.src, img.alt, "img", img);
  for (const media of document.querySelectorAll("video, audio")) {
    const kind = media.tagName.toLowerCase();
    add(media.currentSrc || media.src, media.title, kind, media);
    for (const source of media.querySelectorAll("source[src]")) add(source.src, media.title, kind, source);
  }
  return out.slice(0, 5000);
}

async function collect(tabId, frameId, onlySelection) {
  const results = await call(chrome.scripting, "executeScript", {
    target: { tabId, frameIds: [frameId || 0] },
    func: collectLinks,
    args: [onlySelection],
  });
  return (results && results[0] && results[0].result) || [];
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const page = info.pageUrl || (tab && tab.url) || "";
  if (info.menuItemId === "lid-link") {
    const url = info.linkUrl || info.srcUrl;
    if (url) await handOver({ url, referer: page, page_url: page });
  } else if (info.menuItemId === "lid-video-page") {
    await send({ type: "download", url: page, page_url: page });
  } else if (tab && (info.menuItemId === "lid-all" || info.menuItemId === "lid-selection")) {
    const items = await collect(tab.id, info.frameId, info.menuItemId === "lid-selection");
    if (items.length) {
      await send({ type: "links", items, referer: page, page_url: page,
                   cookies: await cookieHeader(page), user_agent: navigator.userAgent });
    }
  }
});

// ---------------------------------------------------------------- popup and YouTube button
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (!msg || typeof msg !== "object") return false;
  if (HAS_YOUTUBE && msg.type === "video" && typeof msg.url === "string") {
    send({ type: "download", url: msg.url, page_url: msg.url }).then(sendResponse);
    return true;
  }
  if (msg.type === "status") {
    Promise.all([getConfig(true), isEnabled()]).then(([cfg, enabled]) =>
      sendResponse({ connected: Boolean(cfg), enabled, version: cfg && cfg.version, capture: cfg && cfg.capture,
                     running: cfg && cfg.running, error: lastError }));
    return true;
  }
  if (msg.type === "set-enabled") {
    chrome.storage.local.set({ enabled: Boolean(msg.enabled) }, () => {
      updateBadge();
      sendResponse({ ok: true });
    });
    return true;
  }
  if (msg.type === "show") {
    send({ type: "show" }).then(sendResponse);
    return true;
  }
  return false;
});

getConfig(true);
