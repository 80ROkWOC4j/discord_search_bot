use poise::serenity_prelude as serenity;

mod command;
mod database;
mod event;

use dashmap::DashMap;
use poise::serenity_prelude::ChannelId;
use sqlx::SqlitePool;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct Data {
    pub database: SqlitePool,
    pub live_ranges: DashMap<ChannelId, database::Range>,
}
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
async fn main() {
    // Initialize logging
    let file_appender = tracing_appender::rolling::daily("logs", "discord_bot.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "discord_search_bot=info,warn".into()),
        )
        .init();

    tracing::info!("SearchBot start");

    let token = std::env::args().nth(1).unwrap_or_else(|| {
        let key = if cfg!(debug_assertions) {
            "DISCORD_TOKEN_DEBUG"
        } else {
            "DISCORD_TOKEN"
        };
        std::env::var(key).expect("missing DISCORD_TOKEN and no token argument provided")
    });

    // Initialize database
    let database = database::init_db()
        .await
        .expect("Failed to initialize database");
    tracing::info!("Database initialized");

    // Add MESSAGE_CONTENT intent for caching
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: command::commands(),
            event_handler: |ctx, event, framework, data| {
                Box::pin(event::event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                if !cfg!(debug_assertions) {
                    tracing::info!("Production mode: Registering commands globally");
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                }
                Ok(Data {
                    database,
                    live_ranges: DashMap::new(),
                })
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}
