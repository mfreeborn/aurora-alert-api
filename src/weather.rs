use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use sqlx::Connection;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use tokio::task::JoinHandle;

use crate::{
    configuration::{ApplicationSettings, DatabaseSettings, Settings},
    db::get_db_conn,
};

#[derive(Debug, EnumIter)]
pub enum Location {
    FortWilliam = 2649169,
    SpeanBridge = 2637248,
}

#[derive(Debug, Deserialize)]
struct Clouds {
    all: i16,
}

#[derive(Debug, Deserialize)]
struct WeatherDetail {
    description: String,
}

#[derive(Debug, Deserialize)]
struct WeatherBody {
    // Cloud cover is reported as an integer percentage.
    clouds: Clouds,
    // There is always at least 1 primary weather field, plus optional extras.
    weather: Vec<WeatherDetail>,
    // The time the forecast was updated at as an epoch timestamp.
    dt: u32,
    // What OpenWeather calls the city_id is an unsigned integer which fits within a u32.
    id: u32,
}

pub struct Weather {
    pub location_id: u32,
    pub description: String,
    pub cloud_cover: i16,
    pub updated_at: DateTime<Utc>,
    pub last_polled: DateTime<Utc>,
}

impl std::convert::From<WeatherBody> for Weather {
    fn from(weather_body: WeatherBody) -> Self {
        Self {
            location_id: weather_body.id,
            // OpenWeather returns a weather field which contains 1 or more entries, with the
            // first entry being the primary weather conditions. There should always, therefore,
            // be a `.first()` entry.
            description: weather_body.weather.first().unwrap().description.clone(),
            cloud_cover: weather_body.clouds.all,
            updated_at: Utc.timestamp_opt(weather_body.dt.into(), 0).unwrap(),
            last_polled: Utc::now(),
        }
    }
}

async fn get_weather(location: Location, api_key: &str) -> anyhow::Result<Weather> {
    let url = format!(
        "https://api.openweathermap.org/data/2.5/weather?id={}&appid={}",
        location as i32, api_key
    );

    let client = reqwest::ClientBuilder::new()
        .use_rustls_tls()
        .build()
        .unwrap();

    let current_weather: Weather = client
        .get(url)
        .send()
        .await?
        .json::<WeatherBody>()
        .await?
        .into();

    Ok(current_weather)
}

async fn store_weather_data(weather: Weather, config: &DatabaseSettings) -> anyhow::Result<()> {
    let mut conn = get_db_conn(config).await?;
    let mut txn = conn.begin().await?;

    sqlx::query!(
        r#"
            UPDATE weather_data
            SET
              cloud_cover = $1,
              description = $2,
              updated_at = $3,
              last_polled = $4
            WHERE location_id = $5
        "#,
        weather.cloud_cover,
        weather.description,
        weather.updated_at,
        weather.last_polled,
        weather.location_id as i32
    )
    .execute(&mut txn)
    .await?;

    txn.commit().await?;

    Ok(())
}

async fn task(config: Settings) -> anyhow::Result<()> {
    loop {
        for location in Location::iter() {
            let weather = get_weather(location, &config.application.open_weather_api_key).await?;
            store_weather_data(weather, &config.database).await?;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
    Ok(())
}

pub fn run_task(config: Settings) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let handle = tokio::spawn(task(config));
    Ok(handle)
}
