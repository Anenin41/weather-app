# Packages
from __future__ import annotations
import json
import os
import subprocess
import time
from pathlib import Path
from typing import Any, Dict

import yaml
from flask import Flask, abort, jsonify, render_template, redirect, url_for # pyright: ignore[reportMissingImports]

app = Flask(__name__)

# ------------- config loading -------------
def load_config() -> tuple[dict, Path]:
    """
    Load YAML config. Search order:
      - $WEATHER_CONFIG (explicit)
      - ../config/web.yaml (repo default)
      - ./web.yaml (cwd fallback)
    Returns (cfg, cfg_path).
    """
    env_path = os.environ.get("WEATHER_CONFIG")
    if env_path:
        p = Path(env_path).expanduser().resolve()
        if not p.exists():
            raise SystemExit(f"WEATHER_CONFIG not found: {p}")
        with p.open("r", encoding="utf-8") as f:
            return yaml.safe_load(f) or {}, p

    here = Path(__file__).resolve()
    repo_root = here.parents[1]
    default = repo_root / "config" / "web.yaml"
    if default.exists():
        with default.open("r", encoding="utf-8") as f:
            return yaml.safe_load(f) or {}, default

    cwd_fallback = Path("web.yaml").resolve()
    if cwd_fallback.exists():
        with cwd_fallback.open("r", encoding="utf-8") as f:
            return yaml.safe_load(f) or {}, cwd_fallback

    raise SystemExit("No web config found. Put one at config/web.yaml or set WEATHER_CONFIG.")

CFG, CFG_PATH = load_config()

# Helper: resolve a possibly-relative path from the config file location
def resolve_from_cfg(path_str: str) -> Path:
    base = CFG_PATH.parent
    return (base / path_str).expanduser().resolve()

# ------------- caching -------------
_CACHE: Dict[str, Any] = {"ts": 0.0, "data": None}

def get_data(use_cache: bool = True) -> Dict[str, Any]:
    now = time.time()
    ttl = int(CFG.get("cache_ttl_seconds", 120))
    if use_cache and _CACHE["data"] and (now - _CACHE["ts"] < ttl):
        return _CACHE["data"]

    mode = str(CFG.get("mode", "file")).lower()
    if mode == "spawn":
        weather_bin = resolve_from_cfg(CFG["weather_bin"])
        rust_cfg = resolve_from_cfg(CFG["rust_config"])
        # call: weather-app --config <rust_cfg>  (prints JSON to stdout)
        proc = subprocess.run(
            [str(weather_bin), "--config", str(rust_cfg)],
            capture_output=True,
            text=True,
            timeout=90,
        )
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or "Rust fetcher failed")
        data = json.loads(proc.stdout)
    else:  # "file"
        json_path = resolve_from_cfg(CFG["json_path"])
        with json_path.open("r", encoding="utf-8") as f:
            data = json.load(f)

    # minimal validation: required keys
    if "current" not in data or "forecasts" not in data:
        raise RuntimeError("Invalid JSON: missing 'current' or 'forecasts'")
    _CACHE["data"] = data
    _CACHE["ts"] = now
    return data

# ------------- routes -------------
@app.get("/")
def index():
    try:
        data = get_data(use_cache=False)  # always read fresh data for main page
    except Exception as e:
        abort(500, f"Failed to load data: {e}")
    return render_template("index.html", **data)

# handy for debugging / consuming programmatically
@app.get("/api/data")
def api_data():
    try:
        data = get_data(use_cache=True)  # use cache for API endpoint
    except Exception as e:
        return jsonify({"error": str(e)}), 500
    return jsonify(data)

if __name__ == "__main__":
    host = str(CFG.get("server", {}).get("host", "127.0.0.1"))
    port = int(CFG.get("server", {}).get("port", 3000))
    debug = bool(CFG.get("server", {}).get("debug", False))
    app.run(host, port, debug=debug)
