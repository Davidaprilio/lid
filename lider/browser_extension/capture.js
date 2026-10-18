/* Decides which browser downloads LID takes over. Used by background.js and by the tests. */
(function (root) {
  "use strict";

  function extensionOf(name) {
    const base = String(name || "").split(/[\\/]/).pop().split(/[?#]/)[0];
    const dot = base.lastIndexOf(".");
    return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
  }

  function fileNameFromUrl(url) {
    try {
      return decodeURIComponent(new URL(url).pathname.split("/").pop() || "");
    } catch (e) {
      return "";
    }
  }

  // LID's file type list uses shell wildcards, e.g. "R0*" for rar volumes.
  function wildcard(pattern) {
    const escaped = String(pattern).toLowerCase().replace(/[.+^${}()|[\]\\]/g, "\\$&");
    return new RegExp("^" + escaped.replace(/\*/g, ".*").replace(/\?/g, ".") + "$");
  }

  function matchesType(types, ext) {
    return Boolean(ext) && (types || []).some((t) => wildcard(t).test(ext));
  }

  function hostSkipped(sites, host) {
    host = String(host || "").toLowerCase();
    return (sites || []).some((site) => {
      site = String(site).toLowerCase().replace(/^\*\./, "");
      return site && (host === site || host.endsWith("." + site));
    });
  }

  function shouldCapture(config, url, filename) {
    if (!config || !config.capture) return false;
    let parsed;
    try {
      parsed = new URL(url);
    } catch (e) {
      return false;
    }
    if (!/^(https?|ftp):$/.test(parsed.protocol)) return false;
    if (hostSkipped(config.skip_sites, parsed.hostname)) return false;
    const ext = extensionOf(filename) || extensionOf(fileNameFromUrl(url));
    return matchesType(config.types, ext);
  }

  const api = { extensionOf, fileNameFromUrl, matchesType, hostSkipped, shouldCapture };
  if (typeof module === "object" && module.exports) module.exports = api;
  else root.LIDCapture = api;
})(typeof globalThis !== "undefined" ? globalThis : this);
