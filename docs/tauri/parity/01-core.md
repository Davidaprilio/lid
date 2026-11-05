# LID core: behaviour checklist for the Rust + Tauri port

I read all of `lider/core/*.py` plus the listed tests and fixtures. I also checked a few spots in `lider/ui/` where core settings are used (power-off, open/exit, duplicates, cookies), and those are marked **(UI)**. References are `file:line`. Core files live in `lider/core/`, tests in `tests/`. `lider/core/__init__.py` is empty (0 lines).

---

## 1. Engine (`engine.py`)

### 1.1 Constants
- [ ] `CHUNK = 1 MiB`: read size when no limiter is active (`engine.py:30`).
- [ ] `LIMITED_CHUNK = 64 KiB`: read size when the per-file or global limiter has rate > 0 (`:31`, `:409-410`).
- [ ] `MIN_SPLIT = 256 KiB`: a segment is split only if `remaining >= 2*MIN_SPLIT` (512 KiB) (`:32`, `:359`).
- [ ] `FATAL_HTTP = {400,401,403,404,405,410,451}`: fails the connection immediately, no retry (`:33`, `:442-444`).
- [ ] `EngineOptions` defaults: `max_connections=8`, `retries=10`, `retry_delay=3.0`, `temp_dir="/tmp"` (`:187-192`). The manager overrides them from Settings.

### 1.2 Filename helpers
- [ ] `parse_content_disposition` (`:42-49`), case-insensitive:
  1. RFC 5987 `filename*` first. Regex `filename\*\s*=\s*([\w-]*)'[^']*'([^;]+)`. The value is percent-decoded with the given charset (default utf-8, errors replaced), after stripping whitespace and then `"`.
  2. Otherwise quoted `filename="..."`.
  3. Otherwise unquoted `filename=([^;]+)`, stripped.
  - The plain form is not percent-decoded and has no escape handling. `None` or empty input returns `None`.
- [ ] `filename_from_url`: basename of the percent-decoded URL path. `"index.html"` if empty, e.g. a trailing `/` (`:52-54`).
- [ ] `safe_filename` (`:57-60`):
  - Replace `/`, `\` and `\x00-\x1f` with `_`.
  - Strip whitespace, then strip leading and trailing `.`.
  - If empty, use `"download"`.
  - Truncate the stem to `240 - len(ext)` characters (code points, not bytes). Python `splitext` semantics: last suffix only, and a leading-dot name has no ext.
- [ ] `unique_path` (`:63-71`): returns the path unchanged if free. Otherwise `stem (n)ext` with n = 2, 3, … until free, using `splitext` on the full path (`a.tar.gz` becomes `a.tar (2).gz`). Test `test_unique_path`.

### 1.3 SpeedLimiter: token bucket, shared by everything under one limit (`:74-97`)
- [ ] `rate = max(0,int(kbps))*1024` bytes/s. 0 means unlimited, and `consume` returns immediately.
- [ ] `set_kbps` resets tokens to 0 and the clock to now. It is thread-safe.
- [ ] `consume(n, stop)`:
  1. Refill `tokens = min(rate*0.25, tokens + dt*rate)`, so the burst cap is 250 ms worth.
  2. Subtract `tokens -= n` (it may go negative).
  3. Set `delay = -tokens/rate` and sleep outside the lock using `stop.wait(delay)`, so a pause interrupts it.
- [ ] Global limiter: one instance in the manager (`manager.py:102`), shared by all tasks and their connections.
- [ ] Per-file limiter: one per task, built from `d.speed_limit_kbps` (`engine.py:223`). Each read is charged to both, global first, then file (`:423-425`).
- [ ] `DownloadTask.set_speed_limit(kbps)` updates `d.speed_limit_kbps` and the live limiter (`:254-256`).

### 1.4 SpeedMeter (`:100-121`)
- [ ] Sliding 4 s window of `(monotonic_time, bytes)` samples, pruned on `add` and on `rate`.
- [ ] `rate = sum(bytes) / max(1.0, now - oldest_sample_time)`. The divisor is at least 1 s and measured from the oldest sample, not the window length. Returns 0 when there are no samples.
- [ ] One meter per task and one per connection. Test `test_snapshot_reports_speed_per_connection`: the per-connection rates sum to roughly the task rate (±50%).

### 1.5 HttpClient (`:136-172`)
- [ ] Built per task from `(settings.user_agent, settings.proxy, settings.timeout)` (`manager.py:109-110`).
- [ ] Proxy: one string used for both `http` and `https` (`engine.py:141-142`).
  - **Quirk:** when `proxy == ""`, urllib's default ProxyHandler still applies, so `http_proxy`/`https_proxy`/`no_proxy` env vars are honoured.
  - FTP is never proxied by the explicit setting.
- [ ] Timeout: socket timeout per connect and per read, default 30 s.
- [ ] Redirects: followed automatically by urllib (301/302/303/307/308). Headers are copied to the redirect target, including Authorization, Cookie and Range.
  - `probe` reports the final URL (`resp.geturl()`), but `d.url` is never updated, so every connection follows the redirects again (`:165`, `:402`).
- [ ] FTP: for `ftp://` the URL is opened with no headers and no Range (`:146-147`). The response has no `.status`, so it is treated as 200.
- [ ] Headers on every HTTP(S) request (`:148-151`):
  - `User-Agent: <settings.user_agent>`
  - `Accept: */*`
  - `Accept-Encoding: identity` (never compressed)
  - `Range: bytes=<start>-` when ranged. Always open-ended, never `start-end`.
  - Then `headers` passed in are merged last and **can override** the defaults above.
- [ ] `request_headers(d)` (`:175-182`):
  1. Start from a copy of `d.headers` (extra headers such as `Cookie` and `User-Agent` from the browser extension, **(UI)** `main_window.py:989-993`).
  2. `Referer` = `d.referer` if set (overrides).
  3. `Authorization: Basic base64(user:pass)` if `d.username` is set; the password may be empty.
  - There is no cookie jar, no Digest auth and no other auth.
- [ ] `probe(url, headers, keep_open)` (`:154-172`):
  - Sends a **GET** (not HEAD) with `Range: bytes=0-`.
  - If status is 206 and `Content-Range` matches `bytes\s+\d+-\d+/(\d+)`: `size = total` and `resumable = True`.
  - Else if `Content-Length` is present: `size = Content-Length` and `resumable = False`.
  - Else `size = -1`.
  - `filename = safe_filename(CD filename or filename_from_url(final_url))`. `content_type` comes from `Content-Type`.
  - `keep_open=True` keeps the response, which becomes the first connection's stream with no second request.
  - Status ≥ 400 raises HTTPError, which the caller handles.
- [ ] `ProbeResult` fields: `url` (final), `size`, `resumable`, `filename`, `content_type`, `response` (`:126-133`).

### 1.6 Connection model (`:195-203`)
- [ ] Fields: `index`, `segment`, `status` (initially `"Connecting..."`), `received`, `alive`, `thread`, `meter`.
- [ ] `index = len(connections)+1`, 1-based. The list never shrinks; dead connections stay with `alive=False` (`:344`).
- [ ] Connection status strings shown in the UI:
  - `"Connecting..."`
  - `"Receiving data..."`
  - `"Reconnecting (n/retries)..."`
  - `"Finished"`
  - `"Error: <msg>"`
  - `"HTTP <code> <reason>"` (fatal)
  - `"Error: server ignored the Range header"`

