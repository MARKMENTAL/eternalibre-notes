use anyhow::{anyhow, Context, Result};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_LOCATION_LENGTH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TempUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "c" | "celsius" => Some(TempUnit::Celsius),
            "f" | "fahrenheit" => Some(TempUnit::Fahrenheit),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TempUnit::Celsius => "c",
            TempUnit::Fahrenheit => "f",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TempUnit::Celsius => "°C",
            TempUnit::Fahrenheit => "°F",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherCurrent {
    pub desc: String,
    pub temp_c: String,
    pub temp_f: String,
    pub feels_c: String,
    pub feels_f: String,
    pub humidity: String,
}

impl WeatherCurrent {
    pub fn temp(&self, units: TempUnit) -> String {
        match units {
            TempUnit::Celsius => format!("{}°C", self.temp_c),
            TempUnit::Fahrenheit => format!("{}°F", self.temp_f),
        }
    }

    pub fn feels(&self, units: TempUnit) -> String {
        match units {
            TempUnit::Celsius => format!("{}°C", self.feels_c),
            TempUnit::Fahrenheit => format!("{}°F", self.feels_f),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherDay {
    pub date: String,
    pub desc: String,
    pub avg_c: String,
    pub avg_f: String,
    pub max_c: String,
    pub max_f: String,
    pub min_c: String,
    pub min_f: String,
}

impl WeatherDay {
    pub fn avg(&self, units: TempUnit) -> String {
        match units {
            TempUnit::Celsius => format!("{}°C", self.avg_c),
            TempUnit::Fahrenheit => format!("{}°F", self.avg_f),
        }
    }

    pub fn high(&self, units: TempUnit) -> String {
        match units {
            TempUnit::Celsius => format!("{}°C", self.max_c),
            TempUnit::Fahrenheit => format!("{}°F", self.max_f),
        }
    }

    pub fn low(&self, units: TempUnit) -> String {
        match units {
            TempUnit::Celsius => format!("{}°C", self.min_c),
            TempUnit::Fahrenheit => format!("{}°F", self.min_f),
        }
    }

    /// Human-friendly day label: `2026-10-12` becomes `Oct 12`.
    /// Falls back to the raw value when it is not a calendar date.
    pub fn short_date(&self) -> String {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d") {
            date.format("%b %d").to_string()
        } else {
            self.date.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherDisplay {
    pub location: Option<String>,
    pub units: TempUnit,
    pub current: Option<WeatherCurrent>,
    pub days: Vec<WeatherDay>,
}

impl WeatherDisplay {
    pub fn empty() -> Self {
        Self {
            location: None,
            units: TempUnit::Celsius,
            current: None,
            days: Vec::new(),
        }
    }
}

struct CachedWeather {
    location: String,
    current: Option<WeatherCurrent>,
    days: Vec<WeatherDay>,
    checked_at: Instant,
}

pub struct WeatherService {
    location_path: PathBuf,
    units_path: PathBuf,
    endpoint: String,
    client: reqwest::Client,
    cache: Mutex<Option<CachedWeather>>,
}

impl WeatherService {
    pub fn new(location_path: PathBuf, units_path: PathBuf) -> Self {
        Self::with_endpoint(location_path, units_path, "https://wttr.in")
    }

    fn with_endpoint(
        location_path: PathBuf,
        units_path: PathBuf,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            location_path,
            units_path,
            endpoint: endpoint.into(),
            client: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(concat!("EternaLibre-Notes/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client configuration is valid"),
            cache: Mutex::new(None),
        }
    }

    pub async fn display(&self) -> WeatherDisplay {
        let units = self.read_units();
        let location = match self.read_location() {
            Ok(Some(location)) => location,
            _ => {
                return WeatherDisplay {
                    location: None,
                    units,
                    current: None,
                    days: Vec::new(),
                }
            }
        };

        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            if cached.location == location && cached.checked_at.elapsed() < CACHE_TTL {
                return WeatherDisplay {
                    location: Some(location),
                    units,
                    current: cached.current.clone(),
                    days: cached.days.clone(),
                };
            }
        }

        match self.fetch(&location).await {
            Ok((current, days)) => {
                *cache = Some(CachedWeather {
                    location: location.clone(),
                    current: current.clone(),
                    days: days.clone(),
                    checked_at: Instant::now(),
                });
                WeatherDisplay {
                    location: Some(location),
                    units,
                    current,
                    days,
                }
            }
            Err(_) => {
                // Retain a same-location report through an outage and cache
                // the failed check too, so every page load does not wait for
                // another timeout while wttr.in is unavailable.
                let (current, days) = cache
                    .as_ref()
                    .filter(|cached| cached.location == location)
                    .map(|cached| (cached.current.clone(), cached.days.clone()))
                    .unwrap_or((None, Vec::new()));
                *cache = Some(CachedWeather {
                    location: location.clone(),
                    current: current.clone(),
                    days: days.clone(),
                    checked_at: Instant::now(),
                });
                WeatherDisplay {
                    location: Some(location),
                    units,
                    current,
                    days,
                }
            }
        }
    }

    pub async fn set_location(&self, value: &str) -> Result<()> {
        let location = value.trim();
        if location.chars().count() > MAX_LOCATION_LENGTH {
            return Err(anyhow!(
                "Location must be at most {MAX_LOCATION_LENGTH} characters."
            ));
        }
        if location.chars().any(char::is_control) {
            return Err(anyhow!("Location cannot contain control characters."));
        }

        if location.is_empty() {
            match tokio::fs::remove_file(&self.location_path).await {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).context("removing saved weather location"),
            }
        } else {
            if let Some(parent) = self.location_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("creating notes directory for weather setting")?;
            }
            tokio::fs::write(&self.location_path, location)
                .await
                .context("saving weather location")?;
        }

        *self.cache.lock().await = None;
        Ok(())
    }

    pub async fn set_units(&self, value: &str) -> Result<()> {
        let units = TempUnit::parse(value)
            .ok_or_else(|| anyhow!("Units must be Celsius or Fahrenheit."))?;
        if let Some(parent) = self.units_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("creating notes directory for weather setting")?;
        }
        tokio::fs::write(&self.units_path, units.as_str())
            .await
            .context("saving weather units")?;
        // No refetch needed: the cached report keeps both scales and the
        // display layer picks which one to render.
        Ok(())
    }

    pub fn read_units(&self) -> TempUnit {
        match std::fs::read_to_string(&self.units_path) {
            Ok(value) => TempUnit::parse(&value).unwrap_or_default(),
            Err(_) => TempUnit::default(),
        }
    }

    fn read_location(&self) -> Result<Option<String>> {
        let value = match std::fs::read_to_string(&self.location_path) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err).context("reading saved weather location"),
        };
        let location = value.trim();
        if location.is_empty() {
            Ok(None)
        } else if location.chars().count() > MAX_LOCATION_LENGTH
            || location.chars().any(char::is_control)
        {
            Err(anyhow!("saved weather location is invalid"))
        } else {
            Ok(Some(location.to_string()))
        }
    }

    async fn fetch(&self, location: &str) -> Result<(Option<WeatherCurrent>, Vec<WeatherDay>)> {
        let encoded = utf8_percent_encode(location, NON_ALPHANUMERIC);
        let url = format!(
            "{}/{encoded}?format=j1",
            self.endpoint.trim_end_matches('/')
        );
        let response = self.client.get(url).send().await?.error_for_status()?;
        let body = response.text().await?;
        let parsed: WttrResponse =
            serde_json::from_str(&body).context("parsing wttr.in response")?;
        Ok((parsed.current(), parsed.days()))
    }
}

