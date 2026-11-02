# givmedroppedboxes

Browse a public Dropbox share link in your terminal and download only the
files and folders you want. No Dropbox account, no API token — just paste
the link.

Built with [ratatui](https://ratatui.rs).

```
┌ givmedroppedboxes ──────────────────────────────────────────────┐
│ Share:  holiday-photos                                          │
│ Output: ./holiday-photos                                        │
│ Marked: 12 files (1.2 GiB+)                                     │
├─────────────────────────────────────────────────────────────────┤
│ ▼ [ ] holiday-photos                                            │
│     ▼ [x] day-1                                                 │
│         [x] beach.jpg   · 3.4 MiB                               │
│         [ ] raw.dng     · 24.1 MiB                              │
│     ▶ [-] day-2                                                 │
│     [ ] readme.txt      · 1.2 KiB                               │
├─────────────────────────────────────────────────────────────────┤
│ ↑↓/jk move · ←→/hl expand · Space select · a all · n none · …   │
└─────────────────────────────────────────────────────────────────┘
```

## Build

Requires a recent Rust toolchain.

```sh
cargo build --release
# binary: target/release/givmedroppedboxes
```

## Usage

```sh
# interactive: paste the link when prompted
givmedroppedboxes

# or pass it directly
givmedroppedboxes 'https://www.dropbox.com/scl/fo/<key>/<hash>?rlkey=<rlkey>'

# options
-o, --output <dir>   download destination (default: ./<share name>)
-j, --jobs <n>       parallel downloads (default: 3)
```

### Keys

| Screen    | Key                        | Action                              |
| --------- | -------------------------- | ----------------------------------- |
| link      | Enter / Esc                | open link / quit                    |
| browser   | `↑↓` / `j k`               | move cursor                         |
|           | `←→` / `h l` / Enter       | collapse / expand folder            |
|           | Space                      | select/deselect file or whole folder |
|           | `a` / `n`                  | select everything / nothing         |
|           | `r`                        | re-list the selected folder         |
|           | `d`                        | start downloading the selection     |
|           | `q`                        | quit                                |
| progress  | `c`                        | cancel (finished files are kept)    |
| summary   | `r`                        | retry failed downloads              |
|           | `b`                        | back to the browser                 |

Selecting a folder selects everything inside it — including content that
hasn't been listed yet; the file list is resolved when the download starts.
A yellow `[-]` marks folders with only some content selected.

### Already-downloaded markers

Every download is recorded in `.givmedroppedboxes.json` inside the output
directory. On the next run against the same link, downloaded files get a
green `✓`, folders show how many of their files are already on disk
(`· 12 downloaded` — even for folders that were never expanded), and the
header shows the total. The markers also check the disk, so manually
deleted files stop counting as downloaded. Delete the state file to reset
the history.

## How it works

Dropbox share links can be browsed anonymously — the website itself does it.
This tool replicates that: it opens the share page once to get the session
cookies, then talks to the same undocumented endpoint the web UI uses
(`list_shared_link_folder_entries`) and downloads files via their per-item
direct links.

That means **no login and no API token**, but also: this is not an official
API. It works today and is exercised by several maintained open-source
projects, but Dropbox can change it without notice. Password-protected
links and links that require a Dropbox login are not supported.

## Limitations

- Public share links only (no passwords, no team-only links).
- The `rlkey` query parameter must be present in the link (it is on every
  link you copy from the Dropbox web UI).
- Heavy use may hit Dropbox rate limits (the tool backs off and retries).
- Already-downloaded files are re-downloaded (overwritten) if you select
  them again; the markers are informational only.

## Testing

```sh
cargo test
```
