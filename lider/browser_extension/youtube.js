/* A "Download with LID" button on YouTube video pages; LID then offers the resolutions. */
(function () {
  "use strict";
  const ID = "lid-download-button";
  const LABEL = "Download with LID";

  function isVideoPage() {
    return location.pathname === "/watch" || location.pathname.startsWith("/shorts/");
  }

  function setState(button, state, text) {
    button.dataset.state = state;
    button.lastChild.textContent = text;
  }

  function create() {
    const button = document.createElement("button");
    button.id = ID;
    button.type = "button";
    button.title = "Send this video to LID and choose the resolution or audio only";
    const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    icon.setAttribute("viewBox", "0 0 24 24");
    icon.setAttribute("aria-hidden", "true");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", "M12 4v10.5M8 10.5l4 4 4-4M5 19h14");
    icon.appendChild(path);
    button.appendChild(icon);
    button.appendChild(document.createElement("span"));
    setState(button, "idle", LABEL);
    button.addEventListener("click", () => {
      if (button.dataset.state === "busy") return;
      setState(button, "busy", "Sending to LID…");
      chrome.runtime.sendMessage({ type: "video", url: location.href }, (reply) => {
        void chrome.runtime.lastError;
        const ok = reply && reply.ok;
        setState(button, ok ? "ok" : "error", ok ? "Sent to LID" : "LID is not available");
        setTimeout(() => setState(button, "idle", LABEL), 2500);
      });
    });
    return button;
  }

  function update() {
    let button = document.getElementById(ID);
    if (!isVideoPage()) {
      if (button) button.remove();
      return;
    }
    if (!button) document.documentElement.appendChild(create());
  }

  document.addEventListener("yt-navigate-finish", update);
  window.addEventListener("popstate", update);
  update();
})();
