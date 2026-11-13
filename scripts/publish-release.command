#!/bin/bash
# Publishes this version on GitHub. Double-click it in Finder, or run
# `bash scripts/publish-release.command` in a terminal.
#
#  1. commits and pushes everything in the repository;
#  2. pushes the tag below, which starts the "Release" workflow on GitHub: it builds the editor
#     for Windows, macOS and Linux and attaches the zips to a new release (about 15 minutes);
#  3. meanwhile, builds and packages the macOS app locally into dist/ as a backup.
TAG="v0.12.0-beta"
cd "$(dirname "$0")/.." || exit 1
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

# GitHub only runs workflows from .github/workflows; the copies live in scripts/workflows.
mkdir -p .github/workflows && cp scripts/workflows/*.yml .github/workflows/
# files from 0.2.0 that were renamed or replaced
rm -f docs/github-ci.yml docs/cryo-editor.png testar.command teste-log.txt \
      crates/cryo-editor/template/NovoScript.cryo \
      crates/cryo-editor/template/scripts/{CameraLivre,CameraOrbital,CameraSegue,Jogador,Moeda}.cryo

echo "== 1/3  Commit and push =="
git add -A
git commit -m "Cryo Engine $TAG: timeline animation, cutscenes, timers and mesh editing" || echo "(nothing new to commit)"
if ! git push origin HEAD; then
  echo
  echo "Could not push from the terminal. Push with GitHub Desktop instead, then create the"
  echo "tag $TAG there (History > right-click the last commit > Create Tag) and push it."
  PUSH_FAILED=1
fi

if [ -z "$PUSH_FAILED" ]; then
  echo "== 2/3  Tag $TAG =="
  git tag -f "$TAG"
  git push -f origin "$TAG" && echo "Tag pushed. Follow the build at: https://github.com/Victor-477/Cryo-Gen/actions"
fi

echo "== 3/3  Local macOS build (backup) =="
if command -v cargo >/dev/null; then
  cargo build --release -p cryo-editor && bash scripts/package.sh "$TAG" "macos-$(uname -m)" && open dist
else
  echo "cargo not found; skipping the local build."
fi

echo
echo "Done. The release will appear at https://github.com/Victor-477/Cryo-Gen/releases"
