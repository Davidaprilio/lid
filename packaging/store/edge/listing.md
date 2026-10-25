# Microsoft Edge Add-ons — submission sheet

Everything Partner Center asks for, ready to paste. The images are in [`assets/`](assets).
Policies: <https://learn.microsoft.com/legal/microsoft-edge/extensions/developer-policies>.

## 0. Order of work (the extension id)

The store gives the extension its own id. The LID helper only answers the ids listed in
`CHROMIUM_EXTENSION_IDS` (`lider/browser.py`), so a store copy cannot reach a LID release that
does not list that id. Reviewers would see "LID helper not found".

1. **Create the extension and upload the package**, but do not submit yet. Use
   `lid-extension-chromium-store.zip` from `./packaging/build.sh extension`. That build has no
   `key` and no YouTube button.
2. Copy the id from **Extension overview** (the 32-letter "CRX ID"). Done: the id is
   `ophlhhmpoaonhglimkemkgjifigpghej` (Store ID `0RDCKF25DZGT`), now `EDGE_ADDONS_EXTENSION_ID`.
3. Add it to `CHROMIUM_EXTENSION_IDS`, release LID with it, and upload that release's
   `lid-extension-chromium-store.zip` again (same version as the release).
4. Fill in the rest below and submit. Once the listing is public, set its address in `lider/browser.py`
   (the "Get the extension" buttons) and in the README.

## 1. Availability

- **Visibility:** Public
- **Markets:** all markets

## 2. Properties

- **Category:** Productivity
- **Website:** https://github.com/Davidaprilio/lid
- **Support contact detail:** https://github.com/Davidaprilio/lid/issues
- **Mature content:** no

## 3. Privacy

### Single purpose description

> Hands downloads and links from Microsoft Edge to LID, a download manager for Linux that runs on
> the same computer, so LID downloads them instead of the browser.

### Permission justification

| Permission | Justification |
|---|---|
| `downloads` | Sees a download as it starts so that, when the user turned capture on and LID takes that file type, the download is handed to LID and the browser's copy is cancelled. |
| `nativeMessaging` | The only way the extension talks to the LID desktop app on the same computer (download requests, the connection status in the popup, "Open LID"). |
| `contextMenus` | Adds "Download with LID", "Download selected links with LID" and "Download all links with LID" to the right-click menu. |
| `cookies` | Reads the cookies of the site a download comes from, only for downloads the user hands to LID, so files that need a sign-in download in LID as they would in the browser. |
| `scripting` | When the user chooses "Download all links / selected links with LID", runs a small script in that tab to list the page's links. Nothing runs on pages otherwise. |
| `storage` | Remembers the user's "Take over browser downloads" on/off setting. |
| Host permission `<all_urls>` | Downloads and links can come from any site: the cookies of the download's site and the link list of the page the user right-clicks are needed wherever that happens. |

### Are you using remote code?

**No, I am not using remote code.** (Manifest V3, every script is in the package.)

### Data usage

The extension passes download information to a local app on the same computer. It never sends
anything to the developer or to third parties, has no analytics and no ads.

- **What user data do you plan to collect:** leave every box unchecked. The data never leaves the
  computer and the developer receives none of it. If a reviewer counts data handled locally as
  collected, tick **Authentication information** (cookies) and **Web history** (download and page
  addresses), and keep the explanation above.
- **Certifications:** tick all three. The developer does not sell or transfer data, does not use
  it for unrelated purposes, and does not use it for creditworthiness or lending.

### Privacy policy URL

https://github.com/Davidaprilio/lid/blob/redesign/packaging/store/edge/privacy.md

(Entered with the `redesign` branch link; switch it to `main` once the branch is merged.)

## 4. Store listing — English

- **Extension name** (from the manifest): LID Integration
- **Short description** (from the manifest): Send downloads and links from the browser to LID
  (Linux Internet Downloader).

### Description

