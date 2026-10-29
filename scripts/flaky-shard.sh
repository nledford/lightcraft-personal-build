#!/usr/bin/env bash
# One shard of the flaky hunt (hosted-flaky.yml): builds the crate's tests once, then runs one test (exact name) <runs> times,
# counting passes and failures and keeping the first failure's output.
#
#   flaky-shard.sh <crate> <test> <runs> <shard> <budget minutes>
#
# Results go to $RUNNER_TEMP/flaky/shard-<shard>.txt (counts, in the format flaky-summary.sh reads) and
# first-failure-<shard>.log. Stops early, and says so, when the time budget is spent. Exit status: 0 when every run passed,
# 1 when one failed, 2 when the tests don't build or no test of that name ran (the hunt is meaningless then).
set -uo pipefail
crate=${1:?crate}
test=${2:?test name}
runs=${3:?runs}
shard=${4:?shard}
budget=${5:-240}

out=${RUNNER_TEMP:-/tmp}
if command -v cygpath >/dev/null; then
  native=$(cygpath -m "$out")
  out=$(cygpath -u "$out")
else
  native=$out
fi
export LIGHTCRAFT_CONFIG_DIR="$native/lightcraft-test-config"
mkdir -p "$out/lightcraft-test-config" "$out/flaky"
result="$out/flaky/shard-$shard.txt"
first="$out/flaky/first-failure-$shard.log"
: >"$first"
started=$SECONDS

write_result() { # <runs done> <passed> <failed> <note>
  {
    echo "shard: $shard"
    echo "runs requested: $runs"
    echo "runs done: $1"
    echo "passed: $2"
    echo "failed: $3"
    echo "note: $4"
    echo "seconds: $((SECONDS - started))"
  } >"$result"
}

if ! cargo test -p "$crate" --lib --bins --tests --no-run >"$out/build.log" 2>&1; then
  tail -n 60 "$out/build.log"
  write_result 0 0 0 "the tests of $crate don't build"
  exit 2
fi
done_runs=0
passed=0
failed=0
note=""
for ((i = 1; i <= runs; i++)); do
  if [ $((SECONDS - started)) -gt $((budget * 60)) ]; then
    note="stopped after $done_runs runs: the $budget-minute budget is spent"
    break
  fi
  output=$(cargo test -p "$crate" --lib --bins --tests --no-fail-fast -- --exact "$test" 2>&1)
  rc=$?
  ran=$(printf '%s\n' "$output" | sed -nE 's/^test result: [a-zA-Z]+\. ([0-9]+) passed; ([0-9]+) failed.*/\1 \2/p' | awk '{s += $1 + $2} END {print s + 0}')
  if [ "$ran" -eq 0 ] && [ "$rc" -eq 0 ]; then
    echo "no test named exactly '$test' ran in $crate"
    write_result "$done_runs" "$passed" "$failed" "no test named exactly '$test' ran in $crate"
    exit 2
  fi
  done_runs=$((done_runs + 1))
  if [ "$rc" -eq 0 ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    if [ ! -s "$first" ]; then
      {
        echo "shard $shard, run $i of $runs:"
        printf '%s\n' "$output" | tail -n 200
      } >"$first"
    fi
    echo "run $i FAILED"
  fi
done
write_result "$done_runs" "$passed" "$failed" "${note:-all requested runs done}"
echo "shard $shard: $done_runs runs, $passed passed, $failed failed ${note:+($note)}"
[ "$failed" -eq 0 ]