### 1.7 Run flow (`DownloadTask._run`, `:273-324`)
1. [ ] Set `d.last_try = now`, `d.error = ""`, `status = CONNECTING`.
2. [ ] Probe with `request_headers(d)` and keep the response open.
3. [ ] Resume only when **all** of these hold: `d.segments` is non-empty, `d.resumable` is truthy, `probe.resumable` is true, `probe.size == d.size`, and the part file exists (`:281-287`). The probe response is then closed and connections reopen at their saved `pos`.
4. [ ] Otherwise start fresh:
   - Take `d.size`/`d.resumable` from the probe.
   - `segments = [Segment(0, size if size >= 0 else -1, 0)]`.
   - Reuse the probe response for connection #1.
   - **Delete the existing part file** (`:290-295`).
5. [ ] Recompute `d.downloaded = Σ(pos-start)` (`:296`).
6. [ ] `splittable = resumable and size > 0` (`:297`).
7. [ ] Open the part file:
   - `mkdir -p temp_dir`.
   - `open(part, O_RDWR|O_CREAT, 0644)`.
   - If `size >= 0`, `ftruncate(size)` pre-sizes it as a sparse file (`:299-302`).
8. [ ] If stopped during the probe, close the response and skip workers. Otherwise set `status = DOWNLOADING`, spawn workers, and join every connection thread, looping while any is still alive (`:304-315`).
9. [ ] Errors in `_run`:
   - `HTTPError` sets `"HTTP <code> <reason>"`.
   - `OSError`/`HTTPException`/`ValueError` set `str(e.reason or e)`, or the class name if empty (`:316-319`).
   - The fd is always closed. `_finish()` always runs.
- [ ] Part file location: `<temp_dir>/<part_name>.part`. `part_name` defaults to `str(d.id)`, and media streams pass `"<id>-media-<kind>"` (`:219`, `:236-237`).
  - The manager's default `temp_dir` is `~/.cache/lider/parts` (`settings.py:137`).
- [ ] Writes use `os.pwrite(fd, data, offset)` at the segment's `pos`, outside the lock (`:433`).

### 1.8 Segmentation (`:326-374`)
- [ ] `limit = max_connections` if splittable, else 1 (`:328`).
- [ ] Spawn:
  1. One connection per pending (not done) segment, up to `limit`. The first gets the probe response (`:335-336`).
  2. While splittable and fewer than `limit` connections exist, split the largest segment and add a connection for it. Stop when nothing can be split (`:337-341`).
- [ ] `_split_largest` (caller holds the lock):
  - Candidates are segments that are not done **and owned by an alive connection**. Pick the one with the largest `remaining`.
  - Abort if `remaining < 512 KiB`.
  - `mid = pos + remaining//2`. Add a new `Segment(mid, end, mid)` and set the old `end = mid`.
  - Append the new segment and **sort segments by start** (`:352-366`).
- [ ] `_next_work`: when a connection finishes a segment it first takes an **unowned, not-done** segment (abandoned by a failed connection, or left over on resume). Otherwise it splits the largest if splittable, else gets nothing (`:368-374`).
- [ ] Worker loop (`:376-391`):
  - `_fetch`. Any result other than `"done"`, or a stop, sets `segment = None` and ends the worker. A failed connection is not replaced; its segment is abandoned for others to pick up.
  - On `"done"`, take `_next_work()`.
  - On a normal end with status `Receiving…`/`Connecting…`, set status `"Finished"`.
  - `alive = False` in `finally`.
- [ ] The owner of a split segment notices `want = end - pos <= 0` on its next read, returns `"done"` and closes its open-ended response (`:411-414`).

### 1.9 `_fetch`: reading, retries, fallbacks (`:393-465`)
- [ ] `attempts` counts per `_fetch` call (per segment assignment) and is **not reset** after progress. Headers are computed once per call.
- [ ] (Re)connect:
  - Set status `"Connecting..."`.
  - `ranged = splittable or seg.pos > 0`.
  - If ranged and the response status is not 206, raise `ResumeNotSupported("server ignored the Range header")`. The connection fails immediately, the error is set and there is **no retry** (`:399-404`, `:437-440`).
- [ ] Reading:
  - Status `"Receiving data..."`. Use `read1` (whatever has arrived, up to the step size).
  - Each read is `min(step, want)` where `want = end - pos`, or `step` if `end < 0`.
- [ ] EOF (empty read):
  - Unknown size (`end < 0`): set `seg.end = pos` and `d.size = pos`, return done. This is the stream-to-end mode (test `test_unknown_size_streams_to_end`).
  - Known size: raise `ConnectionError("connection closed by the server")`, which is retried.
- [ ] Clamp: `allowed = min(len(data), end - pos)`, recomputed after the limiter sleep because a split may have shrunk `end`. Then advance `pos`, `d.downloaded` and `conn.received`, pwrite, and add to the meters (`:426-435`).
- [ ] Errors:
  - Fatal HTTP code: `conn.status = task.error = "HTTP <code> <reason>"`, return failed.
  - Other HTTP codes (5xx, 416, 429…): `err = "HTTP <code>"`, retry.
  - `OSError`/`HTTPException`, including socket timeout and URLError: `err = reason/str/classname`, retry (`:441-447`).
  - **Quirk:** `ValueError` is not caught inside `_fetch`, so the thread dies and `alive=False`.
- [ ] Retry:
  1. `attempts += 1`. If `attempts > retries`, set `conn.status = "Error: <err>"`, `task.error = err`, return failed. So retries=10 means up to 11 attempts.
  2. **Non-resumable server:** roll back `d.downloaded -= pos-start`, set `pos = start`, `received = 0` and start over.
  3. Status `"Reconnecting (n/retries)..."`, wait `retry_delay` (interruptible) (`:452-464`).
- [ ] Tests:
  - `test_dropped_connection_is_retried` (drop after 300 KB, the result is still byte-exact).
  - `test_server_without_ranges_uses_one_connection` (`resumable False`, 1 connection).
  - `test_http_404_is_an_error` (actually a closed port, ending in ERROR with a non-empty `error`).

### 1.10 Finish (`:467-491`)
- [ ] `complete = segments non-empty and all done`. Completeness is decided by the segments, not by `task.error`.
- [ ] If complete and not stopped:
  1. Status `ASSEMBLING`.
  2. `mkdir -p save_dir`.
  3. `target = unique_path(d.path)` for **name collisions at finish time**.
  4. `shutil.move(part, target)`, which also works across filesystems.
  5. `d.filename = basename(target)`, `d.size = d.downloaded`, `d.segments = []`, status `COMPLETE`.
  - On OSError: `d.error = "Cannot save the file: <e>"`, status `ERROR`.
- [ ] Stopped: `PAUSED`, even if all segments are done. Segments are kept; on the next start `can_resume` holds, there is no pending work, and it completes.
- [ ] Neither: `d.error = task.error or "Download failed"`, status `ERROR`.
- [ ] Then the `on_finish(task)` callback runs. The manager queues it for `poll`.

### 1.11 Public surface / snapshot (`:234-267`)
- [ ] `start()` (daemon thread `dl-<id>`), `stop()` (sets the event), `join(timeout)`, `running`, `set_speed_limit`, `error`, `connections`, `meter`, `d`, `part_path`.
- [ ] `snapshot()` returns:
  - `segments`: a copy of `[Segment]`.
  - `connections`: `[(index, received, status, alive, rate if alive else 0.0)]`.
  - `rate`: task meter rate.
  - `time_left`: `int((size-downloaded)/rate)`, or -1 if rate is 0 or size ≤ 0.
