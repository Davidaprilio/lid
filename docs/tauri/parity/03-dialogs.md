# LID dialogs: feature checklist for the Rust + Tauri rewrite

Base path: `lider/ui/dialogs/`. References are `file:line`. `mw:` means `lider/ui/main_window.py`, `w:` means `ui/widgets.py`, `core/...` means `lider/core/...`. Settings defaults come from `core/settings.py:113-172`.

**Before you plan against this, note four differences from the brief:**
- **Options has 7 tabs, not 6.** The 7th is "Appearance" (options.py:281).
- **There is no speed graph** anywhere in the UI. A grep for "graph" over every `.py` file finds nothing. The progress dialog only has the per-connection segment bar and a connection table.
- **Four features named in the brief live in `main_window.py`, not in `dialogs/`:** the grabber, batch from clipboard, .ef2/text import/export and clipboard monitoring. They reuse `LinksDialog`/`extract_links` from add.py. They are covered in §5 and §6.
- **`__init__.py` is empty** (0 lines).

---

## 0. Shared building blocks used by the dialogs

- [ ] **`shell(dialog, icon, title, subtitle, spacing, tile, badge)`** (w:99) builds the frame of every dialog:
  - an optional header with an icon (optionally a tile or a coloured badge), a title and a subtitle;
  - a padded body (margins 18/16/18/14);
  - a footer bar.
  - It exposes `dialog.header_icon`.
- [ ] **`tab_strip(names)`** (w:200): an underlined QTabBar on a strip, paired with a QStackedWidget. Used by Progress and Scheduler.
- [ ] **`ChecksumEdit`** (w:632-711)
  - **Controls:**
    - Algorithm combo, fixed width 124: "Auto-detect" (`auto`), then "MD5", "SHA-1", "SHA-256", "SHA-512" (`core/checksum.py:13`).
    - Monospaced hash field, placeholder "Paste a hash (optional)".
    - A pill inside the right edge of the field shows the detected type (e.g. "SHA-256"). Text margins leave room for it.
    - `message` label, default hint "Optional — LID checks the file when the download finishes." It turns into the red error text when the input is invalid.
    - The field gets the `invalid` property (red style) while the input is invalid.
    - Emits `changed` on every edit or algorithm change.
  - **`setValue("sha256:<hex>")`** resets the algorithm to Auto-detect and shows only the hex, cursor at 0.
  - **`value()`** returns `"algo:hex"` or `""`, and raises `ChecksumError` while invalid.
  - **Parsing rules** (`core/checksum.py:23-52`):
    - Leading and trailing whitespace is stripped; empty input gives `""`.
    - An optional prefix `md5|sha1|sha-1|sha256|sha-256|sha512|sha-512` followed by `:` or `=` (case-insensitive) is accepted.
    - Only the first whitespace-separated token is taken. That is how a SHA256SUMS line `<hex>  name` or `<hex> *name` works: the name is dropped.
    - The value is lower-cased.
    - Hex only. Error: "Not a valid checksum — “x” is not a hex character."
    - A prefix with no value. Error: "…there is no value after the prefix."
    - The prefix conflicts with the forced algorithm. Error: "The value is marked SHA-1 but SHA-256 is picked."
    - Auto-detect works by length: 32 = md5, 40 = sha1, 64 = sha256, 128 = sha512. Any other length: "expected 32, 40, 64 or 128 hex characters, got N."
    - With a forced algorithm the length must match: "Not a valid SHA-256 value — expected 64 hex characters, got N."
- [ ] **`TagInput`** (w:335-430)
  - Values appear as removable badges.
  - Enter or a comma adds the typed value. Backspace on an empty field removes the last badge.
  - Each value goes through a normaliser:
    - `extension_tag`: strip, remove a leading ".", lower-case (w:487).
    - `site_tag`: strip, lower-case (w:491).
  - `meta_row()` shows "Enter or , adds · Backspace removes the last", then "N types"/"N sites", then a "Clear all" ghost button.
  - `text()` is the space-joined values. `set_values()` accepts a list or a space-separated string.
- [ ] **`CheckHeader`**: a tri-state check-all box in column 0 of a tree header. Clicking it checks everything, or unchecks everything if all are already checked.
- [ ] **Style helpers:** `pill_label`/`set_pill` give coloured pills (tones `ok`, `muted`, `accent`, `bad`). `primary()` gives the accent button style. `panel()` is a framed box. `caption()` is an upper-case small label. `note()` is a hint, optionally boxed. `form_row(label, widgets…, width=)` is a label column of fixed width followed by widgets.

---

## 1. add.py

### 1.1 Module helpers
- [ ] **`URL_RE`** `^(https?|ftp)://\S+$`, case-insensitive (add.py:47). It validates every address in the app.
- [ ] **`link_combo`** (add.py:50-55): an editable combo with a leading "link" icon and a monospaced font set through the QSS `mono` property.
- [ ] **`clipboard_url()`** (add.py:58-60): the trimmed clipboard text if it matches `URL_RE`, else `""`. Used for pre-fill.
- [ ] **`queue_menu(parent, queues, at)`** (add.py:63-68): a popup menu listing the queue names. Returns the chosen name or None.

### 1.2 AuthBox: "Use authorization" (add.py:71-116)
- [ ] **Check box** "Use &authorization", default unchecked.
- [ ] **Fields:** "Login" (QLineEdit) and "Password" (QLineEdit, password echo), on one row.
- [ ] **Two modes:**
  - Panel mode (Add URL): the fields are always visible and disabled until the box is checked.
  - `compact=True` (Batch): the fields are hidden until the box is checked, and enabled once it is (add.py:103-107).
- [ ] **`credentials()`** returns `(login, password)` only when checked, else `("", "")` (add.py:115).

### 1.3 AddUrlDialog: "Enter new address to download" (add.py:119-177)
- [ ] **Frame:**
  - Window title "Enter new address to download". Minimum width 560.
  - Header icon `add-url`. Subtitle "Paste a link. LID reads the file name and size next." (add.py:126-129)
- [ ] **"Address" field** (add.py:130-136)
  - Editable combo with a link icon.
  - Items are the class-level history, which is shared for the whole process.
  - Pre-filled with the `url` argument, else with `clipboard_url()`. The text is selected.
- [ ] **History:** most recent first, de-duplicated, capped at 20 entries (add.py:173-176). It lives in memory only and is not persisted.
- [ ] **YouTube hint row** (add.py:137-145): a video icon plus "**YouTube video detected** — choose the resolution or audio only in the next window."
  - It shows only when `media.is_youtube(text)` is true, and the AuthBox is hidden while it shows (add.py:159-162).
  - The check re-runs on every text change.
- [ ] **AuthBox** in panel mode (add.py:146).
- [ ] **Buttons:**
  - "O&K": primary and the default button.
  - "&Cancel" rejects.
- [ ] **Validation on OK:** the URL must match `URL_RE`. If not, focus returns to the field, its text is selected and the dialog stays open (add.py:167-172).
- [ ] **Outputs:** `url()` (trimmed) and `auth.credentials()`. The caller passes these to probe and then the File Info dialog (mw:781-842).
- [ ] **Caller flow** (mw):
  - Duplicate check (§1.5).
  - A YouTube URL goes to the yt-dlp probe. Without yt-dlp the warning is "Downloading from YouTube needs the yt-dlp package: pip install yt-dlp". Other errors: "Cannot read this YouTube video:…" and "…has no downloadable formats".
  - A normal URL is probed asynchronously with a busy cursor. On failure the question is "Cannot get information about the file:\n{error}\n\nAdd it anyway?", followed by size -1 and resumable unknown.
  - A browser-supplied filename that contains "." wins over a probed name that has no extension.

### 1.4 FileInfoDialog: "Download File Info" (add.py:180-422)
- [ ] **Frame:** window title "Download File Info". Width minimum 720, maximum 1100. Header icon "folder" (a tile), and the subtitle is the filename (add.py:206-209).
- [ ] **Result dataclass `FileInfoResult`:** `action` (`start`|`later`), `path`, `category`, `description`, `queue`, `variant`, `checksum` (`"sha256:<hex>"` or `""`) (add.py:180-188).
- [ ] **"URL" field** (add.py:211-213, 244)
  - Monospaced QLineEdit, cursor at 0 so the start of the address shows.
  - Read-only when this is a YouTube item.
