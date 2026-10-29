#!/usr/bin/env bash
# Runs a group of the `cargo xtask ci` steps (as in xtask/src/main.rs cmd_ci on 29 Oct) from the root of a source checkout, each step
# even after an earlier one failed, as `just ci-rest` does locally (`xtask ci` itself stops at the first failure). Used by
# linux-ci.yml, hosted-test.yml, hosted-flaky.yml and hosted-bisect.yml, so one list of steps serves them all.
#
#   xtask-group.sh <group> [test filter words...]
#
# Groups:
#   lint       type, fmt, clippy, heif clippy, parity, layers, refusals, assets
#   test       the workspace tests (`cargo test`, not nextest: the ui-egui tests take ~30 min under nextest, because every
#              test process rebuilds the demo scene, against under 5 min under cargo test; see xtask/src/ci.rs)
#   heif-doc   heif-test, heif-engine, doctest
#   wasm       the wasm32 build
#   full       every step above, in xtask ci's order
#   macos-only the tests of macOS-only code (see the list below)
# With filter words (any group `test` or `full`...): only `cargo test --workspace` with those words as the test filter.
#
# Environment: REF (for the summary), CACHE (for the summary), RUNNER_TEMP (results go to $RUNNER_TEMP/result/<group>.txt),
# GITHUB_STEP_SUMMARY. Exit status: nonzero when any step failed.
# (Windows' Git Bash: $RUNNER_TEMP is a drive path; the file tools want the POSIX form)
set -uo pipefail

group=${1:?group}
shift
filter=("$@")
out=${RUNNER_TEMP:-/tmp}
native=$out
if command -v cygpath >/dev/null; then
  out=$(cygpath -u "$out")
  native=$(cygpath -m "$native")
fi
export LIGHTCRAFT_CONFIG_DIR="$native/lightcraft-test-config"
mkdir -p "$out/lightcraft-test-config" "$out/result"
log="$out/steps-$group.log"
timing="$out/timing-$group.txt"
: >"$log"
: >"$timing"
failed=""
started=$SECONDS

step() {
  local name=$1 t=$SECONDS r
  shift
  echo "::group::$name"
  if "$@" 2>&1 | tee -a "$log"; then r=ok; else r=FAIL; failed="$failed $name"; fi
  echo "::endgroup::"
  echo "$r $name"
  printf '| %s | %s | %ss |\n' "$name" "$r" "$((SECONDS - t))" >>"$timing"
}

lint() {
  step type cargo xtask type
  step fmt cargo fmt --all -- --check
  step clippy cargo clippy --workspace --all-targets --keep-going -- -D warnings
  step heif cargo clippy -p lightcraft-codecs --features heif --all-targets -- -D warnings
}
checks() {
  for s in parity layers refusals assets; do step "$s" cargo xtask "$s"; done
}
heif_tests() {
  step heif-test cargo test -p lightcraft-codecs -p lightcraft-heif --features lightcraft-codecs/heif --lib --bins --tests --no-fail-fast
  step heif-engine cargo test -p lightcraft-engine --lib --features lightcraft-codecs/heif --no-fail-fast heic
}
workspace_tests() {
  step test cargo test --workspace --lib --bins --tests --no-fail-fast
}
doctests() {
  step doctest cargo test --workspace --doc --no-fail-fast
}
wasm() {
  step wasm cargo xtask wasm
}

if [ ${#filter[@]} -gt 0 ]; then
  # one run of the workspace tests with the given filter words (substrings of test names, `::` paths allowed)
  step test-filter cargo test --workspace --lib --bins --tests --no-fail-fast -- "${filter[@]}"
else
  case $group in
    lint)
      lint
      checks
      ;;
    test) workspace_tests ;;
    heif-doc)
      heif_tests
      doctests
      ;;
    wasm) wasm ;;
    full)
      lint
      heif_tests
      workspace_tests
      doctests
      checks
      wasm
      ;;
    macos-only)
      # Code under cfg(target_os = "macos") or cfg!(target_os = "macos"), found with grep on 29 Oct (4f216e6b58): the app
      # (native menu, editors, HDR display, dialog filter), trash, Apple Photos and export in the engine, the window code,
      # menus, grading and dialog-fit tests in ui-egui, sysmem, the MCP server's Photos test, and segment (Metal). Extend the
      # lists when a macOS-only module appears.
      step app cargo test -p lightcraft --tests --no-fail-fast
      step sysmem cargo test -p lightcraft-sysmem --no-fail-fast
      step segment cargo test -p lightcraft-segment --lib --no-fail-fast
      step mcp cargo test -p lightcraft-mcp --lib --no-fail-fast
      step engine cargo test -p lightcraft-engine --lib --no-fail-fast -- trash:: apple_photos:: tests_export:: export:: memory:: devices:: config::
      step ui-egui cargo test -p lightcraft-ui-egui --lib --no-fail-fast -- window_place:: menus:: tests_grading:: tests_dialog_fit:: photos
      ;;
    *)
      echo "::error::unknown group '$group'"
      exit 2
      ;;
  esac
fi

total=$((SECONDS - started))
{
  echo "### xtask ci $group (${RUNNER_OS:-?}): ${REF:-?}"
  echo ""
  echo "Failed steps:${failed:- none}. Cache: ${CACHE:-?}. Wall time of the steps ${total}s (setup excluded)."
  echo ""
  echo "| step | result | time |"
  echo "|---|---|---|"
  cat "$timing"
  echo ""
  # shellcheck disable=SC2016 # the backticks are Markdown, not a command
  sed -nE 's/^test (.*) \.\.\. FAILED$/- `\1`/p' "$log" | sort -u
} >>"${GITHUB_STEP_SUMMARY:-/dev/null}"
# What the summary job collects: failed steps, failed tests, and the steps' times.
{
  echo "group: $group (${RUNNER_OS:-?})"
  echo "failed steps:${failed:- none}"
  echo "steps seconds: $total"
  sed -nE 's/^test (.*) \.\.\. FAILED$/failed test: \1/p' "$log" | sort -u
  sed -n 's/^/step: /p' "$timing"
} >"$out/result/$group.txt"
[ -z "$failed" ]