- [ ] Resume persistence: what is saved per segment is `{start, end, pos}`, plus `d.size`, `d.downloaded`, `d.resumable` in the Download JSON. The manager saves running tasks every >5 s and on every finish (`manager.py:482-484`, `:460-463`).
- [ ] Tests:
  - `test_segmented_download_is_byte_exact`: 5 MiB+123, 8 connections, SHA matches, more than one connection, more than one non-`bytes=0-` Range request.
  - `test_pause_and_resume`: pause after ≥512 KB gives PAUSED with segments and `0 < downloaded < size`. A new task on the same Download completes byte-exact, and **no request after the first resumes from `bytes=0-`**.
  - `test_speed_limit`: 600 KB under a 400 KB/s global limit takes more than 1 s.

---

## 2. Manager (`manager.py`)

### 2.1 Construction (`:83-106`)
- [ ] Load downloads in id order. Active statuses (connecting/downloading/assembling) become `PAUSED`. `VERIFYING` becomes `COMPLETE` (unchecked; `checksum_result` is untouched). Test `test_active_status_becomes_paused_after_restart`.
- [ ] Load queues: `Queue(name, QueueSchedule(**data["schedule"]))`. **Quirk:** an unknown schedule key raises TypeError.
- [ ] Ensure `MAIN_QUEUE="Main download queue"` (periodic False) and `SYNC_QUEUE="Synchronization queue"` (periodic True) exist. They are not written to the DB until `save_queue`. Test `test_default_queues_exist`.
- [ ] Global limiter = `speed_limit_kbps if speed_limiter_on else 0`.
- [ ] `_next_id = store.next_id()` = `MAX(id)+1`. **Ids can be reused** after the highest id is deleted.
- [ ] `_started=False`, `_last_save=monotonic()`.

### 2.2 Public methods
- [ ] `client()`: new `HttpClient(ua, proxy, timeout)` (`:109`).
- [ ] `probe(url, referer, username, password, headers)`: probe with `request_headers` of a temporary Download; the response is closed (`:112-115`).
- [ ] `find_duplicate(url)`: first download whose `d.url == url`, exact string match (`:117-121`). The duplicate policy is **(UI)** (`main_window.py:794-813`, `duplicate_action`):
  - `ask` shows a dialog with a "remember" option.
  - `complete` shows the Complete dialog if the duplicate is done, else starts it.
  - `overwrite` replaces it.
  - `number` adds a new numbered copy.
- [ ] `add(url, filename, save_dir, category, description, referer, parent_page, username, password, size=-1, resumable=None, queue_name="", start=False, media=None, headers=None, checksum="")` (`:123-167`):
  - Category: if named, `settings.category(name)` (falls back to the first category). Otherwise `categories.classify(settings, filename, url)`.
  - `save_dir` = argument or the category's `save_dir`. `d.category = cat.name`.
  - The id increments. `queue_name` is applied through `set_queue`. Saved, and started if `start`.
  - The filename is **not** sanitised or made unique here.
  - Test `test_add_classifies_and_completes`: `movie.mkv` goes to Video, gives a `complete` event, and the file ends up in the Video save_dir.
- [ ] `task(id)`, `is_running(id)` (thread alive) (`:170-175`).
- [ ] `_options(d)` (`:177-184`):
  - `max_connections = d.connections or settings.connections_for(host)`, where `connections_for` uses `server_limits` fnmatch on the lower-cased host and pattern (first match in dict order), else `max_connections`.
  - `retries`, `retry_delay` and `temp_dir` come from settings.
- [ ] `start(id)` (`:186-198`):
  - No-op, returning the existing task, if the download is missing, already running, or status is COMPLETE/VERIFYING.
  - Media downloads get `MediaTask`, with a client factory `ua -> HttpClient(ua or settings.user_agent, proxy, timeout)`. Others get `DownloadTask`.
  - Registered in `tasks` and started. An ERROR download simply restarts and resumes if possible.
- [ ] `stop(id)`: `task.stop()` plus `skip_check(id)` (`:200-204`).
- [ ] `verify(id, manual=False)` (`:207-217`):
  - Returns False unless the download exists, has a checksum, is not already verifying, has status COMPLETE/VERIFYING, and its file exists.
  - Sets status VERIFYING, starts a `Verifier`, returns True.
- [ ] `check_when_finished(id, checksum)`: sets checksum, `checksum_force=True`, clears result and actual, saves (`:219-224`). Test `test_check_when_finished_runs_even_with_verification_off`.
- [ ] `record_check(id, checksum, result, actual, seconds)`: stores the "Check hash…" dialog result (`checksum_checked = now`). `verified` resets `checksum_retried=False`. Saves (`:226-233`).
- [ ] `skip_check(id)`: `verifier.skip()`. The download ends Complete with result `skipped` (`:235-239`).
- [ ] `verify_progress(id)`: 0..1 or None (`:241-243`).
- [ ] `stop_all()`: stop every task and set every queue's `running=False`. Verifiers are not stopped (`:274-278`).
- [ ] `redownload(id, overwrite=False, start=True)` (`:280-297`):
  1. No-op if running. Otherwise `skip_check`.
  2. If `overwrite` and the file exists, **delete the file**.
  3. Media: `reset_streams` + `remove_parts`, `size = variant.size or -1`. Otherwise `size = -1`.
  4. Reset `status=QUEUED`, `downloaded=0`, `segments=[]`, `resumable=None`, `error=""`, `checksum_result=""`, `checksum_actual=""`. `checksum_retried` and `checksum_force` are kept.
  5. Save, and start if `start`.
  - The old non-media part file is deleted by the next `_run`.
- [ ] `remove(ids, delete_files=False)` (`:299-319`), per id:
  1. Pop and stop the task, `join(5)`. Pop and skip the verifier, `join(5)`.
  2. Pop the download. Delete `<temp>/<id>.part` and media parts (`<id>-media*`).
  3. If `delete_files` **and status COMPLETE** and the file exists, delete the file.
  - Finally `store.delete_downloads(ids)`. Test `test_remove_and_persist`.
- [ ] `remove_completed()`: removes all COMPLETE downloads and returns their ids (`:321-324`).
- [ ] `update(d)`: save (`:326`).
- [ ] `set_speed_limiter(on, kbps=None)`: optionally set `settings.speed_limit_kbps`, set `speed_limiter_on`, and set the limiter to that kbps or 0 (`:329-333`). **(UI)** `speed_limiter_on_startup` calls `set_speed_limiter(True)` at startup (`main_window.py:120`).
- [ ] `shutdown()`: skip all verifiers, stop all tasks, `join(10)` each, save **all** downloads (`:487-494`).

### 2.3 Queues (`:336-414`)
- [ ] `queue_items(name)`: downloads with `d.queue == name`, sorted by `queue_order`.
- [ ] `set_queue(ids, name)`:
  - `order = max(queue_order in target)` (0 if empty or `name==""`).
  - Each id whose queue differs gets `order += 1`, `queue=name`, `queue_order=order`, and is saved. Ids already in that queue are untouched.
  - `name=""` removes from queues.
- [ ] `move_in_queue(id, delta)`: move by `delta` with the index clamped, renumber 1..n, save all.
- [ ] `add_queue(name)`: `setdefault` plus `save_queue`. `delete_queue(name)`: MAIN and SYNC are protected. Otherwise stop the queue, detach items (`queue=""`), delete from memory and the DB.
- [ ] `save_queue`: stores `{"schedule": asdict(schedule)}`. `running`, `next_run`, `failures` and `_fired` are **not persisted**.
- [ ] `start_queue(name)` (`:376-386`):
  1. Returns `[]` if already running.
  2. Sets `running=True` and clears `failures`.
  3. **Periodic (sync) queue:** every COMPLETE item gets `redownload(overwrite=True, start=False)`, which deletes the file and re-fetches it.
  4. Returns `[Event("queue_started")] + _process_queue`.
