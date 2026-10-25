# LID Integration for Microsoft Edge — Privacy Policy

_Last updated: October 25, 2026_

LID Integration is a Microsoft Edge extension that hands downloads and links to **LID (Linux Internet
Downloader)**, a free, open-source download manager that runs on the same computer. This policy
explains what the extension reads, where that information goes, and what it does not do.

## In short

- The extension sends information **only to the LID app on your own computer**, through Microsoft
  Edge's native messaging. Nothing is sent to the developer or to any third party.
- There are no analytics, no tracking, no advertising and no remote code.
- The developer never receives, stores, sells or shares your data.

## What the extension reads, and why

| Information | When | Why |
|---|---|---|
| The download's address, file name, size and type, and the page it came from | When a download starts while "Take over browser downloads" is on and LID is set to take that file type, or when you choose **Download with LID** | So LID can download the file |
| The addresses and texts of links on a page | Only when you choose **Download all links with LID** or **Download selected links with LID** | So LID can list the links for you to pick from |
| Cookies for the download's site | Only for a download or page you hand to LID | So LID can download files that need you to be signed in, the same way Microsoft Edge would |
| Microsoft Edge's user-agent string | With each download handed to LID | So the server sees the same browser |
| Your "Take over browser downloads" setting | Stored in the extension's local storage | To remember whether capture is on |

The extension does not read the contents of pages, form fields, passwords, your browsing history or
any other data. It never reads cookies for sites you do not download from through LID.

## Where the information goes

Everything above goes directly from Microsoft Edge to the LID app on the same computer, through
native messaging. LID keeps its download list locally (in `~/.local/share/lider`) and connects only
to the servers the files are downloaded from. Neither the extension nor LID contains code that
sends data to the developer or to anyone else.

## Your choices

- Turn **Take over browser downloads** off in the extension's popup to keep every download in
  Microsoft Edge.
- Disable or remove the extension at any time from `edge://extensions`.
- Remove downloads from LID's list, or remove the helper in LID under Options → General → Set up
  browsers… → Remove.

## Children

The extension is not directed at children and does not knowingly handle information about them.

## Changes to this policy

If the extension's handling of information changes, this policy is updated before the new version
is published, and the date at the top changes.

## Contact

Questions or concerns: open an issue at <https://github.com/Davidaprilio/lid/issues>.