- [ ] **"Category" combo** (add.py:214-217, 312-315)
  - Lists every `settings.categories` entry with its icon. Minimum width 200.
  - The initial value is `categories.classify(settings, filename, url)`.
- [ ] **"+" button** (tooltip "Add category") (add.py:218-222, 392-395)
  - Calls the `new_category` callback, which opens `CategoryDialog(None)`.
  - If the name is new, it is appended to `settings.categories`, settings are saved and the tree is rebuilt (mw:586-595).
  - The new category is added to the combo and selected.
- [ ] **"Save As" combo** (add.py:223-226, 378-390)
  - Editable, minimum contents length 30.
  - On every category change it is rebuilt: item 0 is `cat.save_dir/<basename>`, followed by `other.save_dir/<basename>` for every other category with a different folder.
  - The basename comes from the current text, or from the filename when the text is empty.
- [ ] **"..." button** (add.py:227-230, 397-400): a native Save dialog titled "Save As", starting at the current path.
- [ ] **"Remember this path" check box** (add.py:231, 382)
  - Label: `Remember this path for "<category>" category`; it updates with the category.
  - Checked at finish: the category's `save_dir` is set to the folder of the chosen path, which edits Settings.categories (add.py:413-414). The caller then calls `save_settings()` (mw:913).
- [ ] **"Description" field** (add.py:232, 246): pre-filled from the `description` argument (link text, etc.).
- [ ] **"Checksum" `ChecksumEdit`**, plus its message line under the field (add.py:247-258)
  - Hidden for YouTube: a merged file has no published checksum.
  - Clipboard pre-fill: when `settings.checksum_from_clipboard` is true, `checksums.parse(clipboard)` fills it. Parse errors are ignored silently.
- [ ] **Side column, 116 px** (add.py:260-286)
  - A 72×72 tile with the category icon (39 px).
  - Size label: bold, monospaced, `fmt.size(size)` or "(unknown)".
  - When `resumable` is not None: the pill "✓ Resumable" (ok) or "Not resumable" (muted).
  - The header icon also follows the category (22 px) (add.py:380-381).
- [ ] **YouTube box**, only when `media_info` is given (add.py:324-366)
  - Caption "YouTube video", the bold title (HTML-escaped, word-wrapped), and a muted meta line "uploader · length".
    - Length is `H:MM:SS` when there are hours, else `M:SS`. It is omitted when the duration is 0.
  - Radio buttons "Video" and "Audio only". Each is enabled only if a variant of that kind exists.
  - "Quality" combo listing the variants of the chosen kind by `label`.
  - **Initial kind:** the `media_kind` argument if variants of that kind exist, else the kind of the first variant (add.py:316-320).
  - **Initial quality:** `media_info.default_variant(kind, prefer=media_quality)`. That is an exact quality match; otherwise, for video, the first mp4 ≤1080p, else the first variant (`core/media.py:102-113`).
  - **Switching kind** refills the quality list and auto-selects the category "Video" or "Music" if that category exists (add.py:363-365).
  - **On a quality change** (add.py:368-376):
    - The size label becomes the variant size, prefixed with "~" when the variant converts to mp3.
    - The Save As extension is replaced with `variant.ext`.
    - The path list is rebuilt.
  - The caller stores the chosen `media_kind`/`media_quality` in Settings for next time (mw:870-871). The defaults are `media_kind="video"` and `media_quality=""`.
- [ ] **Buttons** (add.py:294-310)
  - **"Download &Later"** (queue icon) pops up `queue_menu` under the button. Choosing a queue finishes with `action="later"` and that queue (add.py:402-406).
  - **"&Start Download"** (primary, add-white icon, default button, gets the initial focus) finishes with `action="start"`.
  - **"&Cancel"** rejects.
  - **An invalid checksum disables both Later and Start** until it is fixed or cleared (add.py:308-309).
- [ ] **Finish validation** (add.py:408-422)
  - An empty Save As path: nothing happens.
  - A checksum error: focus goes to the checksum field.
  - Otherwise the result is built and the dialog accepts.
- [ ] **Caller behaviour** (mw:893-922)
  - The dialog shows only if `settings.show_start_dialog` is true. Otherwise start is `not settings.only_add_to_queue` with the auto category and folder.
  - The name is made unique with `_unique_name`: `stem (2).ext`, `stem (3).ext`… against both downloads in the same folder and existing files (mw:877-884).
  - With "Later" the download goes to the chosen queue without starting.
  - With "Start" the progress dialog opens.

### 1.5 DuplicateDialog: "Duplicate download link" (add.py:425-462)
- [ ] **Frame:** window title "Duplicate download link". Minimum width 600. Header icon "queue". Subtitle "This file is already in your download list."
- [ ] **URL field:** read-only, no focus, with the link style.
- [ ] **Radio group** (the first is checked by default):
  1. `number`: "Add the duplicate with a numbered file name"
  2. `overwrite`: "Add the duplicate and overwrite the existing file"
  3. `complete`: "If existing file is complete, show download complete dialog; otherwise resume it"
- [ ] **Check box** "Remember my selection and don't ask again (change it in Options → Downloads)". When it is checked, the choice is written to `settings.duplicate_action` (mw:802-804).
- [ ] **Buttons:** "OK" (primary, default) and "Cancel". Cancel aborts the add.
- [ ] **Shown only when** `settings.duplicate_action == "ask"` (the default) and `manager.find_duplicate(url)` finds a match (mw:792-797).
- [ ] **What each choice does** (mw:805-812, 914-917, 873-876):
  - `complete`: if the existing download is complete, a CompleteDialog opens. Otherwise the existing download is started and its progress dialog shown.
  - `overwrite`: the old record is removed. Its file is deleted too, but only if it is complete and the path is the same.
  - `number`: the add proceeds with a unique name.

### 1.6 BatchDialog: "Batch download" (add.py:465-585)
- [ ] **Frame:**
  - Window title "Batch download". Minimum width 620. Header icon `add-url`.
  - Subtitle "Add a numbered series of files: put one asterisk in the address, e.g. https://www.example.com/pictures/img*.jpg".
- [ ] **"Address":** an editable link combo, pre-filled from `clipboard_url()`.
- [ ] **"Replace asterisk with" panel**
  - Radio "Numbers" (default) or "Letters".
  - **Numbers mode:** "From" spin box (0–999999, default 1) and "To" spin box (0–999999, default 10).
  - **Letters mode:** "From" line edit (max 1 char, default "a") and "To" line edit (max 1 char, default "z").
  - "Wildcard size" spin box, 1–9, default 1. It is enabled only in Numbers mode.
  - All spin boxes have no arrow buttons, are 80×30 and monospaced.
  - Visibility: the number spins show only in Numbers mode and the letter edits only in Letters mode (add.py:575-580).
- [ ] **AuthBox** in compact mode (add.py:522).
- [ ] **Preview panel:** "First file", "Second file" and "Last file", each a monospaced, selectable link-styled label (add.py:525-538).
- [ ] **Footer count label:** "N files will be added", or "Put one * in the address" when no URL can be built (add.py:585).
- [ ] **Buttons:** "OK" (primary, default) and "Cancel".
- [ ] **Expansion rules for `urls()`** (add.py:558-573)
  - The pattern must contain exactly one `*`; otherwise the list is empty.
  - **Numbers:** the range runs from `from` to `to` inclusive. If `to < from`, it counts down (step -1). Each number is zero-padded to "wildcard size" (`zfill`).
  - **Letters:** the alphabet is `a–z` followed by `A–Z`, in that order. Both ends must be letters, otherwise the list is empty. The range is taken between `min(i, j)` and `max(i, j)` inclusive, so it never reverses. Empty edits default to a and z.
  - The `*` is replaced with each value.
- [ ] **Live updates:** the preview recomputes on any change of address, mode, spins or letters.
- [ ] **On OK** (mw:924-928)
  - Every URL is added straight to the main queue through `_add_many`: auto category and folder, a unique name, and the batch credentials as username and password.
  - No file-info dialog is shown, and nothing is auto-started.
  - An empty list adds nothing.

