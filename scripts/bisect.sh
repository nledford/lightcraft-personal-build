#!/usr/bin/env bash
# Correctness bisect (hosted-bisect.yml): finds the first commit between <good> and <bad> where one test fails, with
# `git bisect run` over scripts/test-once.sh. From the root of a checkout with full history, scripts at $TEST_ONCE.
#
#   bisect.sh <good> <bad> <crate> <test> [repeat]
#
# Checks first that <bad> fails and <good> passes (a bisect over a test that doesn't behave that way names a random commit).
# Commits whose tests don't build, or where the test doesn't exist yet, are skipped (git bisect skip, exit 125); when only
# skipped commits remain, git names a range, and the summary says so. Not for performance: hosted runners are shared and noisy.
set -uo pipefail
good=${1:?good}
bad=${2:?bad}
crate=${3:?crate}
test=${4:?test}
repeat=${5:-1}
once=${TEST_ONCE:?path of test-once.sh}
out=${RUNNER_TEMP:-/tmp}
summary=${GITHUB_STEP_SUMMARY:-/dev/stdout}

git rev-parse --verify --quiet "$good^{commit}" >/dev/null || { echo "::error::no commit '$good' in this history"; exit 2; }
git rev-parse --verify --quiet "$bad^{commit}" >/dev/null || { echo "::error::no commit '$bad' in this history"; exit 2; }
good_sha=$(git rev-parse "$good^{commit}")
bad_sha=$(git rev-parse "$bad^{commit}")
git merge-base --is-ancestor "$good_sha" "$bad_sha" || { echo "::error::good ($good_sha) is not an ancestor of bad ($bad_sha)"; exit 2; }

echo "::group::bad end: $bad_sha"
git checkout -q "$bad_sha"
bash "$once" "$crate" "$test" "$repeat"
rc=$?
echo "::endgroup::"
if [ "$rc" -ne 1 ]; then
  echo "::error::the test does not fail at bad $bad_sha (test-once exit $rc: 0 passes, 125 can't be judged)"
  exit 2
fi
echo "::group::good end: $good_sha"
git checkout -q "$good_sha"
bash "$once" "$crate" "$test" "$repeat"
rc=$?
echo "::endgroup::"
if [ "$rc" -ne 0 ]; then
  echo "::error::the test does not pass at good $good_sha (test-once exit $rc: 1 fails, 125 can't be judged)"
  exit 2
fi

git bisect start "$bad_sha" "$good_sha" >/dev/null
git bisect run bash "$once" "$crate" "$test" "$repeat" 2>&1 | tee "$out/bisect-run.log"
log=$(git bisect log)
first=$(sed -nE "s/^([0-9a-f]{40}) is the first '?bad'? commit\$/\\1/p" "$out/bisect-run.log" | head -1)
{
  echo "### Bisect: $crate $test"
  echo ""
  echo "- good \`${good_sha:0:12}\`, bad \`${bad_sha:0:12}\`, $repeat run(s) per commit"
  if [ -n "$first" ]; then
    echo "- **first bad commit:** \`$first\`"
    echo ""
    echo '```'
    git log -1 --format='%H%n%an <%ae>%n%ad%n%n%s' "$first"
    echo '```'
  else
    echo "- no single first bad commit; skipped commits leave a range (below)"
    sed -n '/might be/,$p' "$out/bisect-run.log" | head -40
  fi
  echo ""
  echo "<details><summary>bisect log</summary>"
  echo ""
  echo '```'
  printf '%s\n' "$log"
  echo '```'
  echo ""
  echo "</details>"
} >>"$summary"
git bisect reset >/dev/null 2>&1
[ -n "$first" ]
