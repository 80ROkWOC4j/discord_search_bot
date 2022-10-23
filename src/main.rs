use std::{vec};

use poise::{serenity_prelude::{self as serenity, Activity, CreateButton}};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

struct Data {}

fn search_more_button () -> CreateButton {
    let mut button = CreateButton::default();
    button.custom_id("search more");
    button.label("search more");
    button.style(serenity::ButtonStyle::Primary);
    button
}

#[poise::command(slash_command, prefix_command)]
async fn search(
    ctx: Context<'_>,
    #[description = "text to search"] text: String,
    #[description = "count of recent chat for search(default 200)"] count: Option<u64>,
    #[description = "max length of shown messages(default 200)"] shown_length: Option<u64>,
) -> Result<(), Error> {
    ctx.discord().set_activity(Activity::playing(&text)).await;
    let typing = serenity::Typing::start(ctx.discord().http.clone(), ctx.channel_id().0)?;
    
    let limit = match count {
        None => 200,
        Some(i) => i,
    };
    // max size of discord embed msg is 1024
    let shown_len = match shown_length {
        None => 200,
        Some(i) => i,
    };
    let messages = ctx.channel_id().messages(&ctx.discord(), |retriever| retriever.limit(limit)).await?;
    
    let result_msg = "Search Result of ".to_string() + &text;

    let reply_id = ctx.say(&result_msg).await?.into_message().await?.id;

    let thread = ctx.channel_id().create_public_thread(&ctx.discord().http, reply_id, |t| t.name(&result_msg)).await.unwrap();

    for msg in messages {
        if msg.content.contains(&text) {
            thread.send_message(&ctx.discord().http, |b| {
                let name = format!("{}\t{}", &msg.author.name, &msg.timestamp.date().to_string());
                let value:String;
                if msg.content.len() > shown_len.try_into().unwrap() {
                    value = format!("[{}]({})\n", &msg.content[..shown_len.try_into().unwrap()], &msg.link());
                } else {
                    value = format!("[{}]({})\n", &msg.content, &msg.link());
                }
                b.embed(|e| e.field(&name, &value, false))
            }).await?;
        }
    }

    thread.send_message(&ctx.discord().http, |b| {
        b.components(|c| {
            c.create_action_row(|row| 
                row.add_button(search_more_button())
            )
        })
    }).await?;

    // todo : make button works

    serenity::Typing::stop(typing);
    ctx.discord().set_activity(Activity::playing("/search")).await;
    Ok(())
}

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![search(), register()],
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(serenity::GatewayIntents::non_privileged())
        .user_data_setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(Data {}) }));

    framework.run().await.unwrap();
}
