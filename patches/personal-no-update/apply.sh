#!/usr/bin/env bash
# Run from the root of a source checkout, before building the app: makes this build unable to install a release.
#   1. the app's startup update check stays off, whatever the launch asks for (a Finder or Dock launch passes no flags),
#      and the update prompt never starts an installation (app.patch, or app-dialogs.patch for the layout where
#      updates_ui.rs sits under crates/ui-egui/src/dialogs/);
#   2. the library's prompt (craft-update, a git dependency) never offers UPDATE AND RESTART, so it shows DOWNLOAD
#      (a link to the release page): craft-update.patch, applied to Cargo's own checkout of the dependency.
# Releases don't read my catalog (format 11, the backup chain), so an installed release must never replace this build.
# Fails loudly when a patch does not apply: the caller must not build without it. Help > Check for Updates still checks and
# tells; only the install is gone.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

app=""
for p in app app-dialogs; do
  if git apply --check "$here/$p.patch" 2>/dev/null; then
    app=$p
    break
  fi
done
[ -n "$app" ] || {
  echo "::error::no app patch in $here applies to this source: crates/ui-egui updates_ui.rs changed; add a variant (see README, Personal build: no update installs)"
  git apply --check "$here/app.patch" || true
  git apply --check "$here/app-dialogs.patch" || true
  exit 1
}
git apply "$here/$app.patch"
echo "applied $app.patch"

cargo fetch --locked
root=$(cargo metadata --locked --format-version 1 | jq -r '.packages[] | select(.name == "craft-update") | .manifest_path' | sed -E 's#/crates/update/Cargo.toml$##')
[ -n "$root" ] && [ -f "$root/crates/update/src/dialog.rs" ] || {
  echo "::error::cannot find Cargo's checkout of craft-update (root '$root')"
  exit 1
}
if (cd "$root" && git apply --reverse --check "$here/craft-update.patch" 2>/dev/null); then
  echo "craft-update.patch already applied in $root"
else
  (cd "$root" && git apply "$here/craft-update.patch")
  echo "applied craft-update.patch in $root"
fi

# Prove the edits are in the files that will be compiled.
case $app in app) file=crates/ui-egui/src/updates_ui.rs ;; *) file=crates/ui-egui/src/dialogs/updates_ui.rs ;; esac
grep -q 'show_if(ctx, false)' "$file" || { echo "::error::$file: show_if(ctx, false) missing"; exit 1; }
grep -q 'self.updates_rt.startup_check = false;' "$file" || { echo "::error::$file: startup check not forced off"; exit 1; }
grep -q 'let is_self_updating = false;' "$root/crates/update/src/dialog.rs" || { echo "::error::dialog.rs not patched"; exit 1; }
echo "no-update patches in place"
