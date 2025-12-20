use poise::serenity_prelude as serenity;
use std::vec;

mod command;
use command::{help::help, register::register, search::search, Data};
use crate::command::register::등록;

#[tokio::main]
async fn main() {
    println!("SearchBot start");

    let token = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN and no token argument provided")
    });
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![search(), 등록(), register(), help()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}
