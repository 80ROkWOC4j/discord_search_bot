use poise::serenity_prelude as serenity;
use std::vec;

mod command;
use command::{help::help, search::search, Data, Error};

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _data: &Data,
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
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![search(), help()],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                if !cfg!(debug_assertions) {
                    println!("Production mode: Registering commands globally");
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                }
                Ok(Data {})
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}
