use poise::serenity_prelude as serenity;
use std::vec;

mod command;
mod database;
use command::{config::config, help::help, search::search, Data, Error};

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::GuildCreate { guild, is_new } => {
            let is_new = is_new.unwrap_or(false);
            if is_new || cfg!(debug_assertions) {
                println!(
                    "Registering commands in guild: {} (ID: {}) (new: {})",
                    guild.name, guild.id, is_new
                );
                poise::builtins::register_in_guild(
                    ctx,
                    &_framework.options().commands,
                    guild.id,
                )
                .await?;
            }
        }
        serenity::FullEvent::Message { new_message } => {
            if new_message.author.bot {
                return Ok(());
            }

            if database::is_channel_caching_enabled(&data.database, new_message.channel_id)
                .await
                .unwrap_or(false)
            {
                if let Err(e) = database::insert_message(&data.database, new_message).await {
                    println!("Failed to insert message: {:?}", e);
                }
            }
        }
        serenity::FullEvent::MessageUpdate { event, .. } => {
            if database::is_channel_caching_enabled(&data.database, event.channel_id)
                .await
                .unwrap_or(false)
            {
                if let Err(e) = database::update_message(&data.database, event).await {
                    println!("Failed to update message: {:?}", e);
                }
            }
        }
        serenity::FullEvent::MessageDelete {
            channel_id,
            deleted_message_id,
            ..
        } => {
            if database::is_channel_caching_enabled(&data.database, *channel_id)
                .await
                .unwrap_or(false)
            {
                if let Err(e) = database::delete_message(&data.database, *deleted_message_id).await
                {
                    println!("Failed to delete message: {:?}", e);
                }
            }
        }
        serenity::FullEvent::MessageDeleteBulk {
            channel_id,
            multiple_deleted_messages_ids,
            ..
        } => {
            if database::is_channel_caching_enabled(&data.database, *channel_id)
                .await
                .unwrap_or(false)
            {
                if let Err(e) =
                    database::delete_messages(&data.database, multiple_deleted_messages_ids).await
                {
                    println!("Failed to bulk delete messages: {:?}", e);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    println!("SearchBot start");

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
    println!("Database initialized");

    // Add MESSAGE_CONTENT intent for caching
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![search(), help(), config()],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                if !cfg!(debug_assertions) {
                    println!("Production mode: Registering commands globally");
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                }
                Ok(Data { database })
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}