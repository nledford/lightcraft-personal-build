# Catalog persistence: when the log is compacted

The catalog is two files in the library folder (`crates/catalog/src/journal.rs`): `catalog.snap`, a JSON snapshot of
the whole catalog, and `catalog.log`, one JSON line per change made since. Every change is appended to the log and
fsynced before its command returns; now and then the log is compacted into a new snapshot. This page covers when that
happens and what it costs, and what happens to a library's files when a newer version opens it.

## When the log is compacted

A snapshot rewrites the whole catalog, so its cost grows with the library. `SnapshotPolicy`:

| When | Rule |
|---|---|
| While working | The log is long (2000 records or 16 MB) **and** has grown to a quarter of the snapshot's size; or it has reached 256 MB. |
| On quit, snapshot up to 64 MB | Whatever log there is. |
| On quit, larger snapshot | Only when the log is long (2000 records or 16 MB). |

Before, the log was compacted every 2000 records or 16 MB, and on every quit. For a small library nothing changes. For
a large one, saving follows what changed and not the library's size, and after a quit the log is still never longer
than 2000 records or 16 MB, so what a damaged record in it can cost is what it was. A session that ends without a quit
(a crash, a power cut) leaves the log as long as it had grown, up to a quarter of the snapshot or 256 MB; a damaged
record in it costs the records after it, which are kept as `catalog.log.damaged-<seq>`.

Writing a few times the final snapshot during an import holds up to a snapshot of 1 GB (four times the 256 MB cap).
Beyond it the cap is what makes a log due, and the snapshot is rewritten once per 256 MB of log.

## Measurements

`cargo run --release -p lightcraft-catalog --example persist_bench -- <scratch dir> <photos…>`, on an Apple M1 Max
(internal SSD), 2026-10-23, with the machine busy. "Before" is the same benchmark with the old rule (`SnapshotPolicy`
set to compact every 2000 records or 16 MB, and on every quit). The library is 90 % Local browse records of raws on
their import defaults and 10 % library photos with one edit and one history step; a record is 3.7 kB.

### Importing (one change per photo, compacting whenever it is due)

| Photos | Compactions before | Snapshots written before | Time before | Compactions after | Snapshots written after | Time after |
|---|---|---|---|---|---|---|
| 10,000 | 5 | 0.11 GB | 1.7 s | 5 | 0.11 GB | 1.9 s |
| 100,000 | 50 | 9.4 GB | 38.1 s | 15 | 2.1 GB | 16.7 s |

Before, the snapshots written grow with the square of the library (25 times the final snapshot at 100,000 photos);
after, they stay at about 6 times the final snapshot. Appending one change took 4.5 to 5.0 ms (median, with its fsync)
in every run, and the time the caller was blocked by a background compaction stayed at 10 to 25 ms.

**What this does not change.** The whole catalog is still held in memory and still written as one file; a compaction
still takes as long as the snapshot is large (1.5 s on a worker thread at 100,000 photos).

## What a missing value reads as

A record may leave a develop setting out (the photo's, and those of its history steps, versions and import look); a
missing field reads as its default. A default that changed in the code would then change every photo relying on it,
so the library says what its missing values read as:

- `catalog.snap` states them (`"defaults"`: the settings of a record with nothing stated, about 2.8 kB once per
  library), and that covers the log beside it. Every setting is stated, also the few whose own JSON is left out at
  their default (the white balance scale, the HDR section, the camera's local tone, Denoise on or off); only the
  rendering process is not, since a record without one is on the first process in every build.
- A build with the same defaults reads the records as they are, at no cost. A build that only has settings the
  library never heard of does too (they take that build's default), and so does one that lacks settings the library
  knows (it would not read them whatever it read them over).
- A build whose defaults differ reads every record over the library's own, then rewrites the library with its own
  before anything is added to it (`LoadReport::other_defaults`). If that can't be written, the library is refused,
  and the next open reads it the same way: its files still state the library's own defaults, or the new snapshot was
  written but the log was not emptied, and the log's records are then skipped as stale.
- Snapshots written before this state none; they are read over the defaults of the build that reads them, as before.

What a missing value reads as in the current build is also pinned by a test (`crates/catalog/src/develop_defaults.json`),
so a changed default is seen when it is made. A setting that is removed or renamed needs a loader of its own.

## Going back to an older version

Opening a library with a newer LightCraft rewrites it in that version's format, and older versions then refuse it
(safely: they say so and change nothing). So that this is not a one-way step, the upgrade first keeps the files the
library was, byte for byte, beside it:

- `catalog.snap.v<format>-<seq>` and `catalog.log.v<format>-<seq>`, where `<format>` is the format they are in and
  `<seq>` the number of the last change the library held when it was upgraded (or, where the upgrade was left for a
  later snapshot, when the library was opened; a library that had no log, or only a log, has only the one file). `Journal::open` reports their names (`LoadReport::kept`) and logs them.
- The kept files stay unless the library is known to be in its old format still: once the upgrade is written,
  nothing that fails after it takes them away.
- A library whose files can't be kept, or whose upgrade can't be written (a full disk, a folder that can't be written
  to), is not upgraded: it opens as it is, with no copy left lying about, and the next open tries again. If what is on
  disk after a failed write can't be read back to say whether the upgrade was written, the library is refused instead
  (its way back stays). A library left like that is upgraded by its next snapshot, during the session or when it is
  closed, and that snapshot keeps the files first too (and is not written when they can't be kept). When a snapshot
  during the session does that, LightCraft tells where the files are kept, as it does at open (a snapshot when the
  library is closed, or switched away from in Settings, has no one left to tell: the files are in the folder, named
  as above). The files kept are the library as it was when it opened (the snapshot, and the log up to where it ended
  then): what was changed since is in the files that replace them, not in the kept ones. (An earlier session of this
  version may have added changes while the library stayed in its old format: they are in the log it opened with, and
  an older version may refuse them.) Until then, changes
  go on being appended to the library as it is, so that they can be saved at all where there is no room for a copy of
  its files. (If the session ends without that snapshot, the next open keeps the log as it is then, which holds
  those changes, and tells so; the files kept earlier stay beside it.)
- A library whose log was damaged is upgraded like any other. What it was is then its snapshot, kept under the name
  above, and its log, kept as `catalog.log.damaged-<seq>`: the snapshot alone is the library without the changes
  that log held. If the snapshot can't be kept, or the damaged log can't be replaced, the library is refused and left
  as it was (changes made in it would be appended after the damage, where the next open would not read them).
- Nothing removes the kept files. They are as large as the library was; LightCraft says so once when it has upgraded
  a library (and `library.info` lists it under `settingsWarnings`). Delete them once the older version is no longer
  wanted. The browser build keeps none (it always runs the newest version).

**To go back**, with LightCraft closed: move `catalog.snap` and `catalog.log` away, then rename the kept pair to
`catalog.snap` and `catalog.log`. The older version opens the library as it was when it was upgraded (where the upgrade came
from a later snapshot: as it was when it was opened, so the changes made between that and the upgrade are not carried
back either); changes made since, in the newer version, are in the files that were moved away.
