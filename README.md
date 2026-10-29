# lightcraft-personal-build

Personal builds of [LightCraft](https://github.com/storytold/lightcraft) that include my open pull requests. Not for review, and never a source of pull requests: this repository is not a fork.

- `main` holds this README, the workflows in `.github/workflows/`, the shared setup action in `.github/actions/setup/`, the scripts they run in `scripts/`, the no-update patches in `patches/`, and `members.txt` (the nightly test-merge's list).
- Source branches: `integration` (the build I run), `candidate-<name>` (an integration rebuild to compare against it) and `candidate-test-<short sha>` (a head to test, made by the hosted test, flaky and bisect recipes). They are pushed by my local tooling and force-pushed when they change.
- Only public code goes here: pull request heads, `main`, rebuild topic branches. Never a library, settings or anything from my pictures. No secrets are used anywhere; every workflow's token is read-only, except the `keep` job of the Build workflow, which may create and delete releases.
- Compile caches are written only by runs whose `ref` is `integration`: the Build workflow's (macOS app job and the four Linux jobs, each through `cache/save` or the mbx Action's save, switched on by `save-cache`) and a hand-dispatched `hosted-test` of `integration`, which warms the macOS or Windows cache. `just hosted-test` and the other recipes never write one; every other run only restores (`actions/cache/restore`).

LightCraft is MIT OR Apache-2.0 (see `LICENSE-MIT`, `LICENSE-APACHE` and `NOTICE` on each source branch); the ArtCraft name and logos in `docs/brand/` are trademarks under their own terms.

## Build (`build.yml`)

`just personal-build` from the lightcraft tools pushes a branch here and dispatches it with the laptop's craft-fonts commit and rustc version. Each run has two parts:

- **`LightCraft.app` (macOS arm64)**, ad-hoc signed, from storytold's `packaging/macos/package.sh`; refused when the source has no catalog format 11. Artifacts are kept 30 days.
- **Every `cargo xtask ci` step on Linux**, advisory (the app doesn't wait for it), in four parallel jobs from `linux-ci.yml`, then one summary job named `xtask ci steps (Linux)`:

  | job | steps |
  |---|---|
  | `lint` | type, fmt, clippy, heif clippy, parity, layers, refusals, assets |
  | `test` | `cargo test --workspace --lib --bins --tests --no-fail-fast` |
  | `heif-doc` | heif-test, heif-engine, doctest |
  | `wasm` | the wasm32 build |

  Each group runs every one of its steps even after one failed (as `just ci-rest` does; `xtask ci` stops at the first failure). The summary job lists every failed step and test of the four groups in one place, and its conclusion is what `just personal-download` reads. It is not nextest: `xtask/src/ci.rs` documents the ui-egui tests taking about 30 minutes under nextest (every test process rebuilds the demo scene) against under 5 under `cargo test`, and the hosted run has to match the project's gate.
- The Linux jobs build without debug info (`CARGO_PROFILE_DEV_DEBUG=0`) and without incremental state, and go through mbx (`jdx/mr-boxington-action`, `objects` mode, the Cargo shim first on `PATH`) with one cache generation per group, so the four jobs never overwrite each other's entries. `linux-cache: rust-cache` (dependencies only) is the fallback if the Action breaks. The app job keeps its release profile untouched.

### Timings (Linux, one run each)

Measured on 29 Oct on GitHub's shared runners (noisy; wall time from dispatch to the last Linux job's end, the app job runs beside it). The failures in these runs are real failures of those source heads, not of the jobs.

| source head | layout, cache | wall | per group (lint / test / heif-doc / wasm) | run |
|---|---|---|---|---|
| `b5bd8cae70` (`candidate-base`) | one job, 13 steps in sequence, rust-cache (dependencies only) | 48.3 min | one job | [40907018089](https://github.com/nledford/lightcraft-personal-build/actions/runs/40907018089) |
| `b5bd8cae70` | four groups, mbx, no debug info, warm caches | 21.9 min | 11.0 / 21.6 / 6.6 / 3.3 min | [40915029709](https://github.com/nledford/lightcraft-personal-build/actions/runs/40915029709) |
| `4f216e6b58` (`integration`) | four groups, mbx, cold (first run, saved the caches) | 35.3 min | 13.9 / 35.0 / 19.4 / 2.3 min | [40902710326](https://github.com/nledford/lightcraft-personal-build/actions/runs/40902710326) |
| `8794153b29` (`candidate-settings`) | four groups, mbx, warm | 23.7 min | 13.5 / 23.7 / 9.0 / 3.3 min | [40908684952](https://github.com/nledford/lightcraft-personal-build/actions/runs/40908684952) |

The same head `b5bd8cae70` went from 48.3 to 21.9 minutes (2.2 times as fast). The earlier single-job run on `4f216e6b58` took about 43 minutes cold (measured before this work, not repeated), against 35.3 for the four groups cold. The slowest group is `test`, which is `cargo test` (not nextest), so the wall time is its time. The `integration` run is the only one that saves caches; the others restore them.

### Kept builds (`keep` job of `build.yml`)

Each `integration` run (my build) that passes the app job publishes the app as a GitHub prerelease `build-<UTC yyyymmdd-hhmmss>-<short sha>` and keeps the newest 4 `build-*` releases (older ones are deleted with their tags, in the workflow, so retention needs no laptop). Candidate runs publish nothing. Going back is a download: `just personal-kept` (from the lightcraft tools) lists them, `just personal-kept <tag> [dir]` installs one beside my installed app (never over it) and refuses a build whose catalog format differs from my library's unless `--force-format`. The notes say the source sha, the catalog format, rustc and craft-fonts, and "unreleased, ad-hoc signed, not for others".

Licenses: the app embeds craft-fonts faces and Inter, so every kept build carries the same license texts the project's own packaging (`packaging/env.sh` `copy_docs`) puts in `LightCraft.app/Contents/Resources/Licenses`: `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `OFL-Inter.txt` and one `OFL-<family>.txt` per craft-fonts family. The app job fails when one is missing (it counts the OFL files against `craft-fonts/fonts/*/OFL.txt`), and the `keep` job checks the zip again and fails before it publishes anything; it attaches the same files to the release as loose assets. A proof run (`proof-release: true`, ref `integration`) publishes `build-proof-…` instead, which retention never touches: it is deleted by hand after checking.

### Personal build: no update installs

LightCraft's update prompt (`craft_update`) can install a release over the app through a restart helper, and releases don't read my catalog (format 11, the backup chain). A Finder or Dock launch passes no `--no-update-check`, and not every source ref has the flag (`4f216e6b58` doesn't). So the app job applies `patches/personal-no-update/apply.sh` before building, and fails when it does not apply (it never builds without it):

- `app.patch` / `app-dialogs.patch` (one applies, by where `updates_ui.rs` sits): `set_startup_update_check` ignores its argument, so the startup check never runs, and the prompt is drawn with `show_if(ctx, false)`, so no installation can start even if a prompt offered one.
- `craft-update.patch`, applied to Cargo's own checkout of the `craft-update` dependency after `cargo fetch --locked` (no `Cargo.lock` change): the prompt never offers UPDATE AND RESTART, only DOWNLOAD (a link to the release page).
- Help ▸ Check for Updates… stays: it checks when asked and says what it found.

Proved on `4f216e6b58` (no `startup_update_check`) and on `candidate-settings` (`8794153b29`, which has it): both builds applied their variant (`applied app.patch` / `applied app-dialogs.patch`, then `applied craft-update.patch`). The changes touch only this repo; no product pull request.

## Hosted jobs

All run on GitHub-hosted runners from this repo's `main`, with no secrets. The recipes live in the lightcraft tools (`just/integration.just`); each one pushes the ref here as `candidate-test-<short sha>` (a second job on the same head reuses the branch) and prints the run URL (`HOSTED_WATCH=1` waits for it). A job's summary lists every failed step and test; a red run means a failure.

| job | workflow | recipe |
|---|---|---|
| tests on macOS, Windows or Linux | `hosted-test.yml` | `just hosted-test <ref> [os] [scope] [filter…]` |
| flaky hunt | `hosted-flaky.yml` | `just hosted-flaky <ref> <crate> <test> [runs] [shards] [os]` |
| nightly test-merge | `nightly-merge.yml` | `just hosted-members`, `just hosted-merge` |
| correctness bisect | `hosted-bisect.yml` | `just hosted-bisect <good> <bad> <crate> <test> [os] [repeat]` |

`<ref>` is a sha, a branch, `fork/<branch>`, or `pr<N>` / `'#<N>'` (the head of that storytold/lightcraft pull request; an unquoted `#N` is a shell comment). A ref with a commit on no remote is refused (only public code goes here). Jobs share one list of steps (`scripts/xtask-group.sh`), one setup (`.github/actions/setup`) and one summary (`scripts/xtask-summary.sh`). A filtered step (a `filter`, or the `macos-only` lists) that ran zero tests fails; a filter word that matched no test is listed in the summary as a warning. `HOSTED_WATCH=1` exits nonzero when the run wasn't found.

### Tests on any ref (`hosted-test.yml`)

Inputs: `ref`, `os` (`macos` = macos-15 arm64, `windows` = windows-2025, `linux` = ubuntu-24.04), `scope`, `filter`, the craft-fonts commit and rustc version.

- `scope: full`: every `cargo xtask ci` step, each even after a failure, on one runner (macOS, Windows) or in the four Linux groups above.
- `scope: macos-only`: the tests of macOS-only code (macOS only): the app (native menu, editors, HDR display, dialog filter), sysmem, segment (Metal), the MCP server, and the engine and ui-egui modules that hold `cfg(target_os = "macos")` code (trash, Apple Photos, export, window placement, menus, grading, dialog fit). The list is in `scripts/xtask-group.sh`; extend it when a macOS-only module appears.
- `filter`: words for `cargo test`'s name filter (`window_place:: menus::`); only those workspace tests run and `scope` is ignored.
- Windows has rust-cache only (mbx is trialled on macOS and Linux); a Windows build from a cold cache is slow. The cache is read only unless `ref` is `integration` (only a hand-dispatched run: the recipe's ref is always a `candidate-test-<sha>` branch), which saves it.

### Flaky hunt (`hosted-flaky.yml`)

Inputs: `ref`, `crate`, `test` (exact name, as `cargo test -- --exact` takes it), `runs` (default 200), `shards` (default 4, 1-16), `os`, `budget-minutes` (each shard stops starting runs after it, default 240). Each shard builds the crate's tests once and runs the test its share of the runs one after the other; the summary adds up passes and failures, gives the failure rate, and shows the first failure's output. The run is red when any run failed, green when every requested run passed, and red when the test doesn't exist or the hunt didn't finish. Hosted runners are shared and noisy: the rate is "under Actions", not on my Mac. Use it before `just flaky`.

### Nightly test-merge (`nightly-merge.yml`)

At 06:17 UTC and on dispatch: merges the members of `members.txt` (`<PR number> <head sha> [note]` per line, written by `just hosted-members` from my open PRs or an integration inputs file) onto storytold/lightcraft's `main` in file order, each head fetched from my fork (or from the PR's head ref when the fork dropped it). A conflicting member is skipped and the next one is tried on top, so the summary names, per member, merged, already in, or the conflicting files (docs and locale-only conflicts are marked). It also compares each pinned sha with the PR's head ref on storytold/lightcraft and lists every PR whose head has moved. Nothing is built, pushed or commented. Red when a member conflicts, can't be fetched, or its PR head differs from the pinned sha (refresh `members.txt`). GitHub switches scheduled workflows off after 60 days without a push here; `just hosted-members` pushes one when the list changed (a push of any kind does).

### Correctness bisect (`hosted-bisect.yml`)

Inputs: `good`, `bad`, `crate`, `test`, `os`, `repeat`. `bad` is a branch here (its history holds `good`); `git bisect run` builds the crate's tests and runs the test at each step (`scripts/test-once.sh`: exit 125 when the tests don't build or the test doesn't exist yet, so those commits are skipped). It checks first that the test fails at `bad` and passes at `good`, and the summary names the first bad commit. **Correctness only, never performance**: shared runners are noisy, so no time here is evidence.

### Proof runs

Each was a real run on GitHub, dispatched on 29 Oct from this repo's `main` or from the branch that became it:

| what | run | result |
|---|---|---|
| no update installs, `4f216e6b58` and `8794153b29` | [40900391692](https://github.com/nledford/lightcraft-personal-build/actions/runs/40900391692), [40900395938](https://github.com/nledford/lightcraft-personal-build/actions/runs/40900395938) | both built, each applied its patch variant |
| hosted test, `macos-only`, on `main` `77c0a6adfa` | [40914359139](https://github.com/nledford/lightcraft-personal-build/actions/runs/40914359139) | green, 27.6 min (an earlier run, [40902834335](https://github.com/nledford/lightcraft-personal-build/actions/runs/40902834335), was red on a real failing test of its head) |
| hosted test, Linux, filter `round_trip`, on `main` `77c0a6adfa` | [40914331624](https://github.com/nledford/lightcraft-personal-build/actions/runs/40914331624) | green, 9.1 min |
| hosted test, Linux, filter that matches nothing | [40912113064](https://github.com/nledford/lightcraft-personal-build/actions/runs/40912113064) | red by design: "step test-filter ran zero tests" |
| flaky hunt, 60 runs over 3 shards, a deliberately flaky test on a throwaway branch | [40904047135](https://github.com/nledford/lightcraft-personal-build/actions/runs/40904047135) | red by design: failures counted, first failure shown |
| bisect, a deliberately broken test on a throwaway branch | [40904422494](https://github.com/nledford/lightcraft-personal-build/actions/runs/40904422494) | green: named the first bad commit (`9a114caadc`) |
| nightly test-merge, 226 members (a seed list from 29 Oct) | [40919336430](https://github.com/nledford/lightcraft-personal-build/actions/runs/40919336430) | ran 6 min; red as designed: the summary names each conflicting member and each PR whose head moved since `members.txt` was written (24 of the seed list's PRs had) |
| kept build: app job license check, release, retention | [40912660638](https://github.com/nledford/lightcraft-personal-build/actions/runs/40912660638) (`proof-release`) | green: published `build-proof-20261029-163542-4f216e6b58` with the zip and all license files as assets; downloaded with `just personal-kept`, then deleted. The first attempt, [40912022156](https://github.com/nledford/lightcraft-personal-build/actions/runs/40912022156), failed the license check on a wrong expected count (macOS' file system folds craft-fonts' `inter` family and `OFL-Inter.txt` into one file) and was fixed. |
