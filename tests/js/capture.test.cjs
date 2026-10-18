// Run with: node --test tests/js
const test = require("node:test");
const assert = require("node:assert");
const c = require("../../lider/browser_extension/capture.js");

const config = { capture: true, types: ["ZIP", "ISO", "R0*", "MP4"], skip_sites: ["example.org"] };

test("file extension from names and URLs", () => {
  assert.equal(c.extensionOf("ubuntu-24.04.ISO"), "iso");
  assert.equal(c.extensionOf("/home/u/Downloads/a.tar.gz"), "gz");
  assert.equal(c.extensionOf(".bashrc"), "");
  assert.equal(c.fileNameFromUrl("https://h/x/My%20File.zip?x=1"), "My File.zip");
});

test("captures listed types, wildcards included", () => {
  assert.ok(c.shouldCapture(config, "https://cdn.net/a.zip", ""));
  assert.ok(c.shouldCapture(config, "https://cdn.net/get?id=5", "movie.mp4"));
  assert.ok(c.shouldCapture(config, "https://cdn.net/part.r01", ""));
  assert.ok(!c.shouldCapture(config, "https://cdn.net/page.html", ""));
});

test("respects the switch, schemes and excluded sites", () => {
  assert.ok(!c.shouldCapture({ ...config, capture: false }, "https://cdn.net/a.zip", ""));
  assert.ok(!c.shouldCapture(null, "https://cdn.net/a.zip", ""));
  assert.ok(!c.shouldCapture(config, "blob:https://cdn.net/123", "a.zip"));
  assert.ok(!c.shouldCapture(config, "https://dl.example.org/a.zip", ""));
  assert.ok(c.shouldCapture(config, "https://notexample.org/a.zip", ""));
});
