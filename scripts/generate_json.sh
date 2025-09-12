#!/usr/bin/env bash

set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$REPO_DIR/target/release/weather-app" \
  --config "$REPO_DIR/config/rust.yaml" \
  --out "$REPO_DIR/data/weather.json"