- [ ] `stop_queue(name)`: `running=False`, `stop(id)` on every item. No event is emitted.
- [ ] `_process_queue(q)` (`:394-414`):
  - Retry limit = `schedule.retries` if `retries_enabled`, else 0.
  - Waiting items: not running, status not COMPLETE, and (status not ERROR or `failures[id] <= limit`). So an ERROR item is retried up to limit+1 failures, and with retries disabled an ERROR item gets one try.
  - Start up to `max_parallel - active`, in queue order. **Quirk:** a VERIFYING item is "started" (no-op) and counted as active, so the queue waits for verification. A PAUSED item in a running queue is restarted on the next poll.
  - When nothing is active: `running=False`. A periodic queue sets `next_run = time.time() + every_hours*3600 + every_minutes*60`. Emits `Event("queue_done")`.
- [ ] Failures are counted in `poll` when a download ends in ERROR and its queue is running (`:466-469`).
- [ ] Test `test_queue_runs_items_one_at_a_time_in_order`:
  - `move_in_queue(ids[2], -2)` gives the order [2, 0, 1].
  - With max_parallel 1, at most one item runs at a time.
  - All items end COMPLETE, then `queue_done` fires and the queue is not running.

### 2.4 QueueSchedule fields and defaults (`:26-46`)
| Field | Default |
|---|---|
| `max_parallel` | `1` |
| `periodic` | `False` |
| `start_on_startup` | `False` |
| `start_enabled` | `False` |
| `start_time` | `"23:00"` |
| `daily` | `False` |
| `once_date` | `""` (YYYY-MM-DD) |
| `days` | `[0..6]` (0 = Monday) |
| `every_hours` | `2` |
| `every_minutes` | `0` |
| `stop_enabled` | `False` |
| `stop_time` | `"07:30"` |
| `retries_enabled` | `True` |
| `retries` | `10` |
| `open_when_done` | `""` (path) |
| `exit_when_done` | `False` |
| `shutdown_when_done` | `False` |

- [ ] `Queue` runtime fields: `name`, `schedule`, `running=False`, `next_run=0.0`, `failures={}`, `_fired=set()` (`:64-71`).
- [ ] `next_start(now)` (`:48-61`):
  - None if `start_enabled` is off. `now` is truncated to the minute, so the current minute counts.
  - `daily`: first of today+0..7 days at HH:MM that is `>= now` and whose weekday is in `days`, else None.
  - `once_date`: that date at HH:MM if `>= now`, else None.
  - Neither: today at HH:MM if `>= now`, else tomorrow.
  - Test `test_once_daily_and_off`.

### 2.5 Scheduler (`_tick_schedules`, `:417-438`)
- [ ] `_due(t, now)`: `now.strftime("%H:%M") == t`, an exact zero-padded string compare. `stamp = "YYYY-MM-DD HH:MM"`.
- [ ] Start fires when all hold: `start_enabled`, not running, due, and `"start "+stamp` not in `_fired`. The day must also match:
  - `daily`: weekday in `days`.
  - Otherwise: `once_date` empty (**which fires every day**) or `once_date == today`.
  - When it fires, `stamp` is added to `_fired` and `start_queue` runs.
  - Test `test_scheduler_starts_and_stops_queue`: Tuesday does not fire, Monday 03:15 gives `queue_started` and the queue is running.
- [ ] Stop fires when `stop_enabled`, running, due at `stop_time`, and not fired. It runs `stop_queue` and emits `Event("queue_done", message="stopped by schedule")`. `next_run` is not set.
- [ ] Periodic restart: `periodic and next_run and not running and time.time() >= next_run` sets `next_run=0` and runs `start_queue`. It uses wall-clock `time.time()`, not the `now` argument.
- [ ] `start_on_startup`: started on the first `poll()` (`:443-447`).
- [ ] `_fired` only grows, in memory.
- [ ] On-done actions are **(UI)**, in `main_window.py:1445-1476` `_queue_done`, run on **every** `queue_done`, including "stopped by schedule":
  1. Tray message `"<name>" processing stopped/finished`.
  2. `open_when_done` opens the path with `QDesktopServices.openUrl(file://…)` (xdg-open behaviour) (`ui/util.py:56`).
  3. `shutdown_when_done`, which takes priority over exit: a 30 s countdown dialog the user can cancel, then `manager.shutdown()`, then `subprocess.Popen(["systemctl","poweroff"])`.
  4. Otherwise `exit_when_done` quits the app.
  - A per-download equivalent exists in the progress dialog ("Exit LID when done", "Turn off computer when done") (`ui/dialogs/progress.py:149-155,238-243`, `main_window.py:1382-1402`). The Complete dialog is suppressed while the download's queue is running.

### 2.6 `poll(now=None)` order (`:441-485`)
1. [ ] On the first call, start the `start_on_startup` queues.
2. [ ] Drain finished tasks. Remove the task from `tasks` if it is the same object, and skip the download if it was removed. Then:
   - If `check = verify_checksum or checksum_force`, the status is COMPLETE, the download has a checksum, and `verify()` succeeds: save and emit `changed`. `complete` comes later.
   - Otherwise save, then emit:
     - COMPLETE: `complete`.
     - ERROR: count a queue failure if its queue is running, then `error(message=d.error)`.
     - Anything else (PAUSED): `changed`.
3. [ ] Drain verifiers through `_checked`.
4. [ ] `_process_queue` for each queue.
5. [ ] `_tick_schedules(now or datetime.now())`.
6. [ ] More than 5 s since the last save with tasks present: `save_downloads(all task downloads)`. This is the periodic resume persistence.

### 2.7 Checksum flow (`_checked`, `:245-272`)
- [ ] Remove the verifier.
- [ ] `error`: status ERROR, `d.error = "Checksum check failed: <err>"`, save, emit `error`.
- [ ] Otherwise status COMPLETE and `checksum_result = v.result`.
  - Unless the result is `skipped`, also set `checksum_actual`, `checksum_checked = now`, `checksum_seconds`.
  - `verified` resets `checksum_retried=False`. Save.
- [ ] Manual verify: emit `checked(message=result)` and nothing else.
- [ ] Result other than mismatch (`verified`/`computed`/`skipped`): emit `complete`.
- [ ] Mismatch, by `settings.checksum_mismatch`:
  - `redownload` and not `checksum_retried`: set `retried=True`, `redownload(overwrite=True)`, emit `changed(message="checksum mismatch, downloading again")`.
  - Otherwise emit `mismatch(message="kept" if policy=="keep" else "")`. A second mismatch under `redownload` always asks.
- [ ] **(UI)** reaction to `mismatch`:
  - `message=="kept"` shows a tray "file was kept".
  - Otherwise a MismatchDialog with choices: `again` (redownload with overwrite), `delete` (move to trash and remove), or keep (`main_window.py:1410-1440`).
- [ ] Tests:
  - `test_checksum_is_verified_before_complete`: the only non-`changed` event is `complete`, and actual = sha.
  - `test_checksum_mismatch_asks_keeps_or_downloads_again_once`:
    - Under `ask`: a `("mismatch","")` event, status COMPLETE, no `complete` event.
    - Under `redownload`: a single `mismatch` after one automatic re-download (more server requests are seen).
  - `test_skip_check_and_verify_now`: with verification off the result stays `""`. A manual verify gives `("checked","verified")` with no `complete`. `start()` on a complete download does nothing.

