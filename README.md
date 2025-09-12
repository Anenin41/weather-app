# Weather Dashboard

A simple weather dashboard that fetches current and forecast data for multiple cities using OpenWeather, powered by a Rust backend and a Flask web frontend.

## Features
- Fetches current weather and 5-day forecast for configured cities
- Rust backend for fast, concurrent data fetching
- Flask frontend for a polished dashboard
- Configurable via YAML files
- Manual or automated data refresh (Makefile, script, or cron)

## Setup

### 1. Build the Rust backend
```
cargo build --release
```
Or use:
```
make
```
to perform all of these steps at once.

### 2. Configure the app
Edit `config/web.yaml` and `config/rust.yaml` to set your cities, API keys, and file paths.

### 3. Generate initial weather data
```
bash scripts/refresh_weather.sh
```
Or use:
```
make refresh
```

### 4. Start the Flask web server
```
make run-web
```
Or manually:
```
cd web
python app.py
```

### 5. View the dashboard
Open your browser at `http://127.0.0.1:3000` (or your configured host/port).

## Refreshing Data
- Run `bash scripts/refresh_weather.sh` or `make refresh` to update the weather data.
- Reload the page in your browser to see the latest results.
- For automation, add a cron job:
  ```
  */10 * * * * /path/to/weather-app/scripts/refresh_weather.sh
  ```

## Configuration
- All paths and settings are in `config/web.yaml` and `config/rust.yaml`.
- The Flask app reads these configs at startup.

## Development
- See the `Makefile` for build and run commands.
- The Rust backend outputs JSON for the frontend to display.