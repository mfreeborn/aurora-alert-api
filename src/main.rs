use aurora_alert_api::{
    aurora_watch, configuration::get_configuration, email, telemetry::init_tracing, weather,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = get_configuration()?;
    init_tracing("debug");

    // Task 1 is to poll the Aurora Watch API and store the current level of
    // geomagnetic activty
    let t1 = aurora_watch::run_task(config.clone())?;

    // Task 2 is to poll the Open Weather API and store the current cloud cover
    // percentage
    let t2 = weather::run_task(config.clone())?;

    // Task 3 is to read the current activity level and conditionally email users
    let t3 = email::run_task(config.clone());

    t1.await??;
    Ok(())
}