```text
LID Integration connects Microsoft Edge to LID (Linux Internet Downloader), a free, open-source download manager for Linux. It needs the LID desktop app installed on the same computer. On other systems the extension only explains that LID is required, and downloads stay in Microsoft Edge.

What it does
• Takes over downloads: when a download starts and its file type is one you chose in LID, the file goes to LID instead of the browser's download list. LID downloads it over several connections at once, can pause and resume it, and can put it in a queue or a schedule.
• Right-click a link, image, video or audio and choose "Download with LID".
• Right-click a page and choose "Download all links with LID", or select part of a page and choose "Download selected links with LID", then pick the files you want in LID.
• Downloads that need you to be signed in keep working: for each download you hand over, the extension passes that site's cookies and the page address to LID.
• The toolbar button shows whether LID is connected and turns capture on or off.

Setup
1. Install LID from https://github.com/Davidaprilio/lid/releases (AppImage, .deb or Arch package).
2. In LID, open Options → General → Set up browsers…, check Microsoft Edge and click "Install for checked browsers".
3. Click the LID button in the toolbar: it shows "Connected to LID".

Privacy
The extension talks only to the LID app on your computer, through native messaging. It sends nothing to the developer or anyone else, and has no analytics, no tracking and no ads. Details: https://github.com/Davidaprilio/lid/blob/redesign/packaging/store/edge/privacy.md
```

### Images

| Field | File |
|---|---|
| Extension logo (required, 300 × 300) | `assets/logo-300x300.png` |
| Small promotional tile (440 × 280) | `assets/tile-small-440x280.png` |
| Large promotional tile (1400 × 560) | `assets/tile-large-1400x560.png` |
| Screenshots (1280 × 800) | `assets/screenshot-1.png`, `assets/screenshot-2.png`, `assets/screenshot-3.png` |

Screenshot 3 shows the popup connected to "LID 1.2.0"; it is an illustration and stays as it is.

### Search terms

Seven terms, 30 characters each at most:

```text
download manager
Linux
download accelerator
download all links
resume downloads
LID
batch download
```

## 5. Notes for certification

```text
LID Integration hands browser downloads to LID, a desktop download manager for Linux, through native messaging. It needs Microsoft Edge on Linux with the LID app installed. On Windows or macOS the popup only explains that LID is required (see screenshot 3) and downloads stay in Edge. No account or sign-in is needed.

Test on any x86_64 Linux (for example Ubuntu 22.04 or later):
1. Download LID <VERSION> from https://github.com/Davidaprilio/lid/releases/tag/v<VERSION> :
   - Lider-<VERSION>-x86_64.AppImage: make it executable (chmod +x) and run it, or
   - lider_<VERSION>-1_amd64.deb: sudo apt install ./lider_<VERSION>-1_amd64.deb, then run "lider".
2. In LID: Options → General → "Set up browsers…", check "Microsoft Edge", click "Install for checked browsers", close the dialog. This writes the native messaging host manifest to ~/.config/microsoft-edge/NativeMessagingHosts/io.github.davidaprilio.lid.json. The .deb also installs it system-wide.
3. In Edge, click the LID toolbar button: it shows "Connected to LID <VERSION>".
4. Download a .zip file, for example https://github.com/Davidaprilio/lid/archive/refs/tags/v<VERSION>.zip. LID opens its "Download File Info" window, and the copy in Edge's download list is cancelled.
5. Right-click a link and choose "Download with LID". Right-click a page and choose "Download all links with LID": LID lists the page's links.
6. In the popup, turn "Take over browser downloads" off: downloads stay in Edge again.

The extension sends no data off the computer. Source code: https://github.com/Davidaprilio/lid (folder lider/browser_extension).
```

Replace `<VERSION>` with the LID release that lists the Edge id (step 0): **1.1.2**. The extension
in that release is version **1.0.0** (`EXTENSION_VERSION`); the LID app is 1.1.2.
