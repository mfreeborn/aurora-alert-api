use sqlx::{postgres::PgConnectOptions, ConnectOptions, PgConnection};

use crate::configuration::DatabaseSettings;

pub async fn get_db_conn(config: &DatabaseSettings) -> anyhow::Result<PgConnection> {
    Ok(PgConnectOptions::new()
        .host(&config.host)
        .username(&config.username)
        .password(&config.password)
        .port(config.port)
        .database(&config.database_name)
        .connect()
        .await?)
}
