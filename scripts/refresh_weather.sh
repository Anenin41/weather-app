#!/bin/bash
# Refresh weather data by running the Rust backend

RUST_BIN="$(dirname "$0")/../target/releaset/weather-app"
JSON_PATH="$(dirname "$0")/../data/weather_json"

"$RUST_BIN" --out "$JSON_PATH"