### 1.7 Link extraction (add.py:590-622)
- [ ] **`LinkParser`** (an HTMLParser) collects:
  - `<a href>`: the absolute URL resolved with `urljoin(base)`, and the link text with its whitespace collapsed. Kind `a`.
  - `img`, `source`, `video`, `audio` and `embed` with a `src`: the absolute URL, the `alt` text, and the tag name as the kind.
- [ ] **`extract_links`** keeps only the http, https and ftp schemes.
- [ ] **Extension sets:**
  - `HTML_EXT` = {"", htm, html, php, asp, aspx, jsp, shtml, cgi}.
  - `IMAGE_EXT` = {jpg, jpeg, png, gif, webp, svg, bmp, ico}. It is defined but not used inside this file.

### 1.8 LinksDialog: "Download All Links" (add.py:625-782)

This dialog is used by the grabber, batch from clipboard and the browser "Download all links" command.

- [ ] **Frame:** window title "Download All Links". Size 840×640. Header icon "grabber". Subtitle "N links found · check the ones to add".
- [ ] **Tree**
  - Columns: check, "FILE", "TYPE", "ADDRESS".
  - A CheckHeader check-all box: it checks all, or unchecks all if everything is already checked.
  - Extended selection and alternating rows. Column 0 is fixed at 34 px, TYPE is 70 px and rendered as a pill (`TypePillDelegate`), ADDRESS stretches.
  - **Each row** (add.py:746-755):
    - The name is `safe_filename(filename_from_url(url))`, with a category icon.
    - The name tooltip is the link text.
    - TYPE is the upper-case extension.
    - ADDRESS is the URL without its scheme, monospaced and muted, with the full URL as the tooltip.
  - **Initial check state:** checked when the name matches the auto-capture file types (`categories.is_auto_capture_type`).
  - After a refill the FILE column is resized to its contents.
- [ ] **Buttons:** "Check All" and "Uncheck All".
- [ ] **Filter check boxes**, each refilling the list when toggled:
  - "Hide HTML files", default **on**. It hides kind `a` links whose extension is in `HTML_EXT`, including links with no extension.
  - "Hide images", default off, tooltip "Hide images located on this web page". It hides kind `img` only.
  - "Hide repeated files", default **on**. It drops URLs already seen.
- [ ] **"Save to" panel**
  - Radio "Every file to the folder of its category" (default).
  - Radio "All files to one category", plus a category combo (all categories).
  - Radio "All files to one directory", plus a directory edit (default: the folder of the first category) and "Browse...".
  - Browse opens a folder dialog titled "Save To". Choosing a folder also selects "one directory" (add.py:763-767).
- [ ] **Check box** "Use link texts as download descriptions", default **on**.
- [ ] **Footer:** an accent pill "N checked" that updates live, "OK" (primary, default) and "Cancel".
- [ ] **`choices()`** (add.py:769-782) returns a `LinkChoice` (url, filename, description, category, save_dir) for each checked row.
  - The category is either the one picked in "one category" mode, or the auto-classified one.
  - `save_dir` is the directory edit in "one directory" mode, else the category folder.
  - The description is the link text when "Use link texts" is on, else empty.
- [ ] **Caller** (`_add_choices`, mw:971-978): each choice gets a unique name and is added to the main queue with a referer and parent page. When there is at least one choice, the main queue is started.

---

## 2. browsers.py: BrowserIntegrationDialog, "Browser integration" (browsers.py:32-166)
- [ ] **Frame:** window title "Browser integration". Minimum width 680. Header icon "connection". Subtitle "LID talks to its extension through native messaging — no network port is opened."
- [ ] **Tree**
  - Columns: check, "BROWSER", "LID HELPER". Column 0 is 34 px fixed, column 1 is 260 px, and the last section stretches. Minimum height fits 5 rows.
  - Filled from `browser.BROWSERS`:
    - The state is "installed" when `browser.is_installed(b)`, else "not installed" when the browser was detected, else "browser not found".
    - Rows for browsers that were not detected are disabled and unchecked. Detected browsers start checked (browsers.py:134-146).
  - The LID HELPER column is a pill: installed = ok "Installed", not installed = muted "Not installed". "browser not found" is not in the map, so it falls through as plain text.
  - The header check-all box ignores disabled rows (browsers.py:120-132).
- [ ] **Buttons**
  - **"Install for checked browsers"** (primary) calls `browser.install(checked)`. It fills in the extension folder path for each family it installed, then refreshes the tree.
  - **"Remove"** calls `browser.uninstall(checked)` and refreshes the tree.
- [ ] **Panel "Then add the LID extension to the browser"**, one block per family (browsers.py:23-29, 68-91)
  - The chromium step text: "**Chrome, Chromium, Brave, Edge, Vivaldi, Opera:** open `chrome://extensions`, turn on *Developer mode*, click *Load unpacked* and choose the folder below."
  - The firefox step text: "**Firefox:** open `about:debugging#/runtime/this-firefox`, click *Load Temporary Add-on* and choose `manifest.json` in the folder below. Then allow it to access all websites in the add-on's permissions."
  - Each family has a read-only monospaced folder field (placeholder "Install for a browser first"), a "Copy" button (copies to the clipboard) and an "Open folder" button (`open_path`).
- [ ] **"Get the extension:" link buttons**, each with its URL as the tooltip, opened in the system browser (browsers.py:93-111)
  - "Chrome Web Store", if `CHROME_WEB_STORE_URL` is set.
  - For Firefox, the first that applies:
    - "Firefox Add-ons", if `FIREFOX_ADDONS_URL` is set;
    - otherwise "Firefox (.xpi)", if `FIREFOX_SIGNED_XPI` is set;
    - otherwise "Firefox (.zip)".
  - "All downloads" (`RELEASES_URL`).
- [ ] **Footer:** "Close" (default) rejects.
- [ ] **Opened from:** Options → General → "Set up browsers..." (options.py:336-351), and from the main window (mw:465).

---

## 3. checkhash.py: CheckHashDialog, "Check hash" (checkhash.py:51-361)
- [ ] **When the menu item is enabled** (`can_check`): status is COMPLETE or VERIFYING, or `downloaded > 0`, or the download is running (checkhash.py:36-39).
- [ ] **Frame**
  - Window title "Check hash". Minimum width 640. Header icon "checksum" with an accent badge.
  - The subtitle is "`name · size`" for a finished file or an unknown size, else "`name · <downloaded number> of <size>`" (checkhash.py:64-68).
- [ ] **"Expected checksum" ChecksumEdit**
  - Pre-filled with `d.checksum`.
  - Its hint for a finished file (`READY_HINT`): "Pre-filled with the checksum stored for this download. Paste the value published by the site, or leave it empty to only compute the file's hash."
  - Its hint for a running download (`RUNNING_HINT`): "Paste the value published by the site. Computing a hash without a value is available once the file is complete."
- [ ] **Check box "Save this checksum to the download"**, default on. It is enabled only when the value is valid, not empty, and the state is ready or reading (checkhash.py:269).
- [ ] **Running note** (accent box with a clock icon): "The file is still downloading (NN%). LID checks it as soon as the download finishes." The percentage appears only when the size is known.
- [ ] **Reading block**
  - Bold "Reading the file… NN%".
  - Muted monospaced "X of Y".
  - A progress bar (0–1000).
  - A "Stop" tool button: `verifier.skip()`, nothing is saved.
  - A faint note "SHA-256 · runs in the background; the list stays usable."
  - Polling runs every 100 ms (checkhash.py:285-306).
- [ ] **Match box**
  - Shield icon and "Matches — the file is intact".
  - On the right, "SHA-256 · 1.2 s".
  - Caption "COMPUTED SHA-256", then the value field and a "Copy" button.
  - Result note: "Saved to the download — the list shows ✓ Verified." or "Not saved — the download is unchanged."
- [ ] **Mismatch box**
  - "Expected" (selectable monospaced) and "Actual" (selectable monospaced, red).
  - Note: "SHA-256. Expected is the value above; Actual is computed from the file on disk." When saved, it adds " Saved to the download — the list shows Checksum mismatch."
