#!/usr/bin/env bash
# Test-merges the pinned members (members.txt) onto storytold/lightcraft's main, in file order, and reports every member's
# result: merged, already in main, or conflicting (with the conflicting files; docs-only conflicts are marked). Nothing is built
# and nothing is pushed anywhere: the merges live in a scratch clone that the runner throws away.
#
#   nightly-merge.sh <members file>
#
# members.txt: one member per line, `<PR number> <head sha> [note]`; `#` starts a comment. Each sha is fetched from my fork
# (nledford/lightcraft), or, when the fork has dropped it, from the PR's head ref on storytold/lightcraft (the sha must match).
# A conflicting member is skipped (merge aborted), so the members after it are still tried on top of the ones that merged:
# the report says which PR conflicts first with main and which with the members before it. Exit status: nonzero when a member
# conflicts or can't be fetched.
set -uo pipefail
members=${1:?members file}
[ -s "$members" ] || { echo "::error::members file '$members' is missing or empty (run just hosted-members)"; exit 2; }
members=$(cd "$(dirname "$members")" && pwd)/$(basename "$members")
origin=${ORIGIN_URL:-https://github.com/storytold/lightcraft.git}
fork=${FORK_URL:-https://github.com/nledford/lightcraft.git}
work=${RUNNER_TEMP:-/tmp}/test-merge
summary=${GITHUB_STEP_SUMMARY:-/dev/stdout}

rm -rf "$work"
git clone -q --filter=blob:none "$origin" "$work" || { echo "::error::can't clone $origin"; exit 2; }
cd "$work" || exit 2
git config user.name test-merge
git config user.email test-merge@invalid
git config merge.conflictstyle merge
main_sha=$(git rev-parse HEAD)
git switch -q -c test-merge

# every member head from the fork in one round trip (a sha the fork lacks fails the batch; those are fetched one by one below)
shas=()
while read -r _pr s _rest; do
  case ${_pr:-#} in \#*) continue ;; esac
  [[ ${s:-} =~ ^[0-9a-f]{40}$ ]] && shas+=("$s")
done <"$members"
[ ${#shas[@]} -eq 0 ] || git fetch -q --filter=blob:none "$fork" "${shas[@]}" 2>/dev/null || true

rows=()
bad=0
n=0
while read -r pr sha _note; do
  case ${pr:-#} in \#*) continue ;; esac
  n=$((n + 1))
  if ! [[ $pr =~ ^[0-9]+$ && ${sha:-} =~ ^[0-9a-f]{40}$ ]]; then
    echo "::error::members line '$pr $sha': want '<PR number> <40-hex sha>'"
    rows+=("| #$pr | \`${sha:-?}\` | bad line |")
    bad=$((bad + 1))
    continue
  fi
  where=fork
  if ! git cat-file -e "$sha^{commit}" 2>/dev/null; then
    # not in the batch (the fork lacks it): one try on the fork, then the PR's head ref on storytold/lightcraft
    if ! git fetch -q --filter=blob:none "$fork" "$sha" 2>/dev/null; then
      where="PR head"
      if ! git fetch -q --filter=blob:none origin "refs/pull/$pr/head" 2>/dev/null || [ "$(git rev-parse FETCH_HEAD)" != "$sha" ]; then
        echo "::error::#$pr: $sha is on neither the fork nor the PR's head ref (the PR moved: refresh members.txt)"
        rows+=("| #$pr | \`${sha:0:10}\` | **not fetchable** (the PR head moved or the fork dropped it) |")
        bad=$((bad + 1))
        continue
      fi
    fi
  fi
  if git merge-base --is-ancestor "$sha" HEAD 2>/dev/null; then
    rows+=("| #$pr | \`${sha:0:10}\` | already in the merge so far (main or an earlier member) |")
    continue
  fi
  if mout=$(git merge -q --no-ff --no-edit -m "member #$pr" "$sha" </dev/null 2>&1); then
    rows+=("| #$pr | \`${sha:0:10}\` | merged ($where) |")
  else
    files=$(git ls-files -u | cut -f2 | sort -u)
    if [ -z "$files" ]; then
      # not a content conflict: git refused or failed; say what it said
      git merge --abort >/dev/null 2>&1 || git reset -q --hard HEAD
      rows+=("| #$pr | \`${sha:0:10}\` | **merge failed**: $(printf '%s' "$mout" | tail -n 2 | tr '\n|' '  ') |")
      bad=$((bad + 1))
      continue
    fi
    code=$(printf '%s\n' "$files" | grep -Evc '^(docs/|.*\.md$|crates/ui-egui/locales/)' || true)
    kind="code conflict"
    [ "$code" -eq 0 ] && kind="docs/locale-only conflict"
    # shellcheck disable=SC2016 # the backticks are Markdown
    list=$(printf '%s\n' "$files" | head -8 | sed 's/.*/`&`/' | paste -sd, - | sed 's/,/, /g')
    more=$(printf '%s\n' "$files" | wc -l | tr -d ' ')
    git merge --abort >/dev/null 2>&1 || git reset -q --hard HEAD
    rows+=("| #$pr | \`${sha:0:10}\` | **$kind** in $more file(s): $list |")
    bad=$((bad + 1))
  fi
done <"$members"

{
  echo "### Nightly test-merge"
  echo ""
  echo "main \`${main_sha:0:10}\`; $n member(s) from members.txt, merged in file order (a conflicting member is skipped, the next one is tried on top). Nothing built, nothing pushed."
  echo ""
  echo "| PR | head | result |"
  echo "|---|---|---|"
  printf '%s\n' "${rows[@]:-}"
  echo ""
  echo "Problems: $bad"
} >>"$summary"
[ "$n" -gt 0 ] || { echo "::error::members file lists no members"; exit 2; }
[ "$bad" -eq 0 ]
