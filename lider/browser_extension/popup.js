"use strict";
const status = document.getElementById("status");
const hint = document.getElementById("hint");
const enabled = document.getElementById("enabled");
const open = document.getElementById("open");
const get = document.getElementById("get");
const HOME = "https://github.com/Davidaprilio/lid";
let getUrl = HOME + "/releases/latest";

function render(s) {
  if (!s) return;
  enabled.checked = s.enabled;
  if (s.connected) {
    status.textContent = `Connected to LID ${s.version}`;
    status.className = "status ok";
    hint.textContent = s.capture ? "Downloads of the file types set in LID go to LID."
                                 : "Capture is off in LID: Options → General → Browser integration.";
  } else {
    // Say what the extension needs: LID is a desktop app for Linux, reached through its helper.
    open.hidden = true;
    get.hidden = false;
    status.className = "status bad";
    chrome.runtime.getPlatformInfo((info) => {
      void chrome.runtime.lastError;
      if (info && info.os && info.os !== "linux") {
        status.textContent = "LID runs on Linux only";
        hint.textContent = "This extension hands downloads to LID, a download manager for Linux. " +
                           "There is no LID on this system, so downloads stay in the browser.";
        enabled.closest("label").hidden = true;
        get.textContent = "About LID";
        getUrl = HOME;
      } else {
        status.textContent = "LID helper not found";
        hint.textContent = "This extension works with LID, a download manager for Linux. Install LID, then in " +
                           "LID open Options → General → Set up browsers… and install the helper for this browser.";
      }
    });
  }
}

chrome.runtime.sendMessage({ type: "status" }, (s) => { void chrome.runtime.lastError; render(s); });
enabled.addEventListener("change", () => chrome.runtime.sendMessage({ type: "set-enabled", enabled: enabled.checked }));
get.addEventListener("click", () => chrome.tabs.create({ url: getUrl }, () => window.close()));
open.addEventListener("click", () =>
  chrome.runtime.sendMessage({ type: "show" }, () => { void chrome.runtime.lastError; window.close(); }));
