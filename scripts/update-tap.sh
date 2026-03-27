#!/usr/bin/env bash
# update-tap.sh <tag>
#
# After pushing a release tag, run this script to:
#   1. Wait for the GitHub Actions release workflow to attach the DMG artifact
#   2. Download the DMG and compute its SHA256
#   3. Clone pelagos-containers/homebrew-tap, update Casks/pelagos-ui.rb
#   4. Commit and push the tap update
#
# Usage:
#   bash scripts/update-tap.sh v0.1.1

set -euo pipefail

TAG="${1:-}"
if [[ -z "$TAG" ]]; then
    echo "usage: $0 <tag>   e.g. $0 v0.1.1"
    exit 1
fi

VERSION="${TAG#v}"   # strip leading 'v'
REPO="pelagos-containers/pelagos-ui"
TAP_REPO="pelagos-containers/homebrew-tap"
DMG_ASSET="pelagos-ui-${VERSION}-aarch64.dmg"

echo "[tap] updating pelagos-ui cask to ${VERSION}"

# ---------------------------------------------------------------------------
# Wait for the release asset to be attached
# ---------------------------------------------------------------------------
echo "[tap] waiting for release artifact on ${REPO} ${TAG}..."
TIMEOUT=1800   # 30 minutes
INTERVAL=30
ELAPSED=0

while true; do
    ASSETS="$(gh release view "${TAG}" --repo "${REPO}" --json assets \
        --jq '[.assets[].name] | join(" ")' 2>/dev/null || echo "")"

    if [[ "$ASSETS" == *"$DMG_ASSET"* ]]; then
        echo "[tap]   artifact present"
        break
    fi

    if (( ELAPSED >= TIMEOUT )); then
        echo "ERROR: timed out waiting for release artifact after ${TIMEOUT}s"
        exit 1
    fi

    echo "[tap]   not ready yet (${ELAPSED}s elapsed) — retrying in ${INTERVAL}s..."
    sleep "$INTERVAL"
    (( ELAPSED += INTERVAL ))
done

# ---------------------------------------------------------------------------
# Download artifact and compute SHA256
# ---------------------------------------------------------------------------
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "[tap] downloading artifact..."
gh release download "${TAG}" \
    --repo "${REPO}" \
    --pattern "${DMG_ASSET}" \
    --dir "${TMPDIR}"

DMG_SHA="$(shasum -a 256 "${TMPDIR}/${DMG_ASSET}" | awk '{print $1}')"
echo "[tap]   dmg sha256: ${DMG_SHA}"

# ---------------------------------------------------------------------------
# Clone tap, update cask, commit, push
# ---------------------------------------------------------------------------
TAP_DIR="${TMPDIR}/homebrew-tap"
git clone "git@github.com:${TAP_REPO}.git" "${TAP_DIR}" --depth=1

CASK="${TAP_DIR}/Casks/pelagos-ui.rb"

sed -i '' "s/^  version \".*\"/  version \"${VERSION}\"/" "${CASK}"
sed -i '' "s/^  sha256 \".*\"/  sha256 \"${DMG_SHA}\"/"  "${CASK}"

echo "[tap] updated Casks/pelagos-ui.rb:"
grep -E 'version|sha256' "${CASK}"

cd "${TAP_DIR}"
git add Casks/pelagos-ui.rb
git commit -m "pelagos-ui ${VERSION}

Update cask to ${VERSION} with release artifact SHA256.

DMG: ${DMG_SHA}"
git push

echo ""
echo "[tap] done — homebrew-tap Casks/pelagos-ui.rb updated to ${VERSION}"
echo "      Users can now: brew upgrade --cask pelagos-ui"
