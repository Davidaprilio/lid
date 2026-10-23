/* A "Download with LID" button on YouTube video pages; LID then offers the resolutions.

   On the watch page the button sits in YouTube's own action bar (ytd-menu-renderer, next to Share).
   It is a clone of one of YouTube's buttons there, so it carries whatever classes YouTube uses today
   and keeps matching YouTube's look (and light/dark theme) without an extension update.
   Shorts, m.youtube.com and layouts the script does not recognise get a floating button instead. */
(function () {
  "use strict";
  const ID = "lid-download";
  const LABEL = "LID";
  const LONG_LABEL = "Download with LID";
  const TITLE = "Download with LID: choose the resolution or audio only";
  const SVG = "http://www.w3.org/2000/svg";
  const WAIT_TRIES = 20; // the action bar renders late; wait up to 10 s before falling back
  let tries = 0;
  let timer = 0;
  let observer = null;

  function isVideoPage() {
    return location.pathname === "/watch" || location.pathname.startsWith("/shorts/");
  }

  // The LID logo as a monochrome 24px glyph: an arrow over three download segments.
  function logo() {
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", "24");
    svg.setAttribute("height", "24");
    svg.setAttribute("aria-hidden", "true");
    svg.style.display = "block";
    svg.style.width = "100%";
    svg.style.height = "100%";
    const arrow = document.createElementNS(SVG, "path");
    arrow.setAttribute("d", "M12 3.5v9.5M8 9l4 4 4-4");
    arrow.setAttribute("fill", "none");
    arrow.setAttribute("stroke", "currentColor");
    arrow.setAttribute("stroke-width", "2");
    arrow.setAttribute("stroke-linecap", "round");
    arrow.setAttribute("stroke-linejoin", "round");
    svg.appendChild(arrow);
    [[3.5, 1], [9.75, 0.7], [16, 0.45]].forEach(([x, opacity]) => {
      const bar = document.createElementNS(SVG, "rect");
      bar.setAttribute("x", String(x));
      bar.setAttribute("y", "16.5");
      bar.setAttribute("width", "4.5");
      bar.setAttribute("height", "3");
      bar.setAttribute("rx", "1.5");
      bar.setAttribute("fill", "currentColor");
      bar.setAttribute("fill-opacity", String(opacity));
      svg.appendChild(bar);
    });
    return svg;
  }

  // Feedback colours for a button without a text label (YouTube's icon-only style).
  const STATE_COLORS = { busy: "", ok: "#2ba640", error: "#e5484d" };

  function wire(root, button, label, idleText) {
    const show = (state, text) => {
      root.dataset.state = state;
      if (label) {
        label.textContent = text;
      } else {
        button.title = state === "idle" ? TITLE : text;
        button.style.color = STATE_COLORS[state] || "";
        button.style.opacity = state === "busy" ? "0.6" : "";
      }
    };
    show("idle", idleText);
    // One click sends once: further clicks are ignored while sending and while the result shows.
    button.addEventListener("click", () => {
      if (root.dataset.state !== "idle") return;
      show("busy", "Sending…");
      chrome.runtime.sendMessage({ type: "video", url: location.href }, (reply) => {
        void chrome.runtime.lastError;
        const ok = reply && reply.ok;
        show(ok ? "ok" : "error", ok ? "Sent to LID" : "LID not found");
        setTimeout(() => show("idle", idleText), 2500);
      });
    });
  }

  // ------------------------------------------------------------ inside YouTube's action bar

  function iconPart(button) {
    return Array.from(button.children).find((c) => c.querySelector("svg"));
  }

  function textPart(button) {
    return Array.from(button.children).find((c) => !c.querySelector("svg") && !c.children.length &&
                                                   c.textContent.trim());
  }

  function insideLikeButtons(el, menu) {
    for (let node = el; node && node !== menu; node = node.parentElement) {
      if (/like/i.test(node.tagName)) return true;
    }
    return false;
  }

  // A visible button from the bar's button rows (Share, Download, Save…), not the like/dislike pair
  // nor the "More actions" menu. YouTube shows them with a text label or as round icon-only buttons;
  // one with a label is preferred so the clone gets one too.
  function template(menu) {
    const buttons = Array.from(menu.querySelectorAll(":scope > div button")).filter((b) =>
      !b.closest("#" + ID) && b.getClientRects().length && iconPart(b) && !insideLikeButtons(b, menu));
    return buttons.find(textPart) || buttons[0];
  }

  // The template's ancestor that is a direct item of a button row, for its spacing.
  function rowItem(button, menu) {
    let el = button;
    while (el.parentElement && el.parentElement !== menu && el.parentElement.parentElement !== menu) {
      el = el.parentElement;
    }
    return el;
  }

  function cloneButton(menu, tpl) {
    const item = rowItem(tpl, menu);
    const style = getComputedStyle(item);
    const wrapper = document.createElement("div");
    wrapper.id = ID;
    wrapper.style.display = "flex";
    wrapper.style.flex = "none";
    wrapper.style.marginLeft = style.marginLeft;
    wrapper.style.marginRight = style.marginRight;

    const button = tpl.cloneNode(true);
    for (const name of ["id", "title", "aria-pressed", "aria-expanded", "aria-haspopup", "disabled"]) {
      button.removeAttribute(name);
    }
    button.setAttribute("aria-disabled", "false");
    button.setAttribute("aria-label", LONG_LABEL);
    button.title = TITLE;
    button.type = "button";
    const icon = iconPart(button);
    const svg = icon.querySelector("svg");
    svg.parentNode.replaceChild(logo(), svg);
    const label = textPart(button);
    wrapper.appendChild(button);
    wire(wrapper, button, label, LABEL);

    const row = item.parentElement === menu ? item : item.parentElement;
    const flexible = menu.querySelector(":scope > #flexible-item-buttons");
    menu.insertBefore(wrapper, flexible || row.nextSibling);

    // YouTube may re-render the bar; put the button back when it disappears.
    if (observer) observer.disconnect();
    observer = new MutationObserver(() => {
      if (!wrapper.isConnected || wrapper.parentElement !== menu) refresh();
    });
    observer.observe(menu, { childList: true });
  }

  // ------------------------------------------------------------ fallback

  function floating() {
    const button = document.createElement("button");
    button.id = ID;
    button.className = "lid-floating";
    button.type = "button";
    button.title = TITLE;
    button.appendChild(logo());
    const label = document.createElement("span");
    button.appendChild(label);
    wire(button, button, label, LONG_LABEL);
    document.documentElement.appendChild(button);
  }

  // ------------------------------------------------------------

  function update() {
    clearTimeout(timer);
    const old = document.getElementById(ID);
    if (!isVideoPage()) {
      if (old) old.remove();
      return;
    }
    const watch = location.hostname === "www.youtube.com" && location.pathname === "/watch";
    const menu = watch && document.querySelector("ytd-watch-metadata ytd-menu-renderer");
    const tpl = menu && template(menu);
    if (tpl) {
      if (old && old.parentElement === menu && old.firstChild.className === tpl.className) return;
      if (old) old.remove();
      cloneButton(menu, tpl);
      return;
    }
    if (watch && ++tries < WAIT_TRIES) {
      timer = setTimeout(update, 500);
      return;
    }
    if (!old || !old.classList.contains("lid-floating")) {
      if (old) old.remove();
      floating();
    }
  }

  // Each navigation rebuilds the button from the bar YouTube shows now.
  function refresh() {
    tries = 0;
    const old = document.getElementById(ID);
    if (old && !old.classList.contains("lid-floating")) old.remove();
    update();
  }

  document.addEventListener("yt-navigate-finish", refresh);
  window.addEventListener("popstate", refresh);
  update();
})();
