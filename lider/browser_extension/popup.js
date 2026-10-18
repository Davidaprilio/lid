"use strict";
const status = document.getElementById("status");
const hint = document.getElementById("hint");
const enabled = document.getElementById("enabled");

function render(s) {
  if (!s) return;
  enabled.checked = s.enabled;
  if (s.connected) {
    status.textContent = `Connected to LID ${s.version}`;
    status.className = "status ok";
    hint.textContent = s.capture ? "Downloads of the file types set in LID go to LID."
                                 : "Capture is off in LID: Options → General → Browser integration.";
  } else {
    status.textContent = "LID helper not found";
    status.className = "status bad";
    hint.textContent = "In LID open Options → General → Set up browsers… and install it for this browser.";
  }
}

chrome.runtime.sendMessage({ type: "status" }, (s) => { void chrome.runtime.lastError; render(s); });
enabled.addEventListener("change", () => chrome.runtime.sendMessage({ type: "set-enabled", enabled: enabled.checked }));
document.getElementById("open").addEventListener("click", () =>
  chrome.runtime.sendMessage({ type: "show" }, () => { void chrome.runtime.lastError; window.close(); }));
