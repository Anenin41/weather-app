// Packages
use std::{collections::HashMap, path::PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// OpenWeather fetcher that outputs JSON based on a YAML config.
/// Works with the Python site in /web (either spawn mode or file mode).
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to YAML config. Search order if not given:
    /// $WEATHER_CONFIG, ./config/rust.yaml, ./config.yaml, ~/.config/weather-app/config.yaml
    #[arg(long)]
    config: Option<PathBuf>,

    /// Write JSON here (pretty). If omitted, JSON is printed to stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

/* ============================ Config ============================ */

#[derive(Deserialize, Debug)]
struct Config {
    openweather: OpenWeatherCfg,
    app: AppCfg,
}

#[derive(Deserialize, Debug)]
struct OpenWeatherCfg {
    api_key: String,
    #[serde(default = "default_units")]
    units: String, // "metric" | "imperial" | "standard"
    #[serde(default = "default_lang")]
    lang: String,  // e.g. "en"
}

#[derive(Deserialize, Debug)]
struct AppCfg {
    cities: Vec<String>,
}

fn default_units() -> String { "metric".into() }
fn default_lang() -> String { "en".into() }

fn load_config(explicit: Option<PathBuf>) -> Result<Config> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit { candidates.push(p); }
    if let Ok(p) = std::env::var("WEATHER_CONFIG") { candidates.push(PathBuf::from(p)); }
    candidates.push(PathBuf::from("./config/rust.yaml"));
    candidates.push(PathBuf::from("./config.yaml"));
    if let Some(mut d) = dirs::config_dir() {
        d.push("weather-app/config.yaml");
        candidates.push(d);
    }

    for path in candidates {
        if path.exists() {
            let s = std::fs::read_to_string(&path)
                .with_context(|| format!("reading config from {}", path.display()))?;
            let cfg: Config = serde_yaml::from_str(&s)
                .with_context(|| format!("parsing YAML in {}", path.display()))?;
            return Ok(cfg);
        }
    }
    bail!("No config file found. Use --config or provide one at ./config/rust.yaml");
}


/* ============================ Output JSON ============================ */

#[derive(Serialize)]
struct Output {
    generated_at_utc: String,
    current: Vec<CurrentOut>,
    forecasts: Vec<CityForecastOut>,
}

#[derive(Serialize)]
struct CurrentOut {
    city: String,
    time_local: String, // dd-mm-YYYY HH:MM
    utc_offset: String, // e.g., "UTC+2"
    temp_c: f64,
    humidity_pct: i64,
    condition: String,
}

#[derive(Serialize)]
struct CityForecastOut {
    city: String,
    days: Vec<ForecastDayOut>,
}

#[derive(Serialize)]
struct ForecastDayOut {
    date: String, // dd-mm-YYYY
    min_c: f64,
    max_c: f64,
    condition: String,
}

/* ============================ OpenWeather types ============================ */

#[derive(Deserialize, Debug)]
struct CurrentResp {
    dt: i64,
    timezone: i32, // seconds from UTC
    main: Main,
    weather: Vec<Weather>,
}

#[derive(Deserialize, Debug, Clone)]
struct Main {
    temp: f64,
    #[serde(default)]
    humidity: i64,
}

#[derive(Deserialize, Debug, Clone)]
struct Weather {
    description: String,
}

#[derive(Deserialize, Debug)]
struct ForecastResp {
    city: City,
    list: Vec<ForecastEntry>,
}

#[derive(Deserialize, Debug)]
struct City {
    timezone: i32,
}

#[derive(Deserialize, Debug)]
struct ForecastEntry {
    dt: i64,
    main: Main,
    weather: Vec<Weather>,
}