- [ ] **Computed box** (no expected value given)
  - Caption "SHA-256 OF THE FILE", then the value field and "Copy".
  - Note: "No expected value, so there is no verdict and nothing is saved. Pick MD5, SHA-1 or SHA-512 above to compute another type."
- [ ] **Missing file line:** "File not found at <path>". It shows in the ready state when the file is finished but missing. A read error replaces it with "Could not read the file: <err>" (checkhash.py:193-196, 337-341).
- [ ] **States and visible widgets** (checkhash.py:241-264)

  | State | Visible | Default button |
  |---|---|---|
  | `ready` | save, Close, Check now | Check now |
  | `reading` | save, reading block, Close, Check now | (unchanged) |
  | `match` | save, match box, note, "Check again", primary "Close" | primary Close |
  | `mismatch` | save, mismatch box, "Delete file" (danger), "Download again" (primary), "Keep file" | Download again |
  | `computed` | save, computed box, note, "Check again", primary "Close" | primary Close |
  | `running` | running note, Close, "Check when finished" | Check when finished |

  - The initial state is `ready` for a finished file, `running` otherwise.
  - The field is enabled in every state except `reading`.
  - The default button also gets the focus.
- [ ] **Run button text and enablement** (checkhash.py:270-272)
  - It reads **"Compute hash"** when the field is empty and **"Check now"** otherwise.
  - It is enabled only when the value is valid, the state is ready, and the file exists.
  - "Check when finished" is enabled when the value is valid and not empty.
- [ ] **Editing the field while a result is shown** returns the dialog to `ready` (checkhash.py:273-274).
- [ ] **Algorithm choice** (checkhash.py:277-283)
  - With a value: the value's algorithm.
  - Without a value: the combo's algorithm, or SHA-256 when it is Auto-detect.
  - The Verifier is started with `value` or `"algo:"` (compute only).
- [ ] **Saving:** happens when "save" is checked, a value was given and the result is verified or mismatch. It calls `manager.record_check(id, value, result, actual, seconds)` (checkhash.py:311-313).
- [ ] **Buttons**
  - "Check again" returns to `ready`.
  - "Close" (both variants) rejects.
  - "Check when finished" calls `manager.check_when_finished(id, value)` and accepts.
  - "Delete file", "Download again" and "Keep file" set `choice` to delete, again or keep, then accept.
- [ ] **Caller handling of the choice** (mw:1403-1438)
  - `again`: `manager.redownload(id, overwrite=True)` and the progress dialog opens.
  - `delete`: the file is moved to the Trash (`QFile.moveToTrash`) and the download is removed and unchecked.
  - `keep`: nothing happens.
- [ ] **Closing while reading** stops the verifier (skip, join for 2 s) and stops the timer (checkhash.py:355-361).

---

## 4. closing.py: CloseWhileDownloadingDialog (closing.py:13-70)
- [ ] **Frame:** window title "LID". Minimum width 560. Header icon "queue" with an accent badge.
- [ ] **Header title:** "N downloads are still running" or "1 download is still running".
- [ ] **Subtitle** ("Exiting" when exiting, else "Closing the window"):
  - When background is not offered: "{what} stops them/it. Unfinished downloads can be resumed later."
  - When background is offered: "{what} stops them/it. LID can keep downloading in the background, with an icon in the system tray."
- [ ] **Panel:** the first 3 running downloads (`SHOWN = 3`), each as category icon, filename, and the muted "NN%" (or the downloaded size when the total is unknown). Then a faint "and N more" when there are more.
- [ ] **Buttons**
  - "Stop and close", or "Stop and exit" when exiting. Danger style, placed on the left. Choice `stop`.
  - "Continue in background", primary, only when `background=True`. Choice `background`.
  - "Keep downloading", the default button with focus. Choice `keep`.
  - Dismissing the dialog (Esc or the window close) also gives `keep`.
- [ ] **Caller rules** (mw:1548-1561)
  - "Running" includes downloads that are being verified.
  - Background is offered except on a real Exit when a tray icon exists.
  - Choosing `background` turns `settings.tray_icon` on and hides the window to the tray (mw:1563+).

---

## 5. find.py

### 5.1 FindDialog: "Find file" and "LID Search settings" (find.py:24-92)
- [ ] **Two modes**
  - Title "Find file" with the query field (mw:1144).
  - Title "LID Search settings" with `show_query=False`: no query field, and the button reads "OK" (mw:1131).
- [ ] **Controls**
  - Caption "Find" and a query field with a search icon, pre-filled with the last query (Setting `search_string`) and selected.
  - Panel "Look in":
    - "File name or part of the name" (`TARGET_NAME` 0x04)
    - "Description or part of the description" (`TARGET_DESCRIPTION` 0x08)
    - "Site name / download link / parent web page / referer" (`TARGET_LINKS` 0x10)
  - A row with "Match case" (`MATCH_CASE` 0x01) and "Match whole string only" (`WHOLE_STRING` 0x02).
  - The initial state comes from the Setting `search_flags`, default `0x04` (file name only).
  - Footer hint, only in Find mode: "Ctrl+F · F3 finds next".
- [ ] **Buttons:** "Find" (or "OK"), primary and default; "Cancel".
- [ ] **Validation** (find.py:75-92, `core/search.py:59-64`)
  - At least one target is required. Otherwise an info box titled "Search settings" says "The checkbox for at least one of the search targets must be enabled", the "File name" box is re-checked, and the dialog stays open.
  - On success it writes the flags and the query into `SearchSettings`.

### 5.2 SearchBar: inline bar in the main window (find.py:95-130)
- [ ] **Layout:** "Find:" label, a line edit with a clear button, a primary "Find" button, "Settings..." (tooltip "LID Search settings") and a close tool button on the right (tooltip "Close SearchBar").
- [ ] **Signals**
  - Enter or Find emits `find_requested`.
  - Settings... emits `settings_requested`.
  - Close or **Esc** emits `closed`.

---

## 6. Features in main_window.py that use add.py dialogs (named in the brief)

- [ ] **Batch from clipboard**, menu "Add batch download from &clipboard" (mw:149, 930-938)
  - Splits the clipboard on whitespace and keeps the words that match `URL_RE`, all with kind `a`.
  - None found: the message "Cannot find any web address in the clipboard!" (title "LID").
  - Otherwise LinksDialog opens, and the choices are added with no referer.
- [ ] **Site grabber**, menu "Run site &grabber" (toolbar "Grabber", tooltip "Run site grabber") (mw:150, 328, 940-969)
  - An input dialog titled "Grabber" asks "Enter the address of the web page to explore:". It is pre-filled with the clipboard text if that is a URL, else "https://".
  - Cancel or an invalid URL aborts.
  - A busy cursor shows while the page is fetched asynchronously through `manager.client().open(url, ranged=False)`. The fetch follows redirects (it uses `geturl()` as the base), reads at most **8 MiB**, and decodes with the response charset or UTF-8 with replacement.
  - A fetch error shows "Cannot open the web page:\n{err}". No links shows "Cannot find the links!".
  - Otherwise LinksDialog opens. Choices are added with referer and parent page set to the base URL.
  - **The grabber is single-page:** there is no recursive crawl, no depth setting and no filter rules. It extracts the a, img, source, video, audio and embed tags of that one page.
  - The tree shows a "Grabber projects" node (mw:531), but it has no project functionality.
- [ ] **Browser "Download all links"** (mw:1001-1007): items `{url, text, kind}` from the extension go to LinksDialog, with the cookies and user-agent passed as headers.
- [ ] **Export to .ef2**, "To .ef2 export &file" (mw:151, 1301-1310)
  - A Save dialog titled "Export download list", default name `downloads.ef2`, filter "Export files (*.ef2)".
  - The file is UTF-8 with **CRLF** line endings.
  - Downloads are written sorted by id, one block each: a line `<`, then the URL line, then `referer: <ref>` if there is a referer, then a line `>`.
- [ ] **Export to text**, "To &text file" (mw:152, 1312-1316): `downloads.txt`, filter "Text files (*.txt)", one URL per line sorted by id, with a trailing newline, in UTF-8.
- [ ] **Import from .ef2**, "&From .ef2 export file" (mw:153, 1319-1340)
  - Filters "Export files (*.ef2);;All files (*)". The file is read as UTF-8 with replacement.
  - **Parser:**
    - Lines are trimmed.
    - A line `<` opens a block and `>` closes it.
    - Inside a block, the first non-empty line is the URL. A later `referer:` line, matched case-insensitively, gives the referer.
    - Other header lines are ignored.
  - Each item is added through `_add_many` (auto category, unique name, main queue, not started).