### 2.8 Event kinds (`Event(kind, download_id, queue, message)`, `:74-79`)
| Kind | When | Message |
|---|---|---|
| `complete` | Task ends COMPLETE with no auto-check, or the auto-check result is verified/computed/skipped | `""` |
| `error` | Task ends ERROR, or the verifier errors | `d.error` |
| `mismatch` | Auto-check mismatch | `"kept"` or `""` |
| `checked` | Manual verify done | the result |
| `queue_started` | `start_queue` | |
| `queue_done` | Queue drained | `""`, or `"stopped by schedule"` |
| `changed` | Task ends paused or another status; verification started; mismatch auto-redownload | `""`, or `"checksum mismatch, downloading again"` |

---

## 3. Models (`models.py`)
- [ ] Queue names: `MAIN_QUEUE="Main download queue"`, `SYNC_QUEUE="Synchronization queue"` (`:10-11`).
- [ ] `Status` (string values): `queued`, `connecting`, `downloading`, `paused`, `assembling`, `verifying`, `complete`, `error`. `active` = connecting, downloading or assembling (`:14-26`).
- [ ] `Segment(start, end, pos)` covers `[start, end)`; `end = -1` means unknown size.
  - `remaining` = -1 if `end < 0`, else `max(0, end-pos)`.
  - `done` = `end >= 0 and pos >= end` (`:29-46`).
- [ ] `Download` fields and defaults (`:49-82`):

| Field | Default |
|---|---|
| `id` | required |
| `url` | required |
| `filename` | required |
| `save_dir` | required |
| `category` | `"General"` |
| `queue` | `""` |
| `description` | `""` |
| `referer` | `""` |
| `parent_page` | `""` |
| `size` | `-1` |
| `downloaded` | `0` |
| `status` | `QUEUED` |
| `resumable` | `None` (tri-state) |
| `connections` | `0` (0 = use the default) |
| `segments` | `[]` |
| `date_added` | now (float epoch) |
| `last_try` | `0.0` |
| `error` | `""` |
| `username` | `""` |
| `password` | `""` (stored in plaintext JSON) |
| `speed_limit_kbps` | `0` |
| `queue_order` | `0` |
| `headers` | `{}` |
| `media` | `None` |
| `checksum` | `""` (`"algo:hex"`) |
| `checksum_result` | `""` (or verified, mismatch, skipped, computed) |
| `checksum_actual` | `""` |
| `checksum_checked` | `0.0` |
| `checksum_seconds` | `0.0` |
| `checksum_retried` | `False` |
| `checksum_force` | `False` |

- [ ] `path = join(save_dir, filename)`. `progress` = `min(1, downloaded/size)` if `size > 0`, else 1.0 if COMPLETE, else 0.
- [ ] `to_dict`: `asdict`, with `status` as its string value and segments as a list of `{start,end,pos}` dicts (`:94-97`).
- [ ] `from_dict`:
  - Missing status means `queued`. Segments are rebuilt from dicts.
  - **Unknown keys are dropped** and missing keys take defaults. The four required keys must exist (`:99-105`).
- [ ] `media` dict layout (`media.py:234-249`, `media_task.py`):
  - Top level: `{source:"youtube", page_url, video_id, title, uploader, variant: Variant asdict, streams:[...]}`.
  - Each stream starts as `{kind, format_id}`. Runtime keys added later: `url`, `headers`, `ext`, `size`, `segments` (**lists `[start,end,pos]`**, unlike the top-level dicts), `resumable`, `done`, `file`.
  - `downloaded` is popped but never set (legacy).

---

## 4. Store (`store.py`)
- [ ] Path: `data_dir()/downloads.db` = `$XDG_DATA_HOME or ~/.local/share` + `/lider/downloads.db`. The parent directory is created. `":memory:"` is supported (`:20-24`).
- [ ] Exact schema (`:13-16`):
  ```sql
  CREATE TABLE IF NOT EXISTS downloads (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS queues (name TEXT PRIMARY KEY, data TEXT NOT NULL);
  ```
  There is no schema version and no migrations.
- [ ] `data` is the JSON of `Download.to_dict()` (`json.dumps` default, which escapes non-ASCII as `\uXXXX`). Queue `data` is `{"schedule": {...}}`.
- [ ] Operations:
  - `load_downloads`: `SELECT data … ORDER BY id`.
  - `save_downloads`: `INSERT OR REPLACE` via executemany in one transaction.
  - `delete_downloads`.
  - `next_id`: `COALESCE(MAX(id),0)+1`.
  - `load_queues`, `save_queue` (INSERT OR REPLACE), `delete_queue`, `close`.
  - One connection, `check_same_thread=False`, guarded by a lock.
- [ ] Legacy folder migration lives in settings (§5.3), called at startup from `app.py:91`.

---

## 5. Settings (`settings.py`)

### 5.1 Paths
- [ ] `config_dir = ($XDG_CONFIG_HOME or ~/.config)/lider`, `data_dir = ($XDG_DATA_HOME or ~/.local/share)/lider`, `cache_dir = ($XDG_CACHE_HOME or ~/.cache)/lider`. An empty env var counts as unset (`:45-54`).
- [ ] The settings file is `config_dir/settings.json`.
- [ ] `save`: write `settings.tmp` (`with_suffix(".tmp")`) with `indent=2`, then an atomic `replace`; parent dirs are created (`:216-221`).
- [ ] `load`: any OSError, ValueError or TypeError returns full defaults (`:208-213`).
- [ ] `downloads_root = ~/Downloads`. It is `Path.home()/Downloads`, not the XDG user dir.

### 5.2 Every key, type and default (`:110-171`)

| Key | Type | Default |
|---|---|---|
| `launch_on_startup` | bool | False |
| `menu_offered` | bool | False |
| `monitor_clipboard` | bool | True |
| `browser_capture` | bool | True |
| `auto_capture_types` | str | `"3GP 7Z AAC ACE AIF APK ARJ ASF AVI BIN BZ2 EXE GZ GZIP IMG ISO LZH M4A M4V MKV MOV MP3 MP4 MPA MPE MPEG MPG MSI MSU OGG OGV PDF PLJ PPS PPT QT R0* R1* RA RAR RM RMVB SEA SIT SITX TAR TIF TIFF WAV WMA WMV Z ZIP"` (`:12-16`) |
| `no_auto_sites` | str | `""` |
| `show_start_dialog` | bool | True |
| `only_add_to_queue` | bool | False |
| `show_complete_dialog` | bool | True |
| `duplicate_action` | str: ask, number, overwrite, complete | `"ask"` |
| `verify_checksum` | bool | True |
| `checksum_from_clipboard` | bool | False |
| `checksum_mismatch` | str: ask, keep, redownload | `"ask"` |
| `user_agent` | str | `"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36"` |
| `max_connections` | int | 8 (UI choices `CONNECTION_CHOICES = (1,2,4,8,16,24,32)`) |
| `server_limits` | dict[str,int] | `{}` (host glob to connections) |
| `retries` | int | 10 |
| `retry_delay` | float | 3.0 |
| `timeout` | float | 30.0 |
| `categories` | list[Category] | `default_categories()` |
| `temp_dir` | str | `cache_dir()/"parts"` |
| `proxy` | str | `""` |
| `speed_limiter_on` | bool | False |
| `speed_limit_kbps` | int | 200 |
| `speed_limiter_on_startup` | bool | False |
| `columns` | list of `[key, visible, width]` | from `DEFAULT_COLUMNS` (below) |
| `theme` | str: ember, ember-dark, dark | `"ember"` |
| `follow_system` | bool | True |
| `dark_theme` | str | `"ember-dark"` |
| `density` | str: comfortable, compact | `"comfortable"` |
| `toolbar_labels` | bool | True |
| `menubar` | str: native, custom, both | `"native"` |
| `show_categories` | bool | True |
| `show_details` | bool | True |
| `show_statusbar` | bool | True |
| `tray_icon` | bool | True |
| `close_to_tray` | bool | True |
| `start_minimized` | bool | False |
| `list_font` | str | `""` |
| `sort_key` | str | `"date_added"` |
| `sort_desc` | bool | False |
| `window_geometry` | str (base64) | `""` |
| `sidebar_width` | int | 236 |
| `details_height` | int | 210 |
| `search_flags` | int | `0x04` |
| `search_string` | str | `""` |
| `show_progress_dialog` | bool | True |
| `media_kind` | str | `"video"` |
| `media_quality` | str | `""` |

