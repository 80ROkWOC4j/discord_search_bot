use poise::serenity_prelude as serenity;

mod command;
mod database;
mod event;

use dashmap::DashMap;
use poise::serenity_prelude::ChannelId;
use sqlx::SqlitePool;
use std::time::Duration;
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
                let pool = database.clone();
                let http = ctx.http.clone();
                tokio::spawn(async move {
                    let interval_secs = std::env::var("VERSION_CHECK_INTERVAL_SECS")
                        .ok()
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(86_400);
                    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
                    loop {
                        ticker.tick().await;
                        let latest = match crate::command::check_latest_version().await {
                            Ok(Some(v)) => v,
                            Ok(None) => continue,
                            Err(e) => {
                                tracing::warn!("version polling failed: {}", e);
                                continue;
                            }
                        };
                        let subscribers = match crate::database::list_version_subscribers(&pool).await {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!("list subscribers failed: {}", e);
                                continue;
                            }
                        };
                        for user_id in subscribers {
                            let uid = user_id as u64;
                            let should_notify = match crate::database::should_notify_version(&pool, uid, &latest).await {
                                Ok(v) => v,
                                Err(e) => {
                                    tracing::warn!("should_notify_version failed for {}: {}", uid, e);
                                    false
                                }
                            };
                            if !should_notify {
                                continue;
                            }
                            let user = match poise::serenity_prelude::UserId::new(uid).to_user(&http).await {
                                Ok(u) => u,
                                Err(_) => continue,
                            };
                            let dm = user.dm(
                                &http,
                                poise::serenity_prelude::CreateMessage::new().content(format!(
                                    "새 버전 `{}` 이 감지되었습니다. `/version`으로 확인해보세요.",
                                    latest
                                )),
                            ).await;
                            if dm.is_ok() {
                                let _ = crate::database::mark_version_notified(&pool, uid, &latest).await;
                            }
                        }
                    }
                });
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
