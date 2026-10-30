# PRs in the `integration` build

The `integration` branch (the build I run) is LightCraft `main` at `77c0a6adfa` plus the pull requests below, merged or ported together, plus integration-only fixes where two of them meet (1004 commits on top of `main`, merges and fixes included). The PRs were open when they were taken; the State column is their state now. Source: `integration` at `e423a99eea` (committed 2026-10-29 23:46 UTC). This list is written by `just build-prs e423a99eea` and replaced whenever a new build is swapped in.

How each PR was found: a commit in the build that merges, ports, applies, refreshes or records it, naming the head it took (the commit is shown); its head being in the build's history ("head in build"); all of its commits being in the build by patch ("same patches in build"); or a later head of a PR already in the build whose new commits reverse cleanly against the build's tree (docs, locale catalogs and the lock file left out) or are each in the build as a commit with the same subject and author ("later commits ported"; the commit shown is the earlier head's). "Taken at" is the head that went into the build; "Head now" says whether GitHub still has that head ("same") or the PR has moved on (the build holds the older head: no commit, diff or port of the newer one was found in it).

214 PRs: 2 closed, 195 open, 17 open (draft); 214 at their current head, 0 moved since.

## My PRs (213)

| PR | Title | State | Taken at | Head now | In the build through |
|---|---|---|---|---|---|
| [#768](https://github.com/storytold/lightcraft/pull/768) | LR-LIB-FOLDER: a folder of albums shows the photos of the albums inside it | open | `c12dec194b` | same | `b3f07958fe` |
| [#775](https://github.com/storytold/lightcraft/pull/775) | LR-LIB-FOLDER: the view never shows an album that is gone | open | `f4c039451d` | same | `3e072a5adf` |
| [#776](https://github.com/storytold/lightcraft/pull/776) | LR-LIB-FOLDER: smart albums in album folders keep off loops (stacked on #768) | open (draft) | `afa4a732b7` | same | `f4001cb42f` |
| [#777](https://github.com/storytold/lightcraft/pull/777) | LR-LIB-FOLDER: a refused command says why | open | `a9ec9a9d6f` | same | `fd59fc79af` |
| [#800](https://github.com/storytold/lightcraft/pull/800) | Catalog: the log is compacted by the snapshot's size, not every 2000 records | open | `5cfbf11174` | same | `a4a01fab51` |
| [#841](https://github.com/storytold/lightcraft/pull/841) | Catalog: back up the library to a dated folder that shares unchanged files (stacked on #814) | open (draft) | `cc186d03b8` | same | `17bb8e4568` |
| [#856](https://github.com/storytold/lightcraft/pull/856) | Catalog: restore a library from a backup, and keep what it replaces (stacked on #841) | open (draft) | `a057a2875a` | same | `66312abb82` |
| [#870](https://github.com/storytold/lightcraft/pull/870) | Catalog: restore from the File menu, with the backup copied in the background (stacked on #856) | open (draft) | `0f13f98e1e` | same | `d369311546` |
| [#1004](https://github.com/storytold/lightcraft/pull/1004) | Catalog: the backup folder is shown and chosen in Settings (stacked on #870) | open (draft) | `661108b196` | same | `00cb141c52` |
| [#1025](https://github.com/storytold/lightcraft/pull/1025) | Catalog: a library is backed up by itself while it is open, and when LightCraft quits (stacked on #1004) | open (draft) | `cdbaca775a` | same | `9d89de55b3` |
| [#1034](https://github.com/storytold/lightcraft/pull/1034) | Catalog: a library can keep its backups for a time, and old ones go after a new one is made (stacked on #1025) | open (draft) | `a7ece0e2e9` | same | `763154506f` |
| [#1043](https://github.com/storytold/lightcraft/pull/1043) | Catalog: Restore Library from Backup lists the library's backups to choose from (stacked on #1034) | open (draft) | `d6fd5e5796` | same | `79083995cc` |
| [#1054](https://github.com/storytold/lightcraft/pull/1054) | Catalog: what restores replaced goes with the old backups of a library that keeps them for a time (stacked on #1034) | open (draft) | `30931840fe` | same | `7555129e66` |
| [#1075](https://github.com/storytold/lightcraft/pull/1075) | Backup and restore: the smaller items left on #870's list (stacked on #1054) | open (draft) | `8b9c9ae16f` | same | `602fc7b827` |
| [#1095](https://github.com/storytold/lightcraft/pull/1095) | Import: what the photos themselves carry can be left in the files | open | `7227ccb9bf` | same | `5d3d46c522` |
| [#1124](https://github.com/storytold/lightcraft/pull/1124) | Synchronize Folder: the scan can continue in the background | open | `da8e5bd340` | same | `da60e71938` |
| [#1127](https://github.com/storytold/lightcraft/pull/1127) | Synchronize Folder: a Read row for the new photos, and dialogs remember theirs (stacked on #1095) | open | `43f3ee80e0` | same | `b0a24cd9bf` |
| [#1153](https://github.com/storytold/lightcraft/pull/1153) | Library: show several albums, folders, keywords or dates at once (`library.source {ids, paths}`; engine only) | open | `de864e4877` | same | `9643cd32ad` |
| [#1158](https://github.com/storytold/lightcraft/pull/1158) | LR-LIB-DELETE: the engine can move a file to the system Trash and put it back | open | `15777f8568` | same | `aee129e1c7` |
| [#1243](https://github.com/storytold/lightcraft/pull/1243) | LRC-LIB-LIBFOLDERS: Folders skips the folders that only lead to your photos, with Show Parent Folder to list them | open | `59944f6e9b` | same | `07d4f1ea06` |
| [#1244](https://github.com/storytold/lightcraft/pull/1244) | Sidebar: the row of what is shown has a bar, and a folded section says what it holds | open | `77b0863f1f` | same | `c138532b42` |
| [#1303](https://github.com/storytold/lightcraft/pull/1303) | Settings: Backups says where the library stands, with Back Up Now (stacked on #1075) | open (draft) | `d4a2aa041b` | same | `7bbd3d46d8` |
| [#1374](https://github.com/storytold/lightcraft/pull/1374) | LR-PREF-INTERFACE: text sizes have names, the seven type roles of the theme | open | `3e6e473f41` | same | `35104f3d7e` |
| [#1626](https://github.com/storytold/lightcraft/pull/1626) | LR-BEHAV-SIDEBAR-COLLAPSE: the sidebar's trees open as they were left | open | `3d28e091ef` | same | `e6540550c2` |
| [#1631](https://github.com/storytold/lightcraft/pull/1631) | LR-BEHAV-SIDEBAR-SECTIONS: Option-click on a triangle opens or folds the whole branch (stacked on #1626) | open | `ed5922126a` | same | same patches in build |
| [#1642](https://github.com/storytold/lightcraft/pull/1642) | LR-BEHAV-BG-TASKS: running tasks live under the top bar's bell, not over the top-left corner | open | `af25c0f978` | same | `a89674c3d3` |
| [#1649](https://github.com/storytold/lightcraft/pull/1649) | LR-BEHAV-BG-TASKS: an export or import that ended stays in the list under the bell (stacked on #1642 and #3151) | open | `8c2311c6c9` | same | `3506259c79` |
| [#1711](https://github.com/storytold/lightcraft/pull/1711) | LR-BEHAV-BG-TASKS: a background task says how it ended; the engine's own jobs do (part 1 of 3) | open | `1233e35b20` | same | `3506259c79` |
| [#1781](https://github.com/storytold/lightcraft/pull/1781) | LR-BEHAV-EMPTY-STATES: Help ▸ Add Sample Photos and Remove Sample Photos | open | `4cd0e5c423` | same | `2a8611d98a` |
| [#1925](https://github.com/storytold/lightcraft/pull/1925) | LRC-LIB-LIBFOLDERS: photos move to another folder in the background | open | `099ee8dff8` | same | `28cba69227` |
| [#1933](https://github.com/storytold/lightcraft/pull/1933) | LR-BEHAV-BG-TASKS: waiting preview builds are listed under the bell, File > Previews queues, and a task that did not go well can be retried (stacked on #1649 and #2806) | open | `6b4a62b5a6` | same | `3506259c79` |
| [#1936](https://github.com/storytold/lightcraft/pull/1936) | LR-BEHAV-TOAST: toasts stack instead of replacing one another, and a new state replaces the old one | open | `875ef4808c` | same | `a241129fc6` |
| [#1943](https://github.com/storytold/lightcraft/pull/1943) | LR-BEHAV-BG-TASKS: imports and Synchronize Folder wait for each other instead of refusing (stacked on #2806) | open | `1ac561c501` | same | `df9c0731f0` |
| [#2020](https://github.com/storytold/lightcraft/pull/2020) | LR-LIB-SMARTALBUM: creating or giving a keyword no longer freezes the window while albums are counted | open | `83f068807b` | same | `4ecdbfc57c` |
| [#2034](https://github.com/storytold/lightcraft/pull/2034) | LRC-LIB-LIBFOLDERS: Synchronize Folder can bring in files already in the library elsewhere, and remembers the choice | open | `22468741e1` | same | `1ad8de208b` |
| [#2043](https://github.com/storytold/lightcraft/pull/2043) | LRC-LIB-SMARTCOLL: a Same Content as Another Photo rule finds photos whose file is in the library twice | open | `40e6cc3470` | same | `19b048c025` |
| [#2047](https://github.com/storytold/lightcraft/pull/2047) | LRC-LIB-LIBFOLDERS: importing into a large library keeps the window responsive | open | `3d8ca1c08d` | same | `b71253aa95` |
| [#2075](https://github.com/storytold/lightcraft/pull/2075) | LR-BEHAV-DIALOGS: dialog confirm buttons name their action | open | `c2529503fd` | same | `5cdfc6b778` |
| [#2078](https://github.com/storytold/lightcraft/pull/2078) | LR-BEHAV-ACCESS: one tick box, radio and toggle button for every choice | open | `fd8ff7a4ee` | same | `cd45e190b0` |
| [#2079](https://github.com/storytold/lightcraft/pull/2079) | LR-BEHAV-ACCESS: action buttons share one Button component | open | `77487609c8` | same | `bbbcf91c62` |
| [#2080](https://github.com/storytold/lightcraft/pull/2080) | MENU-BAR: one menu row and shortcut component for every menu | open | `901835bdf8` | same | `cd7b22f9b9` |
| [#2162](https://github.com/storytold/lightcraft/pull/2162) | LR-VIEW-PHOTOGRID: the active photo's selection ring shows on light themes and differs from the other selected photos by weight | open | `a5e1cee421` | same | `4452729856` |
| [#2164](https://github.com/storytold/lightcraft/pull/2164) | LR-VIEW-FILMSTRIP: filmstrip and grid thumbnails share one overlay, and it degrades on narrow tiles | open | `b3ab4d98d3` | same | `11808e8487` |
| [#2194](https://github.com/storytold/lightcraft/pull/2194) | MENU-BAR: every command has a place in the menus, and View drops from 62 rows to 36 | open | `4d27821f0b` | same | `8568a90261` |
| [#2204](https://github.com/storytold/lightcraft/pull/2204) | LR-BEHAV-DRAGDROP: photo drags say what a drop will do, refuse what it can't, and Esc gives them up | open | `aae303aaa6` | same | `6a7fad5c49` |
| [#2370](https://github.com/storytold/lightcraft/pull/2370) | LR-BEHAV-TOAST: toasts stay long enough to read, hold under the pointer, and errors stay 6 s | open | `72344858c7` | same | `20f6ea5adb` |
| [#2373](https://github.com/storytold/lightcraft/pull/2373) | LR-BEHAV-ACCESS: Reduce motion stops zoom, scroll and spinner animations, and follows macOS until you choose | open | `8593a919ab` | same | `bf535958bd` |
| [#2378](https://github.com/storytold/lightcraft/pull/2378) | LR-BEHAV-BG-TASKS: detached readers for an import or scan (stall, skip, cap), tested and not used yet | open | `62381956eb` | same | `4475618d0b` |
| [#2414](https://github.com/storytold/lightcraft/pull/2414) | PERF-SELECT: a big selection no longer slows every idle frame | open | `8eec5d37e3` | same | `e06ba9b477` |
| [#2415](https://github.com/storytold/lightcraft/pull/2415) | PERF-LIB: big views list from a kept sort order instead of sorting every photo | open | `ae3b80ebee` | same | `3bbeb033b0` |
| [#2418](https://github.com/storytold/lightcraft/pull/2418) | PERF-SEARCH: typing in the library search keeps up on a large library | open | `ab21a4c809` | same | `d5513230de` |
| [#2419](https://github.com/storytold/lightcraft/pull/2419) | PERF-SIDECARS: imports and moves answer sidecar lookups from the folder listing | open | `27bc794a99` | same | `41865aff5f` |
| [#2423](https://github.com/storytold/lightcraft/pull/2423) | PERF-SMART: test a smart album's rules cheapest first | open | `9673a69bec` | same | `5650252293` |
| [#2459](https://github.com/storytold/lightcraft/pull/2459) | PERF-WALK: a folder walk on a network share lists up to eight folders at the same time | open | `0d22430798` | same | `92ef44bebd` |
| [#2464](https://github.com/storytold/lightcraft/pull/2464) | LRC-LIB-MISSING: one rule for Missing Photos' files, and a folder listing that panics no longer stays running | open | `a895a6a877` | same | `c97b200313` |
| [#2522](https://github.com/storytold/lightcraft/pull/2522) | LR-PREF-GENERAL: Settings tabs look like tabs, and the keyboard focus shows and moves in one short way | open | `3d7fa853a1` | same | `5c6dad69e7` |
| [#2523](https://github.com/storytold/lightcraft/pull/2523) | LR-PREF-GENERAL: Settings asks before Clear Cache, a model's Remove and deleting smart previews | open | `cd9978f45b` | same | `513b45bb8b` |
| [#2524](https://github.com/storytold/lightcraft/pull/2524) | LR-PREF-GENERAL: the Settings tabs are grouped by what a person comes to do | open | `722a62b5f1` | same | `1c6e53caf8` |
| [#2525](https://github.com/storytold/lightcraft/pull/2525) | LR-PREF-GENERAL: what Settings says can be read in every theme, and in every language it has | open | `0a4b86c9dc` | same | `33bfa1ba06` |
| [#2597](https://github.com/storytold/lightcraft/pull/2597) | LRC-LIB-LIBFOLDERS: the folders test fixture waits for the Missing Photos row | open | `2b48d30a4e` | same | `a454f49d8e` |
| [#2629](https://github.com/storytold/lightcraft/pull/2629) | LRC-LIB-MISSING: library.missing from the control channel checks the files off the UI thread | open | `7e9a810ff4` | same | head in build |
| [#2647](https://github.com/storytold/lightcraft/pull/2647) | LR-LIB-SMARTALBUM: editing a smart album made from an album view keeps the album | open | `d6f328df33` | same | `30463be653` |
| [#2649](https://github.com/storytold/lightcraft/pull/2649) | LR-LIB-DELETE: the catalog answers "which photos have a file in this folder / of this name" from an index | open | `0597982300` | same | `77d60d80d1` |
| [#2691](https://github.com/storytold/lightcraft/pull/2691) | LR-PREF-GENERAL: Esc in a Settings text box gives the text back instead of closing Settings | open | `71be070a2f` | same | `8088a65f8b` |
| [#2693](https://github.com/storytold/lightcraft/pull/2693) | "Favorite" and "Listing folders" in the catalogs that lack them | open | `81cf656dc2` | same | `ea1c5328d8` |
| [#2696](https://github.com/storytold/lightcraft/pull/2696) | LRC-LIB-LIBFOLDERS: a count never covers a name in a narrow sidebar | open | `a521f0c591` | same | `ed9775d0cb` |
| [#2697](https://github.com/storytold/lightcraft/pull/2697) | LRC-LIB-FOLDERS: Local says where its listing stops, with a Go Deeper row | open | `2bd55c33db` | same | `ac28884ded` |
| [#2703](https://github.com/storytold/lightcraft/pull/2703) | LR-BEHAV-LOCALIZE: the translation report also lists plain tr() strings a catalog lacks, wrapped calls included | open | `a165b4ee2c` | same | `eaa5f95b5b` |
| [#2704](https://github.com/storytold/lightcraft/pull/2704) | LR-BEHAV-ACCESS: an unnamed control on a main screen fails the tests (today's gaps listed) | open | `267f473c02` | same | `e041704c58` |
| [#2705](https://github.com/storytold/lightcraft/pull/2705) | LR-IMP-FORMATS: a JPEG cut short imports from the data present | open | `632b6550f6` | same | `64b52a76e5` |
| [#2707](https://github.com/storytold/lightcraft/pull/2707) | LR-BEHAV-MULTISELECT: the model for choosing several sidebar rows with Cmd/Ctrl+click and Shift+click | open | `6ffa75d2a5` | same | `9ee77cfb28` |
| [#2715](https://github.com/storytold/lightcraft/pull/2715) | LR-BEHAV-ACCESS: every color pair the UI paints stands out in every theme (today's gaps listed) | open | `72bbc7865b` | same | `dd16a8c147` |
| [#2721](https://github.com/storytold/lightcraft/pull/2721) | LRC-LIB-LIBFOLDERS: a refused move to another folder says what is wrong in plain words; photo.checkMove and skip | open | `0e8106cd1a` | same | `636a7c6c32`, recorded as contained |
| [#2722](https://github.com/storytold/lightcraft/pull/2722) | LR-VIEW-FILMSTRIP: photos drag from the filmstrip like from the grid | open | `4ee7cd5f7e` | same | `7c9d83b2a6` |
| [#2725](https://github.com/storytold/lightcraft/pull/2725) | LRC-LIB-CATALOG-LOCK: quitting on macOS removes the lock's owner note | open | `2d6126396b` | same | `be198a9723` |
| [#2726](https://github.com/storytold/lightcraft/pull/2726) | LR-BEHAV-LOCALIZE: panel layout toasts, face recognition prompts and the second window title are translated | open | `3ce2e47860` | same | `5b56e5e859` |
| [#2734](https://github.com/storytold/lightcraft/pull/2734) | LR-BEHAV-DRAGDROP: the photos being dragged are dimmed in the grid | open | `e34ab8f535` | same | `f8dca0158d` |
| [#2735](https://github.com/storytold/lightcraft/pull/2735) | LR-BEHAV-DRAGDROP: a drop's destination is named with its parent when another folder or album has the same name | open | `e25bd01993` | same | `769f100ada` |
| [#2736](https://github.com/storytold/lightcraft/pull/2736) | LR-BEHAV-DRAGDROP: the message after a drop that moved, added or tagged photos has an Undo button | open | `624d5ac15a` | same | `0f39357692` |
| [#2747](https://github.com/storytold/lightcraft/pull/2747) | LRC-LIB-LIBFOLDERS: moving a folder refuses a missing destination and says why in plain words | open | `cba7d0a18e` | same | `ebd12720b9` |
| [#2748](https://github.com/storytold/lightcraft/pull/2748) | LR-CLOUD-SMARTPREVIEW: the smart-previews folder test cleans up the folder it takes offline | open | `0aca56a1b8` | same | `a4eb99336a` |
| [#2749](https://github.com/storytold/lightcraft/pull/2749) | LRC-LIB-LIBFOLDERS: after moving a folder, the message says where it went, in every language | open | `5ba1f745cf` | same | `3216e00ac0` |
| [#2751](https://github.com/storytold/lightcraft/pull/2751) | LRC-LIB-LIBFOLDERS: several folders move into one in a single undo step (folder.move paths) | open | `d3365f18c2` | same | `0673c34a2c` |
| [#2752](https://github.com/storytold/lightcraft/pull/2752) | LR-BEHAV-DRAGDROP: a Folders row dragged onto another moves that folder on disk, after a question | open | `2e8f7f2d5b` | same | `ccf4cd2b60` |
| [#2756](https://github.com/storytold/lightcraft/pull/2756) | LR-BEHAV-SIDEBAR-SECTIONS: Option-click on a triangle opens or folds the whole branch | open | `e66985a6e9` | same | `e0442cbe84`, recorded as applied |
| [#2757](https://github.com/storytold/lightcraft/pull/2757) | LR-BEHAV-MULTISELECT: several sidebar albums, keywords or folders are deleted, moved or labeled as one undo step (engine) | open | `90556931e4` | same | `c23be6a6fe` |
| [#2758](https://github.com/storytold/lightcraft/pull/2758) | KEYC-RATING: deleting the only keyword of the filter Enable Filters put aside leaves filtering on | open | `b940d87880` | same | `1daf6ffcea` |
| [#2766](https://github.com/storytold/lightcraft/pull/2766) | LR-BEHAV-DRAGDROP: only the primary button drags and drops photos from the grid | open | `3c8cc5d665` | same | `407cbea771` |
| [#2768](https://github.com/storytold/lightcraft/pull/2768) | LR-VIEW-FILMSTRIP: a selected photo's ring shows before its thumbnail loads | open | `83c8c10be2` | same | `7a4633a618` |
| [#2796](https://github.com/storytold/lightcraft/pull/2796) | docs: gap 9 — the CI gate runs the tests on Linux x64; macOS, Windows and the GPU tests still don't | open | `44cb68b8c1` | same | `f5bbfdf36f` |
| [#2797](https://github.com/storytold/lightcraft/pull/2797) | LR-BEHAV-AUTOSAVE: a change that could only be partly taken back is saved and can be undone | open | `a6e65608b6` | same | `77ab798f21` |
| [#2798](https://github.com/storytold/lightcraft/pull/2798) | LR-LIB-SMARTALBUM: the catalog refuses an edit that makes a smart album include itself | open | `f619085609` | same | `40f9c7072e`, recorded as subsumed |
| [#2806](https://github.com/storytold/lightcraft/pull/2806) | LR-BEHAV-BG-TASKS: a preview build asked for while another runs can wait in line for it (engine) | open | `e62b5b6191` | same | `8bbd9625f4` |
| [#2808](https://github.com/storytold/lightcraft/pull/2808) | LR-BEHAV-DRAGDROP: a right-click while photos are carried opens no context menu | open | `8984e385db` | same | `5823ac70e3`, later commits ported |
| [#2812](https://github.com/storytold/lightcraft/pull/2812) | PERF-CATALOG: equal develop settings are held once in memory | open | `f9085a1255` | same | `71a2224b9a` |
| [#2818](https://github.com/storytold/lightcraft/pull/2818) | LRC-LIB-SMARTCOLL: a smart album on a loop of albums holds no photos, and says which album it loops through | open | `695c30c2e6` | same | `6400593c4b` |
| [#2823](https://github.com/storytold/lightcraft/pull/2823) | LR-BEHAV-DRAGDROP: a right-click on a text field while photos are carried opens no menu | open | `790cb76dce` | same | `39a04e6cb8` |
| [#2825](https://github.com/storytold/lightcraft/pull/2825) | LR-LIB-ALBUM: album commands skip photos the library lacks and record nothing when they add nothing | open | `865cca9290` | same | `0c95516edd`, later commits ported |
| [#2828](https://github.com/storytold/lightcraft/pull/2828) | LRC-LIB-SMARTCOLL: reorder rules and groups in the smart album editor by drag or arrow keys | open | `a2f64b1222` | same | `e05d3cd02d` |
| [#2830](https://github.com/storytold/lightcraft/pull/2830) | LR-BEHAV-MULTISELECT: replacing the selection with a repeated id keeps each photo once | open | `de0556f4af` | same | `f5392b5926` |
| [#2848](https://github.com/storytold/lightcraft/pull/2848) | LRC-SS-LAYOUT: the Slideshow settings page no longer moves when the slide preview finishes rendering | open | `c32d776a0e` | same | `0728042218` |
| [#2858](https://github.com/storytold/lightcraft/pull/2858) | LRC-LIB-LIBFOLDERS: Don't ask again when moving photos is kept only when the move started | open | `f912d18dea` | same | `a1c7286962` |
| [#2860](https://github.com/storytold/lightcraft/pull/2860) | LRC-LIB-LIBFOLDERS: the engine can locate a folder that is somewhere else now (engine only) | open | `b6fc1b334b` | same | `646b917197` |
| [#2867](https://github.com/storytold/lightcraft/pull/2867) | Catalog: an upgrade keeps the files it replaces | open | `e373f47e0e` | same | `91fbdde5b9` |
| [#2868](https://github.com/storytold/lightcraft/pull/2868) | LRC-LIB-LIBFOLDERS: an import batch changes only the Folders rows it touches (stacked on #2047) | open | `a247df87ea` | same | `16ad68aaa1` |
| [#2869](https://github.com/storytold/lightcraft/pull/2869) | LR-BEHAV-BG-TASKS: queue a scanned folder without its review while the library lane is busy (stacked on #1943) | open | `5614a0a345` | same | `b582d1d496` |
| [#2871](https://github.com/storytold/lightcraft/pull/2871) | Catalog: the store can write files that a later write names, list its files and sync folders it can | open | `ca03c18954` | same | same patches in build |
| [#2872](https://github.com/storytold/lightcraft/pull/2872) | LR-BEHAV-BG-TASKS: queued imports and syncs name their folder, the dialogs say when they queue, a refused drop says why, switching libraries asks (stacked on #1943) | open | `9f41193890` | same | `25c9f6f9a4` |
| [#2873](https://github.com/storytold/lightcraft/pull/2873) | LR-IMP-FORMATS: a JPEG cut short is marked in the import review, the import message, Info and a smart-album rule (stacked on #2705) | open | `a70e077bab` | same | `00521fb427` |
| [#2874](https://github.com/storytold/lightcraft/pull/2874) | LR-PREF-GENERAL: Settings has a filter that finds a section on whatever tab it is | open | `9547cdcebd` | same | `39a3d66b26` |
| [#2875](https://github.com/storytold/lightcraft/pull/2875) | LRC-LIB-MISSING: the Missing Photos count checks each file once in the background (stacked on #2464) | open | `436edf06a3` | same | same patches in build |
| [#2877](https://github.com/storytold/lightcraft/pull/2877) | LRC-LIB-MISSING: the Missing Photos count follows each file found gone or back by itself (stacked on #2875) | open | `1dbdd1ce86` | same | same patches in build |
| [#2878](https://github.com/storytold/lightcraft/pull/2878) | LR-BEHAV-EMPTY-STATES: a new library asks how to begin, and a banner says when sample photos are in (stacked on #1781) | open | `21b54eaeef` | same | `756134ed26` |
| [#2880](https://github.com/storytold/lightcraft/pull/2880) | LR-BEHAV-EMPTY-STATES: the welcome has the window, with sample photos to open (stacked on #2878) | open | `29adf1aace` | same | `b318bc2782` |
| [#2884](https://github.com/storytold/lightcraft/pull/2884) | LRC-LIB-LIBFOLDERS: ✕ stops a move to another folder after the file in hand (stacked on #1925) | open | `033e4bd923` | same | `19a164fdf2` |
| [#2891](https://github.com/storytold/lightcraft/pull/2891) | LR-BEHAV-EMPTY-STATES: the welcome reads for everyone (stacked on #2880) | open | `a5d272620b` | same | `1ce870b166` |
| [#2892](https://github.com/storytold/lightcraft/pull/2892) | LR-BEHAV-EMPTY-STATES: every way to the samples leads to the editor, which invites a first edit (stacked on #2891) | open | `c0cbb42148` | same | `57aa8c5f41` |
| [#2895](https://github.com/storytold/lightcraft/pull/2895) | LRC-LIB-LIBFOLDERS: a folder that is somewhere else now can be located from the app (stacked on #2860) | open | `d097662a84` | same | `73790fd00f` |
| [#2897](https://github.com/storytold/lightcraft/pull/2897) | LRC-LIB-LIBFOLDERS: dropping photos on a folder asks about what can't move first (stacked on #2721) | open | `57d2314df7` | same | `2d6cb8e306` |
| [#2898](https://github.com/storytold/lightcraft/pull/2898) | LR-PREF-INTERFACE: text is drawn in the seven sizes, so no two sizes are half a point apart (stacked on #1374) | open | `cbc04f85c9` | same | `79bc197511` |
| [#2899](https://github.com/storytold/lightcraft/pull/2899) | LR-PREF-INTERFACE: what the interface writes stands 4.5:1 in every theme (stacked on #2898) | open | `4e986a10ee` | same | `4d4b8a31d8` |
| [#2900](https://github.com/storytold/lightcraft/pull/2900) | LR-PREF-INTERFACE: marks and words over a picture are in one light ink on a shade (stacked on #2899) | open | `3390ebe3ba` | same | `d7f72c9d4b` |
| [#2901](https://github.com/storytold/lightcraft/pull/2901) | LR-PREF-INTERFACE: an exposure reads one way, and values stand in columns (stacked on #2900) | open | `a2540130f4` | same | `ebb2d9f2b1` |
| [#2902](https://github.com/storytold/lightcraft/pull/2902) | LR-BEHAV-LOCALIZE: longer words keep to their place in Edit, the choices and the filter bar (stacked on #2901) | open | `1ab6bec475` | same | `dcac2c2856` |
| [#2903](https://github.com/storytold/lightcraft/pull/2903) | LR-PREF-INTERFACE: emphasis shows, a value is no larger than its header, an empty view's words stay in it (stacked on #2902) | open | `40e911532f` | same | `1fa743f5e0` |
| [#2904](https://github.com/storytold/lightcraft/pull/2904) | LR-PREF-INTERFACE: cargo xtask type refuses a text size written as a number, and CLAUDE.md names the rule (stacked on #2903) | open | `c86a8d4c1c` | same | `f9d00a438e` |
| [#2905](https://github.com/storytold/lightcraft/pull/2905) | LR-PREF-GENERAL: the Settings filter finds what is typed, best first (stacked on #2874) | open | `bd805fc577` | same | `46179bf58b` |
| [#2906](https://github.com/storytold/lightcraft/pull/2906) | LR-PREF-GENERAL: letters the Settings filter found are marked behind a word, not cut out of it (stacked on #2905) | open | `29bdbd326d` | same | `4beefc45c6` |
| [#2907](https://github.com/storytold/lightcraft/pull/2907) | LR-BEHAV-LOCALIZE: the translation report lists English shown without tr() (stacked on #2703) | open | `bb2f76179a` | same | `53488e78f7` |
| [#2911](https://github.com/storytold/lightcraft/pull/2911) | LR-BEHAV-ACCESS: a control Tab skips, or that shows no focus, fails the tests (stacked on #2704) | open | `1734c1483b` | same | `a503d5f59b` |
| [#2914](https://github.com/storytold/lightcraft/pull/2914) | LRC-LIB-LIBFOLDERS: a move writes a record in the library before the first file moves (stacked on #1925) | open | `45195eb8e5` | same | `246ed70819` |
| [#2915](https://github.com/storytold/lightcraft/pull/2915) | LRC-LIB-LIBFOLDERS: an "Unfinished moves" window says what the next open found of a move cut short (stacked on #3119) | open | `715041137f` | same | `39507bedf3` |
| [#2927](https://github.com/storytold/lightcraft/pull/2927) | LR-LIB-DELETE: the plan and the work of Move Files to Trash, run by tests only (stacked on #1158 and #2649) | open | `7491829ad5` | same | `f15776596e` |
| [#2931](https://github.com/storytold/lightcraft/pull/2931) | LR-LIB-DELETE: photo.trashFiles moves photos' files to the system Trash (stacked on #2927) | open | `6f4a3feddc` | same | `e779e216a7` |
| [#2934](https://github.com/storytold/lightcraft/pull/2934) | LR-LIB-DELETE: Move Files to Trash can run in the background, and the commands that move files wait while it does (stacked on #2931) | open | `7216d3aa1b` | same | `01f2bdbcbf` |
| [#2935](https://github.com/storytold/lightcraft/pull/2935) | LRC-LIB-LIBFOLDERS: a folder's menu has one grouped order in Folders and Local | open | `80b2edf96b` | same | `6b2e3fe219` |
| [#2941](https://github.com/storytold/lightcraft/pull/2941) | LR-META-XMP: Save and Read Metadata can run on a whole folder, with a row in the activity stack | open | `edbd915fac` | same | `3cd8786164` |
| [#2943](https://github.com/storytold/lightcraft/pull/2943) | LR-LIB-DELETE: Move Files to Trash… asks, moves the files in the background, and asks again before erasing (stacked on #2934) | open | `5c18e00f3d` | same | `e2e1eb34ae` |
| [#2945](https://github.com/storytold/lightcraft/pull/2945) | Issue #1184: the template allows a tested core whose caller comes in the next pull request | open | `927e8b3ee3` | same | `86796ff727` |
| [#2946](https://github.com/storytold/lightcraft/pull/2946) | LR-LIB-FOLDER: cargo xtask refusals keeps new silent command calls out of the interface (stacked on #777) | open | `e7d34cc018` | same | `4fc34dc6ec` |
| [#2948](https://github.com/storytold/lightcraft/pull/2948) | PERF-CATALOG: a library states what its missing develop settings read as (stacked on #2867) | open | `9755a53017` | same | `435c5cb499` |
| [#2949](https://github.com/storytold/lightcraft/pull/2949) | LR-PREF-GENERAL: the Settings tab bar, filter, choices and questions can be read in every theme (stacked on #2906) | open | `1cb0efe6d6` | same | `387df257db` |
| [#2951](https://github.com/storytold/lightcraft/pull/2951) | PERF-CATALOG: the catalog formats are one table, and every format from 6 on has a fixture (stacked on #2948) | open | `67d40de88c` | same | `baf99d4fa2` |
| [#2954](https://github.com/storytold/lightcraft/pull/2954) | LR-IMP-ADD-DIALOG: an import checks its probes and looks up sidecars several at a time | open | `8ccc688e2e` | same | `e42409ce4a` |
| [#2956](https://github.com/storytold/lightcraft/pull/2956) | LRC-LIB-LIBFOLDERS: a folder can be exported from its menu (stacked on #2935) | open | `8f54ca5d05` | same | `feac05ff43` |
| [#2957](https://github.com/storytold/lightcraft/pull/2957) | PERF-CATALOG: develop settings can be stored as their difference from the defaults (stacked on #2951) | open (draft) | `1a670fc654` | same | `018a66c79b` |
| [#2959](https://github.com/storytold/lightcraft/pull/2959) | LRC-LIB-LIBFOLDERS: every folder menu can copy the folder's path and make a folder on disk (stacked on #2935) | open | `ea5ed0a673` | same | `adb38cc9b8` |
| [#2961](https://github.com/storytold/lightcraft/pull/2961) | LRC-LIB-LIBFOLDERS: a folder's menu can bring photos in (stacked on #2935) | open | `f766c5b8be` | same | `c0bceb3b67` |
| [#2968](https://github.com/storytold/lightcraft/pull/2968) | PERF-CATALOG: catalog records state only what was set (catalog format 10) (stacked on #2957) | open (draft) | `7ef23c5543` | same | `d54375e183` |
| [#2971](https://github.com/storytold/lightcraft/pull/2971) | LR-IMP-DUPES: a photo browsed in Local is offered by Import and no longer blocks the library's own file | open | `aaeb9505c0` | same | `f4af3981e3` |
| [#2973](https://github.com/storytold/lightcraft/pull/2973) | MENU-BAR: clippy passes on macOS again (Quit, About and Settings share one branch) | open | `f780d66842` | same | `21de600207` |
| [#2978](https://github.com/storytold/lightcraft/pull/2978) | PERF-CATALOG: the segments of a snapshot, with no caller yet (stacked on #2968 and #2871) | open (draft) | `0a12bbae62` | same | `a4f682568c` |
| [#2983](https://github.com/storytold/lightcraft/pull/2983) | PERF-CATALOG: the catalog saves its photos in segment files and opens them on several threads (catalog format 11, stacked on #2978) | open (draft) | `18d4323513` | same | `7ecd291bd4` |
| [#2984](https://github.com/storytold/lightcraft/pull/2984) | PERF-CATALOG: segment files nothing lists are removed when a library is opened, and segments without a snapshot are refused (stacked on #2983) | open (draft) | `28d6874df8` | same | `1132aa2d81` |
| [#2985](https://github.com/storytold/lightcraft/pull/2985) | PERF-CATALOG: a compaction writes only the segments whose photos changed (stacked on #2984) | open (draft) | `05c0d0fc87` | same | `ce2eee1ebc` |
| [#3014](https://github.com/storytold/lightcraft/pull/3014) | MENU-CTX-GRID: the photo menu's rows, built from the menu bar's model | open | `7cc8c1386e` | same | `da0e0861f6` |
| [#3026](https://github.com/storytold/lightcraft/pull/3026) | MENU-CTX-GRID: the photo menu offers the Photo menu's actions, grouped by task (stacked on #3014) | open | `22823e1786` | same | `290dda8c9c` |
| [#3093](https://github.com/storytold/lightcraft/pull/3093) | LRC-LIB-KEYWORDS, LRC-LIB-LIBFOLDERS: undo and redo carry what a keyword, person or folder step did to the filter (stacked on #3157) | open | `599876a765` | same | `eab1c1e815` |
| [#3106](https://github.com/storytold/lightcraft/pull/3106) | LR-BEHAV-MULTISELECT: Cmd/Ctrl+click and Shift+click choose several albums, folders, keywords or dates in the sidebars (stacked on #1153 and #2707) | open | `fbdbf78b37` | same | `02ff4c9ef3` |
| [#3107](https://github.com/storytold/lightcraft/pull/3107) | LR-BEHAV-MULTISELECT: the menu, dragging and Delete key of several chosen sidebar rows act on all of them (stacked on #3106 and #2757) | open | `9dca7d9b91` | same | `f98c59ed4e`, later commits ported |
| [#3108](https://github.com/storytold/lightcraft/pull/3108) | LR-BEHAV-MULTISELECT: Cut, Copy and Paste for albums and keywords, and album.duplicate (stacked on #3107) | open | `fc8da0026c` | same | `06fbbeb17b` |
| [#3115](https://github.com/storytold/lightcraft/pull/3115) | LR-BEHAV-MULTISELECT: batch keyword delete and move look keywords up by level instead of scanning the selection (stacked on #2757) | open | `aaf93a28ec` | same | `fb322b5280` |
| [#3119](https://github.com/storytold/lightcraft/pull/3119) | LRC-LIB-LIBFOLDERS: a move cut short by a force-quit or a crash is followed at the next open (stacked on #2914) | open | `84c693f4d0` | same | `521802d0a5` |
| [#3121](https://github.com/storytold/lightcraft/pull/3121) | PERF-CATALOG: a smart album's per-photo test stops storing an answer nothing can read | open | `587529956b` | same | `31a3523e5a` |
| [#3150](https://github.com/storytold/lightcraft/pull/3150) | LR-BEHAV-BG-TASKS: exports and imports say how they ended (stacked on #1711) | open | `94dfa31325` | same | `3506259c79` |
| [#3151](https://github.com/storytold/lightcraft/pull/3151) | LR-BEHAV-BG-TASKS: every other job says how it ended (stacked on #3150) | open | `fd07728df4` | same | `3506259c79` |
| [#3156](https://github.com/storytold/lightcraft/pull/3156) | LR-BEHAV-TEXTFIELD: an edit in progress is saved however its field is left, except Esc | open | `6bf0ae88d4` | same | `c120b927c3` |
| [#3157](https://github.com/storytold/lightcraft/pull/3157) | LRC-LIB-FILTERBAR: every filter change goes through one step that leaves out what the source shown owns | open | `b5cc5f5630` | same | `d4c4c0ff3b` |
| [#3167](https://github.com/storytold/lightcraft/pull/3167) | LR-BEHAV-DIALOGS: a one-line prompt that's refused stays open and says why | open | `16b984831b` | same | `f6a181bed7` |
| [#3171](https://github.com/storytold/lightcraft/pull/3171) | MENU-APP-UPDATES: --no-update-check skips the startup update check | open | `37c0df9166` | same | `28459204ab` |
| [#3176](https://github.com/storytold/lightcraft/pull/3176) | LR-IMP-ADD-DIALOG: an import's probe opens each file once | open | `abb3f03154` | same | `4e93a8a52b` |
| [#3179](https://github.com/storytold/lightcraft/pull/3179) | LR-BEHAV-SIDEBAR-KEYS: the keys' model for the left sidebar: the mode, its cursor and its ring (no caller yet) | open | `86c6eb25ca` | same | `e9ddf869fa` |
| [#3180](https://github.com/storytold/lightcraft/pull/3180) | LR-BEHAV-SIDEBAR-KEYS: the left sidebar from the keyboard (stacked on #3179) | open | `a3fdbaa562` | same | `b73bf92d62` |
| [#3196](https://github.com/storytold/lightcraft/pull/3196) | MENU-BAR: the macOS menu bar keeps every row when two rows run the same command | open | `2deaa50b9e` | same | `4555f85c78` |
| [#3200](https://github.com/storytold/lightcraft/pull/3200) | MENU-BAR: shortcuts list their modifiers in one order everywhere on macOS (⇧⌘I, not ⌘⇧I) | open | `b1d3aa8d6b` | same | `575499d2e1` |
| [#3201](https://github.com/storytold/lightcraft/pull/3201) | LR-BEHAV-LOCALE: the Edit menu drops its 18-row Language submenu; Settings has the choice | open | `b639b4054e` | same | `6c53269154` |
| [#3202](https://github.com/storytold/lightcraft/pull/3202) | MENU-HELP-HELP: on macOS Check for Updates… is in the app menu, and Help opens with LightCraft Help (⇧⌘?) | open | `5cdf2bfea4` | same | `c37baf9adf` |
| [#3203](https://github.com/storytold/lightcraft/pull/3203) | MENU-WINDOW-BRINGFRONT: the macOS Window menu lists the open windows | open | `dffbd44835` | same | `678dd01577` |
| [#3204](https://github.com/storytold/lightcraft/pull/3204) | LR-BEHAV-TITLEBAR: center the macOS traffic lights on the top bar | open | `52d65cbf0b` | same | `66684bbd8b` |
| [#3208](https://github.com/storytold/lightcraft/pull/3208) | LR-BEHAV-BG-TASKS: on macOS a running job keeps the Mac awake and the app out of App Nap | open | `03eb04be63` | same | `955991040e` |
| [#3209](https://github.com/storytold/lightcraft/pull/3209) | LR-BEHAV-ACCESS: the sidebar's disclosure triangles name their row | open | `7d8b2d7e0e` | same | `a82276a91f` |
| [#3211](https://github.com/storytold/lightcraft/pull/3211) | LR-BEHAV-ACCESS: Increase contrast follows macOS until you choose (stacked on #2373) | open | `51eddad39c` | same | `1f1d384f6d` |
| [#3212](https://github.com/storytold/lightcraft/pull/3212) | LR-BEHAV-ACCESS: a date header in the photo grid is named with the text it shows | open | `578f33da7a` | same | `8c6568e777` |
| [#3213](https://github.com/storytold/lightcraft/pull/3213) | MENU-FILE-CLOSE: document that closing the window quits the app | open | `4f6f4c4ee8` | same | `09fa4084a1` |
| [#3214](https://github.com/storytold/lightcraft/pull/3214) | LR-BEHAV-ACCESS: a slider's value field is named "<slider> value" and describes how to use it | open | `cb710b8146` | same | `46dcc6c41a` |
| [#3215](https://github.com/storytold/lightcraft/pull/3215) | LR-BEHAV-ACCESS: Reduce transparency follows macOS until you choose (stacked on #3211) | open | `9ba6eb277f` | same | `da5795217b` |
| [#3216](https://github.com/storytold/lightcraft/pull/3216) | LR-BEHAV-ACCESS: a photo thumbnail reads the same in the grid and the filmstrip: a selectable cell named with its rating, flag and label | open | `4dff333262` | same | `a93d9ee384` |
| [#3217](https://github.com/storytold/lightcraft/pull/3217) | MENU-HELP-FEEDBACK: the macOS Help menu has one feedback row, LightCraft's | open | `ba87b152f8` | same | `aff1c5bf59` |
| [#3218](https://github.com/storytold/lightcraft/pull/3218) | MENU-VIEW-ENTERFULLSCREEN: the macOS View menu has one window full-screen row, Enter or Exit Full Screen on ⌃⌘F | open | `1d037817ab` | same | `7563b5ec3c` |
| [#3219](https://github.com/storytold/lightcraft/pull/3219) | LR-BEHAV-BG-TASKS: on macOS the Dock icon shows how many jobs are running (stacked on #3208) | open | `c8c2a3007a` | same | `89ee17b6ab` |
| [#3220](https://github.com/storytold/lightcraft/pull/3220) | MENU-HELP-HELP: on macOS the Help menu is registered with AppKit after the menu bar is installed | open | `9fabf58cf3` | same | `8efdb749d0` |
| [#3221](https://github.com/storytold/lightcraft/pull/3221) | LR-VIEW-PHOTOGRID: the grid's empty view names Import Photos' shortcut as the keymap has it | open | `1263557666` | same | `470131f671` |
| [#3223](https://github.com/storytold/lightcraft/pull/3223) | LR-BEHAV-BG-TASKS: on macOS a notification tells you when a job has finished (stacked on #3219) | open | `f550887ed5` | same | `cb9a712891` |
| [#3225](https://github.com/storytold/lightcraft/pull/3225) | MENU-EDIT-SELECTNONE: Edit lists Select Only Active Photo and Paste Settings from Previous with their kin, and the macOS Edit menu keeps single separators | open | `d85d975b3c` | same | `fb2d5287c6` |
| [#3243](https://github.com/storytold/lightcraft/pull/3243) | LR-VIEW-FILMSTRIP: a stack badge's tooltip names the stack toggle's key as bound (stacked on #2164) | open | `9e1b8d8086` | same | `d1655b639d` |
| [#3253](https://github.com/storytold/lightcraft/pull/3253) | LR-BEHAV-ACCESS: an icon button's accessible name is its label, with the shortcut as its description | open | `3613a85dcb` | same | `8a69597c71` |
| [#3255](https://github.com/storytold/lightcraft/pull/3255) | LR-BEHAV-LOCALE: Settings ▸ General ▸ Language can sync with the system again (stacked on #3146) | open | `952d7f7980` | same | `8794153b29` |
| [#3259](https://github.com/storytold/lightcraft/pull/3259) | LR-BEHAV-ACCESS: a button that does something reads as a button and one of several choices as a radio button (stacked on #2078) | open | `895a4689b5` | same | `1aa9084593` |
| [#3260](https://github.com/storytold/lightcraft/pull/3260) | LR-BEHAV-ACCESS: a dialog is exposed to screen readers as a dialog, not a group | open | `08a70fa7c3` | same | `acfccf0f49` |
| [#3261](https://github.com/storytold/lightcraft/pull/3261) | LR-BEHAV-EMPTY-STATES: the grid's title, count and empty-view messages are read by screen readers (stacked on #2891) | open | `9d28c5354a` | same | `6422d28e38` |
| [#3262](https://github.com/storytold/lightcraft/pull/3262) | LR-BEHAV-ACCESS: the photo in Detail is named with its file name | open | `5fe7a46f58` | same | `5fc70096df` |
| [#3263](https://github.com/storytold/lightcraft/pull/3263) | LR-BEHAV-ACCESS: a photo thumbnail is a selectable cell, not a check box (stacked on #3216) | closed | `8980ff4cf3` | same | `a4654ec705` |
| [#3265](https://github.com/storytold/lightcraft/pull/3265) | LR-BEHAV-ACCESS: the grid's Thumbnail size reads as a slider (stacked on #2078) | open | `2195f591f9` | same | `f1f53ed2ef` |
| [#3273](https://github.com/storytold/lightcraft/pull/3273) | LR-BEHAV-ACCESS: panels and bars are named as landmarks and toolbars | open | `a0c61edb6e` | same | `09ac9705a0` |
| [#3276](https://github.com/storytold/lightcraft/pull/3276) | LRC-LIB-FOLDERS: the home folder and Local's built-in places can't be renamed or moved | open | `95a80c954c` | same | `555a64f818` |
| [#3287](https://github.com/storytold/lightcraft/pull/3287) | LR-BEHAV-BG-TASKS: an Auto Import taken out of the waiting line gives its files back (stacked on #1943) | open | `a292df9511` | same | `57702e6372` |
| [#3288](https://github.com/storytold/lightcraft/pull/3288) | MENU-VIEW-SORT: the Sort menus offer only what the view uses | open | `f256472e6d` | same | `213bd190b7` |
| [#3289](https://github.com/storytold/lightcraft/pull/3289) | LR-FILT-SORT: a new sort order or reshuffle keeps the active photo in view | open | `f412831402` | same | `86a6ba79e1` |
| [#3290](https://github.com/storytold/lightcraft/pull/3290) | LR-FILT-SORT: a shuffled grid says so in its header, with Reshuffle beside it | open | `c06e0d3c25` | same | `fea9366303` |
| [#3291](https://github.com/storytold/lightcraft/pull/3291) | LR-BEHAV-LOCALIZE: the translation report keeps same-named helpers of different files apart and reads a slider's text (stacked on #2907) | open | `65ca4158f6` | same | `b1363a6a83` |
| [#3294](https://github.com/storytold/lightcraft/pull/3294) | LR-FILT-SORT: Previous Shuffle and a Reshuffle shortcut (stacked on #3288) | open | `cc6fe89ed1` | same | `5c6d1e5fcc` |
| [#3306](https://github.com/storytold/lightcraft/pull/3306) | LR-FILT-SORT: the header's Reshuffle reads as a button beside the Shuffled label (stacked on #3290) | closed | `5a0eb38c3c` | same | `22df3cf831` |
| [#3308](https://github.com/storytold/lightcraft/pull/3308) | LR-BEHAV-TEXTFIELD: a text box's edit is saved to what it was typed for, and a save run at close ends it | open | `566d133437` | same | `9dc64ede48` |

## Other contributors' PRs (1)

| PR | Title | State | Taken at | Head now | In the build through |
|---|---|---|---|---|---|
| [#3146](https://github.com/storytold/lightcraft/pull/3146) | LR-BEHAV-LOCALE: the desktop app starts in the system's language | open | `16736c707a` | same | head in build |

Not listed: my integration PRs [#3074](https://github.com/storytold/lightcraft/pull/3074), [#3162](https://github.com/storytold/lightcraft/pull/3162), [#3163](https://github.com/storytold/lightcraft/pull/3163), [#3170](https://github.com/storytold/lightcraft/pull/3170), [#3195](https://github.com/storytold/lightcraft/pull/3195), each a bundle whose members are listed on their own.

Not in the build as themselves: [#3295](https://github.com/storytold/lightcraft/pull/3295) (another contributor's PR with the same change as a PR above; the build carries my PR's commits).
