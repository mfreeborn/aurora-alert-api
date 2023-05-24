use chrono::Utc;
use derive_more::Display;
use lettre::{
    message::header, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use serde::Serialize;
use tera::{Context, Tera};
use tokio::task::JoinHandle;

use crate::{
    configuration::{EmailSettings, Settings},
    db::get_db_conn,
    templates::{self, Template},
};

pub type EmailTransport = AsyncSmtpTransport<Tokio1Executor>;

/// Coalesce all possible errors in this module into one type.
#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("Lettre error")]
    Lettre(#[from] lettre::error::Error),

    #[error("Smtp error")]
    Smtp(#[from] lettre::transport::smtp::Error),

    #[error("Address error")]
    Address(#[from] lettre::address::AddressError),

    #[error("Content type error")]
    ContentType(#[from] lettre::message::header::ContentTypeErr),
}

/// An enumeration of the four different aurora alert levels.
#[derive(Clone, Copy, Debug, Display, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    Green,
    Yellow,
    Amber,
    Red,
}
impl AlertLevel {
    fn from_f32(activity_level: f32) -> Self {
        match activity_level {
            _ if activity_level < 50. => Self::Green,
            _ if activity_level < 100. => Self::Yellow,
            _ if activity_level < 200. => Self::Amber,
            _ => Self::Red,
        }
    }
}

#[derive(Debug)]
pub struct AlertBuilder {
    template_engine: Tera,
    template: Template,
    to_address: String,
}

impl AlertBuilder {
    fn new(to_address: &str, template_engine: Tera) -> Self {
        let template = Template::Alert;
        Self {
            template_engine,
            template,
            to_address: to_address.to_string(),
        }
    }

    pub fn add_context(
        self,
        alert_level: &AlertLevel,
    ) -> Result<RenderedEmailBuilder, anyhow::Error> {
        let mut context = Context::new();
        context.insert("alert_level", alert_level);
        dbg!(&context);

        let body = self.template.render(&context, &self.template_engine)?;
        dbg!(&body);

        Ok(RenderedEmailBuilder {
            to_address: self.to_address,
            subject: format!("Aurora alert level is now {alert_level}"),
            body,
        })
    }
}

pub struct RenderedEmailBuilder {
    to_address: String,
    subject: String,
    body: String,
}

impl RenderedEmailBuilder {
    fn build(&self) -> Result<Message, EmailError> {
        let email = Message::builder()
            .header(header::ContentType::parse("text/html; charset=utf8")?)
            .from("Aurora Alert <aurora.alert.app@gmail.com>".parse()?)
            .to(self.to_address.parse()?)
            .subject(self.subject.clone())
            .body(self.body.clone())?;

        Ok(email)
    }

    pub fn build_email(self) -> Result<SendableEmail, EmailError> {
        let email = self.build()?;

        Ok(SendableEmail { email })
    }
}

#[derive(Debug)]
pub struct SendableEmail {
    email: Message,
}

/// Entry point for constructing an email which can be sent to the given
/// address.
#[derive(Clone, Debug)]
pub struct EmailClient {
    pub mailer: EmailTransport,
    pub template_engine: Tera,
}

impl EmailClient {
    pub fn new(config: &EmailSettings) -> Self {
        let creds = Credentials::new(config.username.to_string(), config.password.to_string());
        let mailer = EmailTransport::relay("smtp.gmail.com")
            .expect("failed to initialise gmail relay")
            .credentials(creds)
            .build();

        let template_engine =
            templates::init().expect("failed to initialise email template engine");

        Self {
            mailer,
            template_engine,
        }
    }

    /// Start constructing a new email for sending out an aurora alert.
    pub fn new_alert(&self, to_address: &str) -> AlertBuilder {
        let engine = self.template_engine.clone();
        let x = AlertBuilder::new(to_address, engine);
        dbg!(&x);
        x
    }

    /// Send an email asynchronously.
    pub async fn send(&self, message: SendableEmail) {
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            let recipient = message.email.envelope().to();
            match mailer.send(message.email.clone()).await {
                Ok(_smtp_response) => {
                    tracing::debug!("email sent to {:?} successfully", recipient)
                }
                Err(e) => {
                    tracing::error!("error sending email to user: {}", e)
                }
            }
        });
    }
}

async fn task(config: Settings) -> anyhow::Result<()> {
    let client = EmailClient::new(&config.email);
    loop {
        // decide if we should send out an update:
        //   1. Alert level must be red
        //   2. Last alert must be >3 hours ago

        let mut db_conn = get_db_conn(&config.database).await?;
        let activity_level = sqlx::query_scalar!(
            r#"
              SELECT geomagnetic_activity
              FROM current_activity
            "#
        )
        .fetch_one(&mut db_conn)
        .await?;

        let alert_level = AlertLevel::from_f32(activity_level);

        if alert_level == AlertLevel::Red || true {
            // Check time last email sent
            let last_email_sent = sqlx::query_scalar!(
                r#"
                SELECT MAX(sent_at) as "sent_at"
                FROM alerts
            "#
            )
            .fetch_one(&mut db_conn)
            .await?;
            dbg!(&last_email_sent);

            if last_email_sent.is_none()
                || (Utc::now() - last_email_sent.unwrap()) > chrono::Duration::hours(3)
            {
                dbg!("erm");
                // send email
                let email = client
                    .new_alert(&config.email.to_address)
                    .add_context(&AlertLevel::Red)?
                    .build_email()?;
                dbg!(&email);
                client.send(email).await;

                sqlx::query!(
                    r#"
                    INSERT INTO alerts(activity_level, sent_at)
                    VALUES ($1, $2)
                "#,
                    activity_level,
                    Utc::now()
                )
                .execute(&mut db_conn)
                .await?;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
    Ok(())
}

pub fn run_task(config: Settings) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let handle = tokio::spawn(task(config));
    Ok(handle)
}