/* ============================ Main ============================ */

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let cfg = load_config(args.config)?;

    let key = cfg.openweather.api_key.trim().to_string();
    if key.is_empty() {
        return Err(anyhow!("Config openweather.api_key is empty"));
    }
    let units = cfg.openweather.units.to_lowercase();
    let lang  = cfg.openweather.lang.to_lowercase();

    let client = Client::builder().build()?;

    // ---- current (concurrent)
    let current = stream::iter(cfg.app.cities.clone())
        .map(|city| {
            let client = client.clone();
            let key = key.clone();
            let units = units.clone();
            let lang = lang.clone();
            let label = label_from_query(&city).to_string();
            async move {
                let cur = fetch_current(&client, &key, &city, &units, &lang).await?;
                Ok::<CurrentOut, anyhow::Error>(build_current_out(label, cur, &units))
            }
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    // ---- forecasts (concurrent)
    let forecasts = stream::iter(cfg.app.cities.clone())
        .map(|city| {
            let client = client.clone();
            let key = key.clone();
            let units = units.clone();
            let lang = lang.clone();
            let label = label_from_query(&city).to_string();
            async move {
                let days = fetch_and_summarize_forecast(&client, &key, &city, &units, &lang).await?;
                Ok::<CityForecastOut, anyhow::Error>(CityForecastOut { city: label, days })
            }
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    let out = Output {
        generated_at_utc: Utc::now().format("%d-%m-%Y %H:%M").to_string(),
        current,
        forecasts,
    };

    let json = serde_json::to_string_pretty(&out)?;
    if let Some(path) = args.out {
        std::fs::write(path, json)?;
    } else {
        println!("{json}");
    }

    Ok(())
}

/* ============================ HTTP + builders ============================ */

async fn fetch_current(client: &Client, key: &str, city: &str, units: &str, lang: &str) -> Result<CurrentResp> {
    let url = "https://api.openweathermap.org/data/2.5/weather";
    let resp = client
        .get(url)
        .query(&[("q", city), ("appid", key), ("units", units), ("lang", lang)])
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json::<CurrentResp>().await?)
}

async fn fetch_and_summarize_forecast(
    client: &Client,
    key: &str,
    city: &str,
    units: &str,
    lang: &str,
) -> Result<Vec<ForecastDayOut>> {
    let url = "https://api.openweathermap.org/data/2.5/forecast"; // 5d/3h
    let resp = client
        .get(url)
        .query(&[("q", city), ("appid", key), ("units", units), ("lang", lang)])
        .send()
        .await?
        .error_for_status()?;

    let fc = resp.json::<ForecastResp>().await?;
    let offset = FixedOffset::east_opt(fc.city.timezone)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());

    let mut by_day: HashMap<NaiveDate, Vec<(f64, String)>> = HashMap::new();

    for entry in fc.list {
        let dt_local = DateTime::<Utc>::from_timestamp(entry.dt, 0)
            .expect("valid UNIX ts")
            .with_timezone(&offset);
        let day_key: NaiveDate = dt_local.date_naive();
        let temp_c = to_celsius(entry.main.temp, units);
        let cond = entry
            .weather
            .get(0)
            .map(|w| title(&w.description))
            .unwrap_or_else(|| "Unknown".to_string());

        by_day.entry(day_key).or_default().push((temp_c, cond));
    }

    let mut days: Vec<(NaiveDate, Vec<(f64, String)>)> = by_day.into_iter().collect();
    days.sort_by_key(|(k, _)| *k);

    let mut out = Vec::new();
    for (day, samples) in days.into_iter().take(5) {
        let (min_t, max_t) = samples.iter().fold(
            (f64::INFINITY, f64::NEG_INFINITY),
            |(mn, mx), (t, _)| (mn.min(*t), mx.max(*t)),
        );

        let mut counts: HashMap<String, usize> = HashMap::new();
        for (_, c) in &samples {
            *counts.entry(c.clone()).or_insert(0) += 1;
        }
        let common = counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(c, _)| c)
            .unwrap_or_else(|| "Unknown".into());

        out.push(ForecastDayOut {
            date: day.format("%d-%m-%Y").to_string(),
            min_c: min_t,
            max_c: max_t,
            condition: common,
        });
    }

    Ok(out)
}

fn build_current_out(label: String, cur: CurrentResp, units: &str) -> CurrentOut {
    let offset = FixedOffset::east_opt(cur.timezone)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let when_local = DateTime::<Utc>::from_timestamp(cur.dt, 0)
        .expect("valid UNIX ts")
        .with_timezone(&offset);

    let cond = cur
        .weather
        .get(0)
        .map(|w| title(&w.description))
        .unwrap_or_else(|| "Unknown".to_string());

    CurrentOut {
        city: label,
        time_local: when_local.format("%d-%m-%Y %H:%M").to_string(),
        utc_offset: utc_offset_label(cur.timezone),
        temp_c: to_celsius(cur.main.temp, units),
        humidity_pct: cur.main.humidity,
        condition: cond,
    }
}

/* ============================ Utils ============================ */

fn label_from_query(city: &str) -> &str {
    city.split(',').next().unwrap_or(city).trim()
}

fn title(s: &str) -> String {
    let mut cs = s.chars();
    match cs.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
    }
}

fn utc_offset_label(secs: i32) -> String {
    let sign = if secs >= 0 { '+' } else { '-' };
    let abs = secs.abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;
    if mins == 0 { format!("UTC{sign}{hours}") } else { format!("UTC{sign}{hours}:{mins:02}") }
}

/// Normalize temperatures to Celsius based on the units from config.
fn to_celsius(value: f64, units: &str) -> f64 {
    match units {
        "metric" => value,                 // already °C
        "imperial" => (value - 32.0) * 5.0 / 9.0, // °F -> °C
        "standard" => value - 273.15,      // K -> °C
        _ => value, // unknown -> assume metric
    }
}