- [ ] `DEFAULT_COLUMNS` as (key, title, visible, width) (`:29-42`):
  - `queue` "Q" T 34
  - `filename` "File Name" T 280
  - `size` "Size" T 92
  - `status` "Status" T 180
  - `time_left` "Time left" T 100
  - `rate` "Transfer rate" T 120
  - `last_try` "Last Try Date" T 130
  - `description` "Description" T 200
  - `date_added` "Date Added" F 130
  - `save_to` "Save To" F 200
  - `referer` "Referer" F 200
  - `parent_page` "Parent Web Page" F 200
- [ ] `Settings.category(name)`: exact name match, else `categories[0]`. `connections_for(host)`: see §2.2.

### 5.3 Migration shims
- [ ] `_from_dict` (`:188-205`):
  1. If `theme` is missing and `dark_mode` is truthy: `theme="ember-dark"`, `follow_system=False`.
  2. If `toolbar_labels` is missing and `toolbar_large` is present: `toolbar_labels = bool(toolbar_large)`.
  3. Unknown keys are ignored. Values are set without type checks.
  4. `categories` is rebuilt with `Category(**c)`. An unknown category key raises TypeError, so **all** settings fall back to defaults.
  5. Columns: any `DEFAULT_COLUMNS` key missing from the saved columns is appended at the end with default visibility and width.
  - Test `test_old_dark_mode_and_toolbar_size_map_to_themes`: `{dark_mode:True, toolbar_large:False}` gives `("ember-dark", False, False)`, and `{}` gives `("ember", True, "ember-dark")`.
- [ ] `migrate_legacy_dirs()` (`:57-87`), with `LEGACY_NAMES=("lidman","lindm")` (newest first):
  1. For each of the config, data and cache dirs, and each legacy sibling folder that exists:
     - If the new folder does not exist, rename old to new.
     - Otherwise **merge**: move each item not already present, then try `rmdir(old)` and ignore failure.
  2. Rewrite the `settings.json` text, plain-string replacing each `<cache parent>/<legacy>` path with the new cache dir (fixes `temp_dir`).
  - Test `test_legacy_lindm_folders_are_migrated`: lindm config is moved, `downloads.db` is merged into an existing `lider/` that has `native-host/`, and `temp_dir` is rewritten to `…/k/lider/parts`.
- [ ] Test `test_settings_round_trip`: `max_connections=16` and an added "vob" extension survive save and load.

---

## 6. Categories (`categories.py`, `settings.py:20-26,94-107`)
- [ ] `Category(name, extensions=[], save_dir="", sites=[], remember_last=False)`.
- [ ] Defaults, in order:
  - `General`: no extensions, `~/Downloads`.
  - `Compressed`: `zip rar r0* r1* arj gz sit sitx sea ace bz2 7z`
  - `Documents`: `doc pdf ppt pps docx pptx`
  - `Music`: `mp3 wav wma mpa ram ra aac aif m4a tsa`
  - `Programs`: `exe msi`
  - `Video`: `avi mpg mpe mpeg asf wmv mov qt rm mp4 flv m4v webm ogv ogg mkv ts tsv`
  - Each non-General category saves to `~/Downloads/<Name>`.
- [ ] `extension(fn)`: last `splitext` suffix, without the dot, lower-cased.
- [ ] `classify(settings, filename, url)` (`:20-32`):
  1. **Site rules win.** Lower-cased URL hostname, fnmatch against each lower-cased pattern in `cat.sites`, first category in list order.
  2. Then extension rules: fnmatch of ext against lower-cased patterns, first category.
  3. Else `categories[0]` (General).
  - fnmatch supports `* ? [seq] [!seq]` and is case-sensitive on POSIX, hence the explicit lower-casing.
- [ ] `is_auto_capture_type(settings, fn)`: ext non-empty and matches any whitespace-split pattern in `auto_capture_types` (case-insensitive) (`:35-37`).
- [ ] `remember_last` is stored. It is used by the Options UI and the file-info dialog **(UI)**.
- [ ] Tests:
  - `test_default_extension_lists`: `a.R01` is Compressed, `.pptx` is Documents, `.flac` is General, `setup.MSI` is auto-capture, `page.html` is not.
  - `test_site_rule_wins`: `*.soundhost.com` sends a `.zip` to Music.

---

## 7. Checksum (`checksum.py`)
- [ ] Algorithms are md5 (32 hex chars), sha1 (40), sha256 (64) and sha512 (128). Titles are MD5, SHA-1, SHA-256, SHA-512.
- [ ] `parse(text, algorithm="auto")` (`:23-52`):
  1. Strip. Empty text returns `""`.
  2. Optional prefix at the start, regex `(md5|sha-?1|sha-?256|sha-?512)\s*[:=]\s*` (case-insensitive). The name is normalised by removing `-` and lower-casing.
  3. Value = first whitespace token, lower-cased. This drops the filename in SHA256SUMS lines (`<hex>  name`, `<hex> *name`).
  4. Exact error texts, which must be kept (they use an em dash and curly quotes):
     - No value: `Not a valid checksum — there is no value after the prefix.`
     - Non-hex: `Not a valid checksum — “<ch>” is not a hex character.`, naming the first bad character.
     - Prefix contradicts the chosen algorithm: `The value is marked <T1> but <T2> is picked.`
     - Algorithm unknown by length: `Not a valid checksum — expected 32, 40, 64 or 128 hex characters, got N.`
     - Wrong length: `Not a valid <T> value — expected N hex characters, got M.`
  5. Returns `"algo:hex"`.
- [ ] `split("a:h")` returns `(a, h)`. `title(cs)` returns the title, or `""`.
- [ ] `Verifier` thread `verify-<id>` (`:66-121`):
  - Hashes in 1 MiB reads. `total = filesize`; `progress = read/total`.
  - `skip()` is checked between blocks and gives result `skipped`.
  - Results: `verified`, `mismatch`, `computed` (empty expected value, e.g. `"md5:"`), `error` (OSError, or ValueError such as an unknown algorithm, with `str(e)`).
  - `seconds` = monotonic duration. `manual` flag. Calls `on_finish(self)`.
- [ ] Tests:
  - `test_accepted_input`: uppercase hex with whitespace, `SHA-256: `, `hex *ubuntu.iso`, the md5 empty-string hash, and forced sha256.
  - `test_verifier_computes_without_an_expected_value`.
  - `test_invalid_input_says_why`: covers "expected 64 … got 62", "got 62", "“g” is not a hex character", and "marked SHA-256 but MD5".

---

