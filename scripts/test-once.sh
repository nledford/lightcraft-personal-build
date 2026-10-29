#!/usr/bin/env bash
# Builds a crate's tests and runs one test (exact name) <repeat> times, from the root of a source checkout.
#
#   test-once.sh <crate> <test> [repeat]
#
# Exit status as `git bisect run` reads it: 0 every run passed, 1 a run failed (the test is bad), 125 this commit can't be judged:
# the tests don't build, or no test of that name ran (a commit before the test existed).
set -uo pipefail
crate=${1:?crate}
test=${2:?test name}
repeat=${3:-1}
# Tests must not share (or touch) a settings folder across bisect steps: a fresh scratch one per run of this script.
LIGHTCRAFT_CONFIG_DIR=$(mktemp -d "${RUNNER_TEMP:-/tmp}/lightcraft-test-config.XXXXXX")
export LIGHTCRAFT_CONFIG_DIR
log=${RUNNER_TEMP:-/tmp}/test-once-build.log
if ! cargo test -p "$crate" --lib --bins --tests --no-run >"$log" 2>&1; then
  tail -n 30 "$log"
  echo "test-once: the tests of $crate don't build here (exit 125: skip)"
  exit 125
fi
for ((i = 1; i <= repeat; i++)); do
  out=$(cargo test -p "$crate" --lib --bins --tests --no-fail-fast -- --exact "$test" 2>&1)
  rc=$?
  # tests that ran, over every test binary: "test result: ok. 1 passed; 0 failed; ..."
  ran=$(printf '%s\n' "$out" | sed -nE 's/^test result: [a-zA-Z]+\. ([0-9]+) passed; ([0-9]+) failed.*/\1 \2/p' | awk '{s += $1 + $2} END {print s + 0}')
  if [ "$ran" -eq 0 ] && [ "$rc" -eq 0 ]; then
    echo "test-once: no test named exactly '$test' ran in $crate here (exit 125: skip)"
    exit 125
  fi
  if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$out" | tail -n 80
    echo "test-once: run $i of $repeat FAILED (exit 1)"
    exit 1
  fi
done
echo "test-once: $repeat run(s) passed"