- [ ] **Import from text**, "From &text file" (mw:154, 1342-1347): every whitespace-separated word that matches `URL_RE` is added through `_add_many`.
- [ ] **Clipboard monitoring** (Options → General "Automatically start downloading of URLs placed to clipboard", Setting `monitor_clipboard`, default True) (mw:1479-1495)
  - A new clipboard text that matches `URL_RE` and differs from the last one triggers it.
  - It is skipped when the host ends with an entry of `no_auto_sites` (space-separated).
  - YouTube URLs and URLs whose filename is an auto-capture type are passed to `add_url(..., ask_address=False)`.

---

## 7. menu.py: AddToMenuDialog (menu.py:15-50)
- [ ] **Frame:**
  - Window title "LID". Minimum width 520. Header icon "general" with an accent badge.
  - Header title "Add LID to the application menu?".
  - Subtitle "{AppImage file name} runs on its own, so the application menu does not list LID yet." The name falls back to "LID".
- [ ] **Boxed note:** "LID keeps the entry pointed at the AppImage you start, also after you move or update it, and it disappears when the AppImage is deleted. You can change this later in Options → General."
- [ ] **Buttons:** "Not now" rejects. "Add to menu" (primary, default) accepts and calls `desktop.add_to_menu()`; an OSError is ignored.
- [ ] **`should_offer`** is true only when running as an AppImage and none of these hold: `settings.menu_offered`, already in the menu, a system package is installed, listed elsewhere (menu.py:37-39).
- [ ] **`offer`** sets `settings.menu_offered = True` and saves the settings **before** showing the dialog, so the offer is made once whatever the answer (menu.py:42-50).

---

## 8. options.py

### 8.1 Helpers
- [ ] **`_browse_dir`** opens a folder dialog titled "Select a Folder" and sets the edit's text (options.py:46-49).
- [ ] **`_ok_cancel`** adds the footer "Cancel" and "OK" (primary, default) (options.py:52-59).

### 8.2 CategoryDialog: "LID categories" (options.py:62-114)
- [ ] **Frame:** window title "LID categories". Minimum width 640. The header has the category icon (or "folder" for a new category) in a tile. Subtitle "New category" or "Edit the {name} category".
- [ ] **A new category defaults to:** name "", no extensions, folder `~/Downloads/New`, no sites (options.py:70).
- [ ] **"Category name"** QLineEdit. It is **read-only for "General"** (options.py:100-101).
- [ ] **Label** "Automatically put in this category the following file types:", then a TagInput (placeholder "Add a type, e.g. mkv", extension normaliser, noun "types") and its meta row.
- [ ] **Check box** "Automatically put in this category the files from these sites:"
  - It starts checked when the category has sites.
  - It enables a sites TagInput (placeholder "Add a site, e.g. *.example.com", site normaliser).
- [ ] **Label** "Save future downloads of this category to:", then a monospaced folder edit and "Browse...".
- [ ] **Check box** "Remember last save path", from `cat.remember_last`.
- [ ] **OK** accepts only when both the name and the folder are non-empty after trimming. Otherwise it does nothing.
- [ ] **Result:** `Category(name, exts, folder, sites if the box is checked else [], remember_last)`.

### 8.3 ServerLimitDialog: "Max. connections number for a server" (options.py:117-140)
- [ ] **Frame:** minimum width 480, no header.
- [ ] **"Server":** a monospaced edit, prefilled with the pattern, plus a faint hint "You may use an asterisk as a wildcard."
- [ ] **"Max. connections number":** a combo of `CONNECTION_CHOICES` (1, 2, 4, 8, 16, 24, 32). The default is 2 for a new entry.
- [ ] **Buttons:** OK and Cancel.
- [ ] **`value()`** returns `(pattern.strip(), int)`.

### 8.4 SpeedLimiterDialog: "Speed Limiter settings" (options.py:143-168)
- [ ] **Frame:** minimum width 520. Header icon "speed" with an accent badge. Subtitle "Applies to all downloads while the limiter is on".
- [ ] **"Maximum speed":** a spin box, 1–10,000,000, followed by the unit "KBytes/sec". Setting `speed_limit_kbps`, default 200.
- [ ] **Boxed note:** "Note: servers may close the connection when the speed is very low."
- [ ] **Check box** "Always turn on Speed Limiter on LID startup". Setting `speed_limiter_on_startup`, default False.
- [ ] **OK** writes both settings (options.py:165-168). It is opened from the main window (mw:1198).

### 8.5 ColumnsDialog: "Columns" (options.py:171-261)
- [ ] **Frame:** minimum width 440. Intro "Check the columns to show. Drag or use the buttons to change the order."
- [ ] **Check list**
  - Each item uses the title from `DEFAULT_COLUMNS`, is user-checkable, and is 30 px high.
  - Drag-and-drop reordering (InternalMove). The list is tall enough for 12 rows.
  - It is filled from the Setting `columns` (list of `[key, visible, width]`).
- [ ] **Default columns** (key, title, visible, width):
  - queue, "Q", visible, 34
  - filename, "File Name", visible, 280
  - size, "Size", visible, 92
  - status, "Status", visible, 180
  - time_left, "Time left", visible, 100
  - rate, "Transfer rate", visible, 120
  - last_try, "Last Try Date", visible, 130
  - description, "Description", visible, 200
  - date_added, "Date Added", hidden, 130
  - save_to, "Save To", hidden, 200
  - referer, "Referer", hidden, 200
  - parent_page, "Parent Web Page", hidden, 200
- [ ] **Side buttons**
  - "Move up" and "Move down" move the current row by one.
  - "Reset" (ghost, tooltip "Back to the default columns") **immediately writes the default columns and accepts the dialog** (options.py:246-248).
- [ ] **"Selected column width":** a spin box, 10–2000 px. It shows the current item's width, and editing it changes that item's width (options.py:224-232).
- [ ] **On OK** it writes `settings.columns` in list order. **"filename" is always forced visible** (options.py:250-261).
- [ ] `_check` (options.py:241) is dead code with no caller.

### 8.6 OptionsDialog: "LID options" (options.py:264-744)

**Frame**
- [ ] Window title "LID options". Size 720×712. A QTabWidget named `optionsTabs` with **7 tabs**, in this order: General, File types, Save to, Downloads, Connection, Proxy / Socks, Appearance. The footer has Cancel and OK.
- [ ] Each page has a page header (icon and title), a separator line, and spacing.
- [ ] **The dialog edits copies:** working copies of the categories and `server_limits` (options.py:268-270). Nothing is written until OK, and Cancel discards everything.

**Tab 1: General** (header "Desktop Integration", icon general) (options.py:311-346)
- [ ] **"Launch LID on startup"**
  - Initial state: `desktop.autostart_enabled()`.
  - On OK: the Setting `launch_on_startup` is written. `desktop.set_autostart()` runs only if the value differs from the current state.
- [ ] **"Show LID in the application menu"**
  - When `desktop.system_installed()`: checked, **disabled**, tooltip "Added by the installed LID package".
  - Otherwise the initial state is `desktop.in_menu()`.
  - On OK, only when the box is enabled: checked and not in the menu calls `add_to_menu()`; unchecked and in the menu calls `remove_from_menu()`. OSError is swallowed (options.py:705-713).
- [ ] **"Automatically start downloading of URLs placed to clipboard":** `monitor_clipboard`, default True.
- [ ] **Group "Browser integration"**
  - Check box "Capture downloads from browsers (needs the LID extension)": `browser_capture`, default True.
  - Note: "The extension takes over downloads of the file types on the File types tab, and adds “Download with LID”, “Download all links with LID” and a YouTube button."
  - Button **"Set up browsers..."** opens BrowserIntegrationDialog (§2).
  - Status pill: "✓ N browser(s) connected" (ok, plural-aware), counted from `browser.is_installed` over `BROWSERS`, or "Not set up yet" (muted).
  - The pill is computed once when the dialog opens. It does not refresh after the setup dialog closes.