## 8. fmt.py
- [ ] `size(n)` (`:10-19`):
  - None or negative: `""`.
  - Under 1024: `"N Bytes"`.
  - Otherwise divide by 1024 per step through KB, MB, GB, TB with `%.2f`. TB is the cap.
  - Examples: `"1.50 KB"`, `"5.00 GB"`.
- [ ] `rate(bps)`: `size(int(bps)) + "/sec"` if `bps > 0`, else `""`.
- [ ] `duration(s)`:
  - Negative or None: `""`.
  - With hours: `"H hour(s) M min"`.
  - With minutes: `"M min S sec"`.
  - Otherwise `"S sec"`.
  - Examples: 125 gives "2 min 5 sec"; 3720 gives "1 hour(s) 2 min".
- [ ] `date(ts)`: `"%b %d %H:%M:%S %Y"` in local time; 0 gives `""`.
- [ ] `status(d)` (`:42-57`):
  - Complete: "Complete". Error: "Error". Connecting: "Connecting...". Assembling: "Assembling...". Verifying: "Verifying...".
  - Queued with 0 bytes downloaded: `""`.
  - Otherwise `"xx.xx%"` if `size > 0`, else `size(downloaded)`.
  - Test: 50/200 paused gives `"25.00%"`.

---

## 9. search.py
- [ ] Flag bits (`:18-25`): `MATCH_CASE=0x01`, `WHOLE_STRING=0x02`, `TARGET_NAME=0x04`, `TARGET_DESCRIPTION=0x08`, `TARGET_LINKS=0x10` (url, parent_page and referer). The default is `TARGET_NAME`, matching the `search_flags` setting.
- [ ] `SearchSettings(query="", flags=NAME, active=False)`.
- [ ] `validate()`: if no target bit is set, **add TARGET_NAME first**, then raise `SearchSettingsError("The checkbox for at least one of the search targets must be enabled")` (`:59-64`).
- [ ] Matcher (`:72-104`):
  - Without match-case, compare with Unicode **full casefold** (`ß` == `ss`; Rust needs full case folding, not `to_lowercase`).
  - Empty field values never match.
  - Whole string means **full-field equality**. Otherwise substring.
  - An empty query matches everything.
  - Field order: filename, description, url, parent_page, referer.
- [ ] `filter_downloads`: inactive returns everything without validating. Active validates (and may raise), then filters (`:107-113`).
- [ ] `FindNext.next(rows)` (`:116-147`):
  1. Empty query: return None without validating.
  2. Validate. If the query changed since `_last_query`, set `cursor=0`.
  3. Scan from `min(cursor,len)` to the end, then wrap once over `0..start`.
  4. On a hit, `cursor = i+1` and return i. With no hit, `cursor=0` and return None.
  - `reset()` sets cursor to 0.
- [ ] Tests (`test_search.py`):
  - Empty query matches all.
  - Case-insensitive by default: SETUP gives [3,4].
  - Match case: Setup gives [4].
  - Casefold: STRASSE gives Straße.
  - Whole string: "setup" gives [], "setup.exe" gives [3,4].
  - Targets are independent: ubuntu.com matches nothing under name and [1] under links.
  - Links include the referer: google gives [3].
  - Description target: lecture gives [5].
  - No target raises and re-enables name.
  - Inactive shows all.
  - FindNext walks 2, 3, then wraps to 2; "missing" gives None.
  - A new query restarts from the top: cursor 4 with old query gives 4, then "ubuntu" gives 0.

---

## 10. Media / YouTube (`media.py`, `media_task.py`)

### 10.1 Detection and tools
- [ ] `YOUTUBE_RE`: `^https?://(?:(?:www|m|music)\.)?(?:youtube\.com/(?:watch\?|shorts/|live/|embed/)|youtu\.be/)\S+`, case-insensitive, applied to `url.strip()` (`media.py:17-26`).
  - Accepted: watch, youtu.be, m.youtube, shorts, music, http live.
  - Rejected: example.com, the bare home page, `youtube.com.evil.io`, `/channel/`.
  - Test `test_youtube_links`.
- [ ] `available()`: yt_dlp is importable. **(UI)** if not, a warning says "needs the yt-dlp package: pip install yt-dlp" (`main_window.py:841`).
- [ ] `ffmpeg()`: `which ffmpeg`, cached.
- [ ] `ffmpeg_has_mp3()`: `ffmpeg -hide_banner -encoders` (10 s timeout) contains `libmp3lame`, cached.
- [ ] `js_runtimes()`: the first of `deno`, `node`, `bun` on PATH gives `{name: {"path": p}}`, else `{}`.
- [ ] yt-dlp is used **as a Python library, not the CLI**: `YoutubeDL(opts).extract_info(url, download=False)`.
  - Options: `quiet`, `no_warnings`, `noprogress`, `skip_download`, `noplaylist` (all True), plus `js_runtimes` when a runtime was found (`:63-75`).
  - A Rust port calling the CLI would roughly need `yt-dlp -J --no-playlist --no-warnings --skip-download [--js-runtimes <name>:<path>]`. This is my mapping; check it against your yt-dlp version.
- [ ] Error text: the `"ERROR: "` prefix is stripped from messages (`media_task.py:170`, UI `main_window.py:850`).

### 10.2 Variant selection (`build_variants`, `:158-212`)
- [ ] Usable formats: `protocol` starts with `"http"`, so https is in and m3u8_native and mhtml are out, and `"drc"` is not in `format_id`.
- [ ] Audio-only: `vcodec=="none"` and acodec not None or `"none"`.
- [ ] `best` = max by `abr or tbr or 0`. `m4a` = best audio with ext m4a; `opus` = best audio with ext webm.
- [ ] `_quality`: `format_note` matching `^(\d{3,4}p)`, else `"{height}p"`. Append `int(fps)` if `fps > 30` (e.g. `720p60`).
- [ ] `_codec`: `avc1` is H.264; `vp9`/`vp09` is VP9; `av01` is AV1; else the vcodec up to the first `.`, upper-cased.
- [ ] `_size`: `filesize or filesize_approx`, else `tbr*1000/8*duration`, else -1.
- [ ] `_fmt_size`:
  - Negative: "size unknown".
  - Under 1 MiB: `"%.0f KB"`.
  - Under 1 GiB: `"%.1f MB"`.
  - Otherwise `"%.2f GB"`.
- [ ] With ffmpeg:
  1. Video-only formats (vcodec set, `acodec=="none"`, height truthy) are grouped by quality.
  2. Per group, pick the min of `(codec rank H.264 0, VP9 1, AV1 2, other 9; -tbr)`.
  3. Pair with audio:
     - H.264 or AV1, and m4a exists: m4a, ext **mp4**.
     - Else if opus exists: opus, ext **webm**.
     - Else if m4a exists: m4a, ext **mkv**.
     - Else skip the group.
  4. `total` = video + audio sizes if both are known, else -1.
  5. Label: `"{q} · {EXT} ({codec}) · {size}"` (U+00B7 middle dot).
- [ ] Without ffmpeg: only progressive formats (both codecs set). Label `"{q} · {EXT} · {size}"`, no audio_format.
- [ ] Video variants are sorted by `(-height, quality)`.
- [ ] Audio variants:
  - `M4A`: `"M4A (AAC {abr} kbps) · size"`, ext m4a, no convert. Offered if m4a exists, even without ffmpeg.
  - `MP3`: if `have_mp3`. `"MP3 (192 kbps, converted) · ~size"`, estimated at `duration*192000/8`, source m4a, convert mp3.
  - `OPUS`: if opus and ffmpeg. `"Opus ({abr} kbps) · size"`, ext opus, convert opus.
