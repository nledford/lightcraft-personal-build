#!/usr/bin/env bash
# Collects the per-group result files that scripts/xtask-group.sh writes (one results/<group>.txt each, gathered from the
# jobs' `xtask-result-*` artifacts) into one run summary: every failed step and test across the jobs, and each job's time.
#
#   xtask-summary.sh <title> <jobs result> [results dir]
#
# <jobs result> is the `needs.<job>.result` of the jobs that ran the groups (success, failure, cancelled, skipped); the exit
# status is zero only for `success`, so this job's conclusion is the one place a person or `just personal-download` reads.
set -uo pipefail
title=${1:?title}
jobs_result=${2:?jobs result}
dir=${3:-results}

{
  echo "### $title"
  echo ""
  echo "Jobs: $jobs_result"
  echo ""
  echo "Failed steps (every job):"
  grep -h '^failed steps:' "$dir"/*.txt 2>/dev/null | sed -E 's/^failed steps: ?//' | tr ' ' '\n' | grep -v '^$' | grep -vx none | sort -u | sed 's/^/- /'
  echo ""
  echo "Failed tests:"
  # shellcheck disable=SC2016 # the backticks are Markdown, not a command
  grep -h '^failed test: ' "$dir"/*.txt 2>/dev/null | sed -E 's/^failed test: /- `/; s/$/`/' | sort -u
  echo ""
  echo "| job | seconds (steps only) |"
  echo "|---|---|"
  for f in "$dir"/*.txt; do
    [ -f "$f" ] && printf '| %s | %s |\n' "$(sed -n 's/^group: //p' "$f")" "$(sed -n 's/^steps seconds: //p' "$f")"
  done
} >>"${GITHUB_STEP_SUMMARY:-/dev/stdout}"

[ "$jobs_result" = success ] || {
  echo "::error::$title: jobs $jobs_result (see the summary above and each job's own summary)"
  exit 1
}