**Tab 2: File types** (header "Downloaded file types", icon filetypes) (options.py:353-368)
- [ ] **Label** "Automatically start downloading the following file types:", then a TagInput (placeholder "Add a file type…", extension normaliser) and its meta row. Setting `auto_capture_types` (a space-separated string).
  - Default: "3GP 7Z AAC ACE AIF APK ARJ ASF AVI BIN BZ2 EXE GZ GZIP IMG ISO LZH M4A M4V MKV MOV MP3 MP4 MPA MPE MPEG MPG MSI MSU OGG OGV PDF PLJ PPS PPT QT R0* R1* RA RAR RM RMVB SEA SIT SITX TAR TIF TIFF WAV WMA WMV Z ZIP". Entries can be wildcards such as `R0*`.
  - The normaliser lower-cases what the TagInput shows.
- [ ] **Button "Default"** (right-aligned, tooltip "Back to the default file types") resets the list to `DEFAULT_FILE_TYPES`.
- [ ] **Label** "Don't start downloading automatically from the following sites:", then a TagInput (placeholder "Add a site, e.g. *.example.com", site normaliser). Setting `no_auto_sites`, default "".

**Tab 3: Save to** (header "Categories, file types, folders", icon saveto) (options.py:370-457)
- [ ] **Group "Save to..."**
  - "Category" combo listing the categories with icons, plus "New" and "Edit..." buttons.
  - Label `Automatically put in “{name}” the following file types:`, then a TagInput (placeholder "Add a type…").
  - Label `Default download directory for “{name}”`, then a monospaced edit and "Browse".
  - **Switching category saves the edits of the previous one first** (`_store_category`): extensions, and the folder only if it is not empty (options.py:425-440).
  - "New" opens CategoryDialog(None). The category is added only if the name is unique, and it is then selected.
  - "Edit..." opens CategoryDialog(current) and replaces the current category with the result.
- [ ] **Default categories** (settings.py:20-26): General, plus:
  - Compressed: zip rar r0* r1* arj gz sit sitx sea ace bz2 7z
  - Documents: doc pdf ppt pps docx pptx
  - Music: mp3 wav wma mpa ram ra aac aif m4a tsa
  - Programs: exe msi
  - Video: avi mpg mpe mpeg asf wmv mov qt rm mp4 flv m4v webm ogv ogg mkv ts tsv
- [ ] **Group "Temporary directory"**
  - A monospaced edit and "Browse". Setting `temp_dir`, default `~/.cache/lider/parts`. An empty value keeps the old one.
  - Faint hint: "Parts are stored here while downloading. The finished file moves to its category folder."
- [ ] **On OK:** `settings.categories` is replaced with the edited copy.

**Tab 4: Downloads** (header "Default download settings", icon downloads) (options.py:459-516)
- [ ] **"Show "Download progress" dialog":** `show_progress_dialog`, default True.
- [ ] **"Show start download dialog":** `show_start_dialog`, default True.
  - **Indented under it:** "Do not start downloading, only add files to the queue": `only_add_to_queue`, default False. It is **enabled only while "Show start download dialog" is checked** (options.py:514-515).
- [ ] **"Show download complete dialog":** `show_complete_dialog`, default True.
- [ ] **"If a duplicate download link is added"** combo, Setting `duplicate_action`, default `ask`:
  - "Show the dialog and ask me" (`ask`)
  - "Add the duplicate with a numbered file name" (`number`)
  - "Add the duplicate and overwrite the existing file" (`overwrite`)
  - "If existing file is complete, show download complete dialog; otherwise resume it" (`complete`)
- [ ] **"User-Agent for manual downloads":** a monospaced edit, Setting `user_agent`, default "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36". An empty value keeps the old one.
- [ ] **Sub-header "Checksum verification"** (icon checksum) and a separator.
  - "Verify the checksum when a download finishes (when one was entered)": `verify_checksum`, default True.
    - Faint note under it: "Reads the whole file once, after its parts are merged. Large files take a few seconds; the list shows “Verifying”."
  - "Fill in the checksum from the clipboard when it holds a hash": `checksum_from_clipboard`, default False. It drives the File Info pre-fill (§1.4).
  - **"If the checksum doesn't match"** combo, Setting `checksum_mismatch`, default `ask`:
    - "Ask me — show the mismatch dialog" (`ask`)
    - "Keep the file, mark it “Checksum mismatch”" (`keep`)
    - "Delete the file and download it again" (`redownload`)

**Tab 5: Connection** (header "Connections and Limits", icon connection) (options.py:518-590)
- [ ] **Group "Max. connections number"**
  - "Default max. conn. number" combo of 1, 2, 4, 8, 16, 24, 32. Setting `max_connections`, default 8.
  - "Exceptions" tree: columns "SERVER" (220 px) and "MAX. CONNECTIONS", single selection, monospaced, sorted by pattern. Setting `server_limits` (a dict of pattern to n, default empty).
  - Side buttons:
    - "New" opens ServerLimitDialog and adds the entry if the pattern is not empty.
    - "Delete" removes the selected entry.
    - "Edit" opens ServerLimitDialog prefilled. It removes the old key, then adds the new one if the pattern is not empty, so renaming an entry works.
- [ ] **"Reconnection attempts for each connection":** a spin box, 0–100. Setting `retries`, default 10.

**Tab 6: Proxy / Socks** (page header "Proxy", icon proxy) (options.py:592-606)
- [ ] **Check box "Use proxy":** checked when `settings.proxy` is not empty.
- [ ] **"Proxy address":** a monospaced edit, prefilled with the proxy or `http://127.0.0.1:8080`. It is enabled only while "Use proxy" is checked.
- [ ] **Faint hint:** "Format: http://[user:password@]host:port — used for HTTP and HTTPS."
- [ ] **On OK:** `proxy` is the trimmed address when the box is checked, else "". The default is "". There is no SOCKS-specific field despite the tab title.

**Tab 7: Appearance** (header "Appearance", icon general) (options.py:608-698)
- [ ] **Caption "Theme"** with one card per key in `theme.THEMES`: ember, ember-dark, dark.
  - Each card has a `_ThemePreview`, a 78 px mini drawing of the window in that theme's colours (options.py:774-811).
  - Each card has a radio button with the theme title. Clicking anywhere on the card selects it, and the selected card gets the `selected` style.
  - Setting `theme`, default "ember".
- [ ] **"Follow the system light / dark setting":** `follow_system`, default True.
  - On the same row: "When dark, use", a combo of `theme.DARK_THEMES` titles. Setting `dark_theme`, default "ember-dark". It is enabled only while "Follow the system" is checked.
- [ ] **Segmented controls** (`_Segmented`, exclusive QToolButtons) (options.py:747-771)
  - **"List density":** "Comfortable" (`comfortable`) or "Compact" (`compact`). Setting `density`, default comfortable.
  - **"Toolbar":** "Icons and labels" (True) or "Icons only" (False). Setting `toolbar_labels`, default True.
  - **"Menu bar":** "Native" (`native`), "In window" (`custom`) or "Both" (`both`). Setting `menubar`, default native.
    - Hint: "Native = desktop global menu" when a global menu is available, else "No global menu here: Native shows it in the window".
- [ ] **"List font":** a read-only field showing "System default · N pt" or "Family · N pt".
  - "Change..." opens the font dialog.
  - "Reset" (ghost) clears it back to the default.
  - Setting `list_font`, a `QFont.toString()` string; default "".
- [ ] **Check-box grid of view settings**, 2 columns:
  - "Show Categories panel" (`show_categories`, default True)
  - "Show details pane (F9)" (`show_details`, default True)
  - "Show status bar" (`show_statusbar`, default True)
  - "Show icon in system tray" (`tray_icon`, default True)
  - "Close button hides to tray" (`close_to_tray`, default True)
  - "Start minimized to tray" (`start_minimized`, default False)
  - There is no dependency between these boxes in the dialog.

**OK / apply** (options.py:701-744)
- [ ] It saves the current category edits first, then writes every key listed above.
- [ ] Empty `temp_dir` and `user_agent` values keep their previous values.
- [ ] The theme is written only if a theme card is checked.
- [ ] The caller applies and saves the settings (mw:1157).

