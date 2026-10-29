#!/usr/bin/env bash
# Adds up the shards of the flaky hunt: totals, the failure rate, and the first failure's output.
#
#   flaky-summary.sh <title> <shards result> [results dir]
#
# Reads <dir>/shard-*.txt and <dir>/first-failure-*.log (the shards' `flaky-result-*` artifacts). The exit status is nonzero
# when any run failed, a shard stopped short or didn't run: a red run means "flaky (or broken) here"; a green one means
# every requested run passed.
set -uo pipefail
title=${1:?title}
shards_result=${2:?shards result}
dir=${3:-results}

sum() { # <field>
  cat "$dir"/shard-*.txt 2>/dev/null | sed -nE "s/^$1: ([0-9]+)\$/\\1/p" | awk '{s += $1} END {print s + 0}'
}
requested=$(sum 'runs requested')
done_runs=$(sum 'runs done')
passed=$(sum passed)
failed=$(sum failed)
shards=$(find "$dir" -name 'shard-*.txt' 2>/dev/null | wc -l | tr -d ' ')

{
  echo "### $title"
  echo ""
  echo "- shards reporting: $shards (jobs: $shards_result)"
  echo "- runs: $done_runs of $requested requested; passed $passed, failed $failed"
  if [ "$done_runs" -gt 0 ]; then
    echo "- failure rate: $(awk -v f="$failed" -v d="$done_runs" 'BEGIN {printf "%.2f%%", 100 * f / d}')"
  fi
  notes=$(cat "$dir"/shard-*.txt 2>/dev/null | sed -n 's/^note: //p' | grep -vx 'all requested runs done' | sort -u)
  [ -n "$notes" ] && printf '%s\n' "$notes" | sed 's/^/- note: /'
  first=$(find "$dir" -name 'first-failure-*.log' -size +0 2>/dev/null | sort | head -1)
  if [ -n "$first" ]; then
    echo ""
    echo "First failure:"
    echo ""
    echo '```'
    head -c 20000 "$first"
    echo '```'
  fi
} >>"${GITHUB_STEP_SUMMARY:-/dev/stdout}"

if [ "$failed" -gt 0 ]; then
  echo "::error::$failed of $done_runs runs failed: see the summary for the first failure"
  exit 1
fi
if [ "$shards_result" != success ] || [ "$done_runs" -lt "$requested" ] || [ "$requested" -eq 0 ]; then
  echo "::error::the hunt did not complete ($done_runs of $requested runs; shard jobs $shards_result)"
  exit 1
fi
