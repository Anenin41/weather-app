RUST_CFG := config/rust.yaml
WEB_CFG  := config/web.yaml

.PHONY: all build run-json run-web refresh

all: build refresh run-web

build:
	cargo build --release --manifest-path=./Cargo.toml

refresh:        ## generate data/weather.json
	./target/release/weather-app --config $(RUST_CFG) --out data/weather.json

run-web:        ## start Flask using web config
	python3 -m venv web/.venv && . web/.venv/bin/activate && \
	pip -q install -r web/requirements.txt && \
	WEATHER_CONFIG=$(WEB_CFG) python web/app.py

run-json:       ## print JSON to stdout (debug)
	./target/release/weather-app --config $(RUST_CFG)