#[derive(Debug, Deserialize)]
struct WttrResponse {
    #[serde(default)]
    current_condition: Vec<WttrCurrent>,
    #[serde(default)]
    weather: Vec<WttrDay>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct WttrCurrent {
    #[serde(default)]
    temp_C: String,
    #[serde(default)]
    temp_F: String,
    #[serde(default)]
    FeelsLikeC: String,
    #[serde(default)]
    FeelsLikeF: String,
    #[serde(default)]
    humidity: String,
    #[serde(default)]
    weatherDesc: Vec<WttrValue>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct WttrDay {
    #[serde(default)]
    date: String,
    #[serde(default)]
    avgtempC: String,
    #[serde(default)]
    avgtempF: String,
    #[serde(default)]
    maxtempC: String,
    #[serde(default)]
    maxtempF: String,
    #[serde(default)]
    mintempC: String,
    #[serde(default)]
    mintempF: String,
    #[serde(default)]
    hourly: Vec<WttrHourly>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct WttrHourly {
    #[serde(default)]
    weatherDesc: Vec<WttrValue>,
}

#[derive(Debug, Deserialize)]
struct WttrValue {
    #[serde(default)]
    value: String,
}

fn clean(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "—".to_string()
    } else if trimmed.chars().count() > max {
        trimmed.chars().take(max).collect()
    } else {
        trimmed.to_string()
    }
}

impl WttrResponse {
    fn current(&self) -> Option<WeatherCurrent> {
        let now = self.current_condition.first()?;
        Some(WeatherCurrent {
            desc: clean(
                now.weatherDesc
                    .first()
                    .map(|d| d.value.as_str())
                    .unwrap_or(""),
                80,
            ),
            temp_c: clean(&now.temp_C, 12),
            temp_f: clean(&now.temp_F, 12),
            feels_c: clean(&now.FeelsLikeC, 12),
            feels_f: clean(&now.FeelsLikeF, 12),
            humidity: clean(&now.humidity, 12),
        })
    }

    /// Every forecast date wttr.in returns, in order. The service currently
    /// sends three dates including today, so callers must render what is
    /// present rather than assuming a five-day horizon.
    fn days(&self) -> Vec<WeatherDay> {
        self.weather
            .iter()
            .map(|day| {
                let desc = day
                    .hourly
                    .iter()
                    .filter_map(|h| h.weatherDesc.first())
                    .map(|d| d.value.trim())
                    .find(|d| !d.is_empty())
                    .unwrap_or("");
                WeatherDay {
                    date: clean(&day.date, 10),
                    desc: clean(desc, 80),
                    avg_c: clean(&day.avgtempC, 12),
                    avg_f: clean(&day.avgtempF, 12),
                    max_c: clean(&day.maxtempC, 12),
                    max_f: clean(&day.maxtempF, 12),
                    min_c: clean(&day.mintempC, 12),
                    min_f: clean(&day.mintempF, 12),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn temp_path(tag: &str, ext: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "eternalibre-weather-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        p.set_extension(ext);
        p
    }

    fn service(tag: &str, endpoint: &str) -> (WeatherService, PathBuf, PathBuf) {
        let location = temp_path(tag, "location");
        let units = temp_path(tag, "units");
        let _ = std::fs::remove_file(&location);
        let _ = std::fs::remove_file(&units);
        (
            WeatherService::with_endpoint(location.clone(), units.clone(), endpoint),
            location,
            units,
        )
    }

    async fn mock_server(response: String) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let length = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..length]).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            request
        });
        (format!("http://{addr}"), task)
    }

    const J1: &str = r#"{
        "current_condition": [{
            "temp_C": "15", "temp_F": "59",
            "FeelsLikeC": "13", "FeelsLikeF": "55",
            "humidity": "51",
            "weatherDesc": [{"value": "Overcast"}]
        }],
        "weather": [
            {"date": "2026-10-11", "avgtempC": "12", "avgtempF": "54",
             "maxtempC": "15", "maxtempF": "59", "mintempC": "9", "mintempF": "48",
             "hourly": [{"weatherDesc": [{"value": "Overcast"}]}]},
            {"date": "2026-10-12", "avgtempC": "14", "avgtempF": "57",
             "maxtempC": "17", "maxtempF": "62", "mintempC": "13", "mintempF": "55",
             "hourly": [{"weatherDesc": [{"value": "Sunny"}]}]}
        ]
    }"#;

    #[tokio::test]
    async fn location_is_stored_and_blank_clears_it() {
        let (svc, location, _units) = service("location", "http://127.0.0.1:1");
        svc.set_location("  Reykjavík  ").await.unwrap();
        assert_eq!(std::fs::read_to_string(&location).unwrap(), "Reykjavík");
        assert_eq!(svc.display().await.location.as_deref(), Some("Reykjavík"));
        svc.set_location(" ").await.unwrap();
        assert!(!location.exists());
    }

    #[tokio::test]
    async fn location_rejects_control_characters_and_long_values() {
        let (svc, _, _) = service("invalid", "http://127.0.0.1:1");
        assert!(svc.set_location("London\nInjected").await.is_err());
        assert!(svc
            .set_location(&"x".repeat(MAX_LOCATION_LENGTH + 1))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn units_default_to_celsius_and_persist() {
        let (svc, _, units) = service("units", "http://127.0.0.1:1");
        assert_eq!(svc.read_units(), TempUnit::Celsius);
        svc.set_units("f").await.unwrap();
        assert_eq!(std::fs::read_to_string(&units).unwrap(), "f");
        assert_eq!(svc.read_units(), TempUnit::Fahrenheit);
        assert!(svc.set_units("kelvin").await.is_err());
    }

    #[tokio::test]
    async fn weather_fetch_is_cached_and_location_is_encoded() {
        let (endpoint, server) = mock_server(J1.to_string()).await;
        let (svc, location, _) = service("cache", &endpoint);
        svc.set_location("London, UK").await.unwrap();

        let first = svc.display().await;
        assert_eq!(first.location.as_deref(), Some("London, UK"));
        assert_eq!(first.units, TempUnit::Celsius);
        let current = first.current.as_ref().expect("current conditions");
        assert_eq!(current.temp(TempUnit::Celsius), "15°C");
        assert_eq!(current.temp(TempUnit::Fahrenheit), "59°F");
        assert_eq!(first.days.len(), 2);
        assert_eq!(first.days[0].short_date(), "Oct 11");
        assert_eq!(first.days[1].high(TempUnit::Celsius), "17°C");
        let request = server.await.unwrap();
        assert!(request.starts_with("GET /London%2C%20UK?format=j1 "));
        let second = svc.display().await;
        assert_eq!(second, first);
        let _ = std::fs::remove_file(location);
    }

    #[tokio::test]
    async fn switching_units_reuses_the_cached_report() {
        let (endpoint, server) = mock_server(J1.to_string()).await;
        let (svc, _, _) = service("units-cache", &endpoint);
        svc.set_location("London").await.unwrap();
        let celsius = svc.display().await;
        server.await.unwrap();
        svc.set_units("f").await.unwrap();
        let fahrenheit = svc.display().await;
        assert_eq!(fahrenheit.units, TempUnit::Fahrenheit);
        assert_eq!(fahrenheit.current, celsius.current);
        assert_eq!(fahrenheit.days, celsius.days);
    }
}