---

## 9. progress.py

### 9.1 SegmentBar: the per-connection bar (progress.py:42-74)
- [ ] **Shape:** a rounded track 12 px high, corner radius 5, in the `track` colour.
- [ ] **Each segment** is filled from `start` to `pos`, clamped to `end`. The fill is `ok` (green) once the segment is done (`end >= 0 and pos >= end`), else `ac` (accent). The minimum width is 1 px.
- [ ] **Separators:** a 2 px gap in the background colour (`win` here) is drawn before every segment except the first.
- [ ] **Unknown size:** nothing is drawn when `size_total <= 0`.

### 9.2 ProgressDialog (progress.py:85-333)
- [ ] **Window**
  - Deleted on close. Has a minimize button. Minimum width 600.
  - **The title is "NN% filename" and updates live.**
  - Refreshed every **500 ms** (progress.py:229-232).
  - The dialog closes itself when the download disappears.
- [ ] **Tab strip** with 3 tabs: "Download status", "Speed Limiter", "Options on completion".

**Tab "Download status"**
- [ ] **URL label:** monospaced, selectable. For YouTube it is followed by "   (1080p MP4)", i.e. the quality and the upper-case extension.
- [ ] **Grid rows:**
  - **Status** pill. The first match wins:
    1. "Verifying NN%" (accent)
    2. the task's live `status_text` (accent)
    3. "Error: …" (bad)
    4. "Paused" (muted)
    5. "⤓ Receiving data…" for DOWNLOADING, "Assembling…" for ASSEMBLING (accent)
    6. "✓ Complete" (ok)
    7. otherwise "Connecting…" (accent)
  - **File size:** the size or "(unknown)".
  - **Downloaded:** the size, plus " (NN.N %)" when the total is known.
  - **Transfer rate:** shown only while running.
  - **Time left:** a duration while running and known, else "(unknown)".
  - **Resume capability** pill: "(unknown)" (muted), "✓ Yes" (ok) or "No" (muted).

**Tab "Speed Limiter"**
- [ ] **Transfer rate:** a live copy of the rate.
- [ ] **"Use Speed Limiter":** checked when `d.speed_limit_kbps > 0`.
- [ ] **"Maximum speed":** a spin box, 1–10,000,000, unit "KBytes/sec". Initial value `d.speed_limit_kbps`, else the global `settings.speed_limit_kbps`.
- [ ] **"Remember Speed Limiter settings for this file on download start":** default **on**.
- [ ] **Behaviour** (progress.py:251-257)
  - Any change to the check box or the spin box applies immediately: `task.set_speed_limit(kbps or 0)`.
  - When "Remember" is on, the value is also written to the Download field `speed_limit_kbps`.

**Tab "Options on completion"**
- [ ] **"Show download complete dialog":** initial value from `settings.show_complete_dialog`. It applies to this download only.
- [ ] **"Save To":** a read-only monospaced field showing `d.path`.
- [ ] **Caption "Power options":**
  - "Exit LID when done"
  - "Turn off computer when done"
- [ ] **These are returned by `completion_options()`** as `{show_complete, exit, shutdown}` (progress.py:238-243).
- [ ] **What the caller does with them** (mw:1383-1401)
  - The dialog's values override the global setting.
  - The complete dialog is suppressed while the download's queue is running.
  - When it is suppressed, a tray message "Download complete" appears instead.
  - Shutdown runs `systemctl poweroff` after `manager.shutdown()`. Exit quits LID.

**Below the tabs**
- [ ] **Big percent label:** monospaced, bold, 20/12 scale. It shows "NN.N%", or the downloaded size when the total is unknown. Next to it, a progress bar (0–1000, 10 px).
- [ ] **Buttons**
  - **"<< Hide details" / "Show details >>"** (ghost) toggles the details area and resizes the dialog.
  - **Pause / Resume / Skip check**, minimum width 96:
    - The label is "Skip check" while verifying, "Pause" (stop icon) while running, "Resume" (resume icon) otherwise.
    - The action is `skip_check`, `stop` or `start` respectively.
    - It is disabled when the download is COMPLETE.
  - **"Cancel"** stops the download and closes the dialog.
- [ ] **Details area**, shown by default
  - Title "Start positions and download progress by connections".
  - A SegmentBar.
  - A connection table with columns "N.", "DOWNLOADED", "SPEED", "INFO" (widths 50, 110 and 100, minimum height 136). INFO is drawn by `InfoPillDelegate`.
  - Rows are reused, and extra rows are removed when the connection count drops.
  - SPEED shows only when the connection is alive and the download is running. INFO shows when alive or running (progress.py:320-333).

### 9.3 CompleteDialog: "Download complete" (progress.py:336-429)
- [ ] **Frame:** deleted on close. Minimum width 640. No shell header; it builds its own.
- [ ] **Header:**
  - A 42 px ok badge. It shows the "shield-check" icon when `d.checksum_result == "verified"`, else a painted check mark.
  - Title "Download complete", 1.25× and bold.
  - Subtitle "Downloaded 12.3 MB (12,345,678 Bytes)".
  - A pill "SHA-256 verified" when verified.
- [ ] **"Address":** a read-only monospaced field (click-focus) with the URL.
- [ ] **"The file saved as":** a read-only field with a leading category icon.
- [ ] **When verified:** caption "Checksum (SHA-256) · matches", then a read-only hash field with a trailing check icon.
- [ ] **Footer**
  - Check box "Don't show this dialog again". On close, if it is checked, `show_complete_dialog` is set to False and saved (mw:1441-1444).
  - **"Open"** (primary) runs `open_path` and closes.
  - **"Open with..."** asks "Command (the file path is appended):" in an input dialog titled "Open with". It runs `shlex.split(cmd) + [path]` in a new session, then closes.
  - **"Open folder"** runs `open_folder` and closes.
  - **"Close"** is the default button.

### 9.4 MismatchDialog: "Checksum mismatch" (progress.py:432-496)
- [ ] **Frame:** minimum width 640. Header icon "shield-alert" with a bad badge. Title "The file doesn't match its checksum". Subtitle "{filename} · {size}. It may be corrupt or changed on the server."
- [ ] **Mismatch box**
  - "Expected": the stored hex.
  - "Actual": `d.checksum_actual`, in red. Both values are selectable and monospaced.
  - Note: "{SHA-256}. Expected is the value you entered; Actual is computed from the downloaded file after its parts were merged."
- [ ] **"The file saved as":** a read-only field with the category icon.
- [ ] **Buttons**
  - "Delete file": danger style with the delete icon, placed on the left. Choice `delete`.
  - "Download again": primary and default. Choice `again`.
  - "Keep file": choice `keep`. Closing the dialog also keeps the file.
- [ ] **Handling** is the same as Check hash (§3). The dialog is non-modal and deleted on close.
- [ ] **It is shown only when** `checksum_mismatch == "ask"`. With `keep`, a tray message "Checksum mismatch — The file was kept." appears instead (mw:1411-1425). `redownload` is handled in the manager.

### 9.5 `choose_file`
- [ ] An Open dialog titled "Choose a file", starting at `start` or `~` (progress.py:499-501).

---

## 10. properties.py: PropertiesDialog, "File Properties" (properties.py:38-239)
- [ ] **Frame:** minimum width 660, no shell header.
- [ ] **Head**
  - A 52 px tile with the category icon.
  - An **editable file name** QLineEdit.
  - A status pill:
    - "! Error: msg" or "! Error" (bad);
    - "✓ Complete" (ok);
    - otherwise `fmt.status(d)` or "Queued", accent when running, else muted.
  - A muted category pill.
- [ ] **File panel**
  - **"Type":** "`{mime} file ({category})`". The mime comes from `mimetypes`, else the upper-case extension, else "Unknown".
  - **"Size":** "12.3 MB (12,345,678 Bytes)", or "(unknown)" when the size is negative.
  - **"YouTube":** the variant label, only for media downloads.
  - **"Save To":** a monospaced edit plus "&Browse..." (a folder dialog titled "Save To").
  - **"Address":** monospaced, **read-only for YouTube**.
  - **"Description"**.
