#!/usr/bin/env bash

set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR/web"
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
WEATHER_CONFIG="$REPO_DIR/config/web.yaml" python app.py
