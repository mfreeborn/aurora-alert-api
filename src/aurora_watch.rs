use serde::{Deserialize, Deserializer};
use sqlx::Connection;
use tokio::task::JoinHandle;

use crate::{
    configuration::{DatabaseSettings, Settings},
    db::get_db_conn,
};

const ACTIVITY_DATA_URL: &str =
    "https://aurorawatch-api.lancs.ac.uk/0.2/status/project/awn/sum-activity.xml";

async fn get_awn_data() -> anyhow::Result<CurrentActivity> {
    let client = reqwest::ClientBuilder::new()
        .use_rustls_tls()
        .build()
        .unwrap();
    let xml_response = client.get(ACTIVITY_DATA_URL).send().await?.text().await?;
    let polled_at = chrono::Utc::now();
    let activity = quick_xml::de::from_str::<SiteActivity>(&xml_response).unwrap();
    dbg!(&activity);
    let activity = CurrentActivity {
        polled_at,
        updated_at: activity.updated,
        value: activity.latest_activity() as f32,
    };

    Ok(activity)
}

#[derive(Debug)]
struct CurrentActivity {
    updated_at: chrono::DateTime<chrono::Utc>,
    polled_at: chrono::DateTime<chrono::Utc>,
    value: f32,
}

#[derive(Debug, Deserialize)]
struct SiteActivity {
    #[serde(rename = "@site_id")]
    site_id: String,
    #[serde(deserialize_with = "deserialize_updated")]
    updated: chrono::DateTime<chrono::Utc>,
    activity: [Activity; 24],
}

impl SiteActivity {
    fn latest_activity(&self) -> f64 {
        self.activity[23].value
    }
}

#[derive(Debug, Deserialize)]
struct Activity {
    datetime: chrono::DateTime<chrono::Utc>,
    value: f64,
}

fn deserialize_updated<'de, D>(deserializer: D) -> Result<chrono::DateTime<chrono::Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Updated {
        datetime: chrono::DateTime<chrono::Utc>,
    }
    Ok(Updated::deserialize(deserializer)?.datetime)
}

async fn store_awn_data(data: CurrentActivity, config: &DatabaseSettings) -> anyhow::Result<()> {
    let mut conn = get_db_conn(config).await?;
    let mut txn = conn.begin().await?;
    dbg!(&data);

    sqlx::query!(
        r#"
            UPDATE current_activity
            SET
              geomagnetic_activity = $1,
              updated_at = $2,
              last_polled = $3
            WHERE activity_id = 1
        "#,
        data.value,
        data.updated_at,
        data.polled_at
    )
    .execute(&mut txn)
    .await?;

    txn.commit().await?;

    Ok(())
}

async fn task(config: Settings) -> anyhow::Result<()> {
    loop {
        let data = get_awn_data().await?;
        store_awn_data(data, &config.database).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
    Ok(())
}

pub fn run_task(config: Settings) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let handle = tokio::spawn(task(config));
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    const ACT_DATA: &'static str = "<site_activity api_version=\"0.2.5\" project_id=\"project:AWN\" site_id=\"site:AWN:SUM\" site_url=\"http://aurorawatch-api.lancs.ac.uk/0.2.5/project/awn/sum.xml\"><lower_threshold status_id=\"green\">0</lower_threshold><lower_threshold status_id=\"yellow\">50</lower_threshold><lower_threshold status_id=\"amber\">100</lower_threshold><lower_threshold status_id=\"red\">200</lower_threshold><updated><datetime>2023-05-08T19:30:32+0000</datetime></updated><activity status_id=\"green\"><datetime>2023-05-07T20:00:00+0000</datetime><value>16.9</value></activity><activity status_id=\"green\"><datetime>2023-05-07T21:00:00+0000</datetime><value>30.2</value></activity><activity status_id=\"green\"><datetime>2023-05-07T22:00:00+0000</datetime><value>17.7</value></activity><activity status_id=\"green\"><datetime>2023-05-07T23:00:00+0000</datetime><value>20.3</value></activity><activity status_id=\"green\"><datetime>2023-05-08T00:00:00+0000</datetime><value>35.5</value></activity><activity status_id=\"yellow\"><datetime>2023-05-08T01:00:00+0000</datetime><value>98.7</value></activity><activity status_id=\"yellow\"><datetime>2023-05-08T02:00:00+0000</datetime><value>63.6</value></activity><activity status_id=\"green\"><datetime>2023-05-08T03:00:00+0000</datetime><value>18.3</value></activity><activity status_id=\"green\"><datetime>2023-05-08T04:00:00+0000</datetime><value>6.2</value></activity><activity status_id=\"green\"><datetime>2023-05-08T05:00:00+0000</datetime><value>5.5</value></activity><activity status_id=\"green\"><datetime>2023-05-08T06:00:00+0000</datetime><value>10.4</value></activity><activity status_id=\"green\"><datetime>2023-05-08T07:00:00+0000</datetime><value>13.9</value></activity><activity status_id=\"green\"><datetime>2023-05-08T08:00:00+0000</datetime><value>15.5</value></activity><activity status_id=\"green\"><datetime>2023-05-08T09:00:00+0000</datetime><value>18.6</value></activity><activity status_id=\"green\"><datetime>2023-05-08T10:00:00+0000</datetime><value>13.8</value></activity><activity status_id=\"green\"><datetime>2023-05-08T11:00:00+0000</datetime><value>20.5</value></activity><activity status_id=\"green\"><datetime>2023-05-08T12:00:00+0000</datetime><value>14.0</value></activity><activity status_id=\"green\"><datetime>2023-05-08T13:00:00+0000</datetime><value>27.2</value></activity><activity status_id=\"yellow\"><datetime>2023-05-08T14:00:00+0000</datetime><value>71.3</value></activity><activity status_id=\"yellow\"><datetime>2023-05-08T15:00:00+0000</datetime><value>68.2</value></activity><activity status_id=\"green\"><datetime>2023-05-08T16:00:00+0000</datetime><value>35.4</value></activity><activity status_id=\"yellow\"><datetime>2023-05-08T17:00:00+0000</datetime><value>65.3</value></activity><activity status_id=\"yellow\"><datetime>2023-05-08T18:00:00+0000</datetime><value>58.3</value></activity><activity status_id=\"green\"><datetime>2023-05-08T19:00:00+0000</datetime><value>13.0</value></activity></site_activity>";

    #[test]
    fn test_parse_xml() {
        let activity = quick_xml::de::from_str::<SiteActivity>(&ACT_DATA).unwrap();
        assert_eq!(
            activity.updated,
            Utc.with_ymd_and_hms(2023, 5, 8, 19, 30, 32).unwrap()
        );
        assert_eq!(activity.activity[23].value, 13.0);
    }
}