- [ ] `Variant(kind, quality, label, ext, video_format="", audio_format="", size=-1, height=0, convert="")`. `MediaInfo(url, id, title, uploader, duration, variants)`.
- [ ] `default_variant(kind, quality)`:
  - Options are the variants of that kind, or all variants.
  - An exact quality match wins.
  - Video: first mp4 with height ≤ 1080, else first. Audio: first.
  - **(UI)** the last pick is remembered in `media_kind`/`media_quality`.
- [ ] `probe(url)`:
  - `url = webpage_url or url`, `title or "video"`, `uploader or channel or ""`, duration as int.
  - Variants are built with `ffmpeg()` and `ffmpeg_has_mp3()`.
- [ ] `suggested_filename`: `safe_filename(title).rstrip(". ") + "." + ext`. Test: `"Review: A07 / A08."` gives `"Review: A07 _ A08.mp4"`.
- [ ] `resolve_streams(page_url, ids)`:
  - Runs a fresh `extract_info`.
  - A missing id raises `LookupError("YouTube no longer offers format {fid} for this video")`.
  - Each stream returns `{url, headers: http_headers, size: _size if filesize present else -1, ext or "bin"}`.
- [ ] `postprocess_command(inputs, out, variant)` (`:267-284`), base `[ffmpeg, -y, -hide_banner, -loglevel, error]`, None if there is no ffmpeg:
  - 2 inputs: `-i V -i A -map 0:v:0 -map 1:a:0 -c copy` (+ `-movflags +faststart` if ext mp4) `OUT`.
  - mp3: `-i IN -vn -c:a libmp3lame -q:a 2 OUT`.
  - opus: `-i IN -vn -c:a copy OUT`.
  - Otherwise: `-i IN -c copy OUT`, which turns fragmented DASH into a regular file.
- [ ] Tests (`test_media.py`, using fixture `youtube_formats.json`: video 8h9mr7EZiA4, duration 501, DASH https + m3u8 + storyboard + drc formats; the "1080p" format has height 960):
  - Qualities are exactly `2160p,1440p,1080p,720p,480p,360p,240p,144p`.
  - 1080p is mp4, 137+140, size 92006373+8106621, label contains "1080p · MP4 (H.264)".
  - 2160p is webm, 313+251.
  - Audio is `[(M4A,140,""),(MP3,140,mp3),(OPUS,251,opus)]` (drc skipped).
  - Without ffmpeg: no convert, and video variants have no audio_format.
  - Default video is 1080p, default audio is M4A, and an explicit 480p is honoured.

### 10.3 MediaTask (`media_task.py`)
- [ ] It has the same surface as DownloadTask. `connections` and `meter` come from the inner task, or empty/idle. `status_text` holds the phase text (`:41-119`).
- [ ] Phase texts:
  - `"Getting the video address..."`
  - `"Receiving video..."` / `"Receiving audio..."`
  - `"Getting a new video address..."`
  - `"Merging video and audio..."`
  - `"Converting to MP3..."`
  - `"Finishing the file..."`
  - `""` when idle.
- [ ] Snapshot:
  - Segments of every stream are concatenated with byte offsets (offset += stream size).
  - The current stream uses the live inner segments. Done streams show as full. Others use saved segments.
  - Connection labels become `"Video: <status>"`.
  - Rate and time left come from `d.size`/`d.downloaded`.
- [ ] `_run` (`:125-177`):
  1. Set `last_try`, `error=""`, CONNECTING.
  2. **Re-resolve every stream URL on every start.** If a known size changed, drop that stream's progress. Update `url`, `headers`, `ext`, and set the size if it was unknown.
  3. `d.size` = sum of stream sizes once all are known.
  4. Streams run sequentially:
     - Skip a done stream whose file exists; reset it if the file is gone.
     - On failure with `"HTTP 403"` in the error: up to 2 refreshes, each waiting 1 s, re-resolving that one stream and retrying.
  5. End state:
     - Stopped: PAUSED.
     - All streams done: `_assemble`.
     - Otherwise ERROR.
  6. Any exception becomes ERROR with its message.
  7. Clear `inner`/`_current`/`status_text`, then call `on_finish`.
- [ ] `_download_stream`:
  - A sub-Download with the same id: url, filename `"<id>-media-<kind>.<ext>"`, `save_dir = temp_dir`, size/resumable/segments from the stream, the parent's speed limit, and the stream's headers (yt-dlp `http_headers` override engine defaults).
  - With no saved segments, delete any stale finished stream file.
  - Client UA = stream header `User-Agent` or the default. `part_name="<id>-media-<kind>"`.
  - Poll every 0.2 s: `d.downloaded = done_before + sub.downloaded`, updating the size.
  - Afterwards persist `segments` (as lists), `resumable`, `size`.
  - COMPLETE: `done=True`, `file = sub.path`.
  - PAUSED: return False.
  - Otherwise error `"<Kind> stream: <err or 'download failed'>"`.
- [ ] `_assemble`:
  1. ASSEMBLING. Output goes to `<temp>/<id>-media-out.<ext>`.
  2. With no ffmpeg: fail with `"ffmpeg is required to finish this download"` if there are 2 inputs or a conversion; otherwise just move the file.
  3. A non-zero ffmpeg exit fails with `"ffmpeg failed: <last 300 chars of stderr>"`. ffmpeg runs blocking and ignores stop.
  4. `mkdir -p save_dir`, `unique_path`, move, set `filename`, `size = downloaded = file size`, remove all parts, COMPLETE.
- [ ] Helpers: `part_prefix = "<id>-media"`. `remove_parts` globs `<temp>/<id>-media*`. `reset_streams` pops `segments,size,resumable,done,file,downloaded`.
- [ ] Manager integration:
  - `redownload` of media resets the streams, removes parts, and sets `size = variant.size`.
  - `remove` deletes media parts.
- [ ] Tests:
  - `test_video_and_audio_are_merged`: the output has audio and video streams, `d.size` equals the file size, and the parts dir ends up empty.
  - `test_audio_converted_to_mp3`: `.mp3` output with codec mp3.
  - `test_pause_resume_refreshes_urls_and_keeps_progress`: PAUSED keeps stream segments or done, and resolve is called **2 times** across 2 starts.
  - `test_forbidden_stream_gets_a_fresh_url`: a first 403 triggers a re-resolve (resolves = 2) and the download completes.
  - `test_resolver_error_is_reported`: ERROR containing "format 137".
  - Live probe, gated by `LIDER_NETWORK_TESTS`.

---

## 11. Test fixture contract (`tests/httpfixture.py`)
- [ ] `FileServer(size, ranges, length, delay_per_chunk, drop_after, data, fail_first)`, HTTP/1.1 on `127.0.0.1:<random>`.
  - URL: `/files/test%20file.bin`. Every path is served.
- [ ] Records each request's `Range` header (`""` if absent) in `requests`.
- [ ] Behaviour:
  - `fail_first` answers the first N requests with 403.
  - Parses `bytes=S-[E]` and returns 206 with `Content-Range bytes S-E/N`. Returns 416 with `bytes */N` if S ≥ N.
  - `ranges=False` always returns 200 with the full body.
  - `length=False` with a non-partial response has no Content-Length and uses `Connection: close`.
  - Always sends `Content-Disposition: attachment; filename="test file.bin"`.
  - Sends 32 KiB chunks with an optional delay.
  - `drop_after` cuts the **first** response after N bytes.
- [ ] The Rust port needs an equivalent fixture to keep these engine and media tests.
