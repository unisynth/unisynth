#!/usr/bin/env bash
# Renders tools/social-card/card.html to site/social.png (1280×640) with headless Chrome.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
"$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --window-size=1280,640 \
  --virtual-time-budget=2000 --screenshot="$root/site/social.png" "file://$here/card.html" 2>/dev/null
echo "wrote $root/site/social.png"
