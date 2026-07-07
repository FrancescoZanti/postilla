#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.5.0"
  exit 1
fi

NEW_VERSION="$1"

if ! echo "$NEW_VERSION" | grep -qP '^\d+\.\d+\.\d+$'; then
  echo "Error: version must be in semver format (e.g. 0.5.0)"
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT_DIR"

# Update package.json
sed -i 's/"version": "[0-9.]*"/"version": "'"$NEW_VERSION"'"/' package.json

# Update tauri.conf.json
sed -i 's/"version": "[0-9.]*"/"version": "'"$NEW_VERSION"'"/' src-tauri/tauri.conf.json

# Update Cargo.toml
sed -i 's/^version = "[0-9.]*"/version = "'"$NEW_VERSION"'"/' src-tauri/Cargo.toml

# Regenerate package-lock.json
npm install --package-lock-only

git add -A
git commit -m "Bump version to $NEW_VERSION"

git tag -a "v$NEW_VERSION" -m "Postilla v$NEW_VERSION"

echo "Commit and tag v$NEW_VERSION created. Push with:"
echo "  git push origin main --tags"