- [ ] **Checksum panel**
  - Caption "Checksum" and a status pill. A 300 ms timer refreshes both.
  - "Expected": a ChecksumEdit with no hint text, and its message line.
  - "Result": a text label and a "Verify now" button.
  - **Result text, pill and colour** (properties.py:168-194):

    | Condition | Pill | Result text | Colour |
    |---|---|---|---|
    | verifying | "Verifying" (accent) | "Checking… NN%" | muted |
    | verified | "✓ Verified" (ok) | "Matches · checked Oct 24, 10:13 in 1.2 s" | okText |
    | mismatch | "Checksum mismatch" (bad) | "Doesn't match · checked … in … s" | badText |
    | skipped | "Not checked" (muted) | "Skipped — the file was not checked" | muted |
    | a checksum is set | "Not checked" (muted) | "Not checked yet" | muted |
    | no checksum | "Not set" (muted) | "No checksum entered" | muted |

  - "Verify now" is enabled only when a valid, non-empty value is entered, no verification is running, and the status is COMPLETE.
  - Verify now: if the value changed, it writes `d.checksum` and resets `checksum_result` and `checksum_actual`. It then calls `manager.verify(id, manual=True)` (properties.py:196-203).
- [ ] **Web page and authorization panel**
  - "Parent page": a selectable label with `d.parent_page`, or "Unknown" (muted).
  - "Referer": an editable monospaced field.
  - "Login" and "Password" (password echo) on one row.
- [ ] **Footer info line:** "Last try: {date or 'never'} · Result: {Error: … | status}", followed by " · SHA-256 verified/mismatch" when there is a result.
- [ ] **Buttons:** "Open" (enabled only when COMPLETE) runs `open_path`. "OK" is primary and default, with the initial focus.
- [ ] **OK / apply** (properties.py:210-239)
  - A trimmed name or folder that is empty keeps the old value.
  - **Rename or move:**
    - Blocked while running: warning "Stop the download before moving or renaming it."
    - When COMPLETE and the file exists, the folder is created with `makedirs` and the file moved with `shutil.move`. On error: "Cannot move the file:\n{e}", and the dialog stays open.
    - Then `filename` and `save_dir` are updated.
  - Writes `url` (an empty value keeps the old one), `description`, `referer`, `username` and `password`.
  - An invalid checksum shows a warning with the parse error and the dialog stays open.
  - A changed checksum resets `checksum_result`, `checksum_actual` and `checksum_retried = False`.
  - Finally calls `manager.update(d)`.

---

## 11. scheduler.py: SchedulerDialog, "Scheduler" (scheduler.py:320-499)
- [ ] **Window:** title "Scheduler", size 820×720. Constants: `DAYS` = Monday … Sunday, index 0 = Monday.
- [ ] **Left column (210 px)**
  - Caption "Queues", then a tree with no header. Each item has an icon ("queue-sync" for the sync queue, "queue" otherwise) and a count badge with the number of items (`COUNT_ROLE`).
  - **Order:** MAIN_QUEUE, then SYNC_QUEUE, then the other queues sorted by name (scheduler.py:439-441).
  - **"&New queue"** asks "Enter a name for the new queue that will be displayed in the list of queues" in an input dialog titled "Enter queue name". The trimmed name must be non-empty and unique. It applies the current queue's edits, adds the queue and selects it.
  - **"&Delete"**
    - The default queues are protected: the info message is "The default queues cannot be deleted."
    - Otherwise it asks `Delete queue "{name}"?`. On Yes it deletes the queue and selects the main queue.
- [ ] **Right column**
  - A bold title (14/12 scale) with the queue name.
  - A state pill: "Running" (accent) or "Stopped" (muted).
  - A card with the tab strip "Schedule" | "Files in the queue".
- [ ] **Selecting another queue auto-applies the previous queue's edits** (scheduler.py:443-454).
- [ ] **Footer**
  - **"&Start now"** (primary, play icon) applies the edits, calls `manager.start_queue`, passes the events to the main window, and sets the pill to Running.
  - **"S&top"** calls `stop_queue` and sets the pill to Stopped.
  - **"&Apply"** stores the schedule and `max_parallel`, then calls `manager.save_queue`.
  - **"&Close"** applies and accepts. There is no Cancel; edits are always saved.

### 11.1 Tab "Schedule" (ScheduleTab, scheduler.py:58-237). Model `QueueSchedule` in `core/manager.py:27-61`
- [ ] **Mode radio group:**
  - "One-time downloading" (`periodic=False`, the default)
  - "Periodic synchronization" (`periodic=True`)
- [ ] **"Start download on LID startup":** `start_on_startup`, default False.
- [ ] **"Start download at"**, then an `HH:mm` time edit on the right. `start_enabled`, default False; `start_time`, default "23:00".
  - Indented radio **"Once at"**, then a date edit (calendar popup, format `yyyy-MM-dd`). `once_date`; when it is empty, today's date is shown.
  - Indented radio **"Daily"** (`daily`, default False, so "Once at" is the default), then a 3-row grid of the 7 weekday check boxes, Monday…Sunday. `days`, default all 7.
- [ ] **"Start again every"**, then a hours spin box (0–999, default 2) and a minutes spin box (0–59, default 0). `every_hours`, `every_minutes`. **Visible only in Periodic mode.**
- [ ] **"Stop download at"**, then an `HH:mm` time edit. `stop_enabled`, default False; `stop_time`, default "07:30".
- [ ] **"Number of retries for each file if downloading failed"**, then a spin box (0–999, no arrows). `retries_enabled`, default True; `retries`, default 10.
- [ ] **"Open the following file when done:"**
  - Below it, an indented row with a monospaced path edit and a "..." button. The row is **visible only while the box is checked**.
  - "..." opens a file dialog titled "Open the following file when done". Choosing a file also checks the box.
  - `open_when_done` is the path, or "" when unchecked.
- [ ] **"Exit LID when done":** `exit_when_done`, default False.
- [ ] **"Turn off computer when done":** `shutdown_when_done`, default False.
- [ ] **Enable rules** (scheduler.py:190-199)
  - The start time, "Once at" and "Daily" are enabled only when "Start download at" is checked.
  - The date edit is enabled only when that box is checked and "Once at" is selected.
  - The weekday boxes are enabled only when that box is checked and "Daily" is selected.
  - The stop time and retries spin box have **no** enable dependency on their check boxes.
- [ ] **Store rules:** `once_date` is "" when Daily is selected, else the chosen date (scheduler.py:228).
- [ ] **"Next run" box** (accent, clock icon) at the bottom (scheduler.py:156-166, 407-424)
  - It recomputes on changes to start-enabled, once, daily, any weekday, the start time and the date.
  - It is hidden when `next_start()` returns None.
  - The text is "Next run {day} at HH:MM · in {H h }{M} m". Minutes are rounded up.
  - `day` is "tonight" (today at or after 18:00), "today", "tomorrow", or "Fri 07 Nov" (`%a %d %b`).
- [ ] **`next_start` semantics** (`core/manager.py:48-61`)
  - None unless start is enabled.
  - Daily: the first time ≥ now within 8 days whose weekday is in `days`.
  - Once with a date: that date and time, if it is not in the past; otherwise None.
  - Once without a date: today at that time, or tomorrow if it has passed.
- [ ] **After a queue finishes** (mw:1446+): a tray message `"{name}" processing stopped/finished`, then the open-when-done file, exit or shutdown.

### 11.2 Tab "Files in the queue" (FilesTab, scheduler.py:240-317)
- [ ] **"Download [N] files at the same time":** a spin box, 1–64. Stored as `schedule.max_parallel`, default 1.
- [ ] **List**
  - Columns "FILE NAME" (stretches, with a category icon), "SIZE" (90 px, monospaced), "STATUS" (110 px; a pill delegate maps "Queued" to muted).
  - Single selection. The rows are `manager.queue_items(queue)` in queue order.
  - The status is `fmt.status(d)`, or "Queued".
- [ ] **Round icon buttons** (30 px)
  - "Move up" (up icon) calls `manager.move_in_queue(id, -1)` and keeps the selection.
  - "Move down" (down-arrow icon) calls `move_in_queue(id, +1)`.
  - "Delete from queue" (remove icon) calls `manager.set_queue([id], "")`. This removes the download from the queue, not from the list.
