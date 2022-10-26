use std::vec;

use chrono::{DateTime, NaiveDateTime, Utc};
use poise::serenity_prelude::{self as serenity, CreateButton, InteractionResponseType, Message};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

struct Data {}

fn search_more_button() -> CreateButton {
    let mut button = CreateButton::default();
    button.custom_id("search more");
    button.label("search more");
    button.style(serenity::ButtonStyle::Primary);
    button
}

fn timestamp_to_readable(timestamp: serenity::model::Timestamp) -> String {
    let naive = NaiveDateTime::from_timestamp(timestamp.timestamp(), 0);
    let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn substr(content: String, n: usize) -> String {
    if content.chars().count() <= n {
        return content;
    }

    let tmp: Vec<char> = content.chars().take(n).collect();
    let s: String = tmp.iter().collect();
    s
}

#[poise::command(slash_command, prefix_command)]
async fn search(
    ctx: Context<'_>,
    #[description = "text to search"] text: String,
    #[description = "number of messages to scan(default : 100)"] count: Option<u64>,
) -> Result<(), Error> {
    let _count = match count {
        Some(i) => {
            if i > 100 {
                i / 100
            } else {
                1
            }
        }
        None => 1,
    };

    let channel_to_search = ctx.channel_id();
    let dm_reply_msg = format!(
        "Search [{}] in {}::{}",
        &text,
        &ctx.guild().unwrap().name,
        &channel_to_search.name(&ctx.discord()).await.unwrap()
    );
    let dm = ctx
        .author()
        .direct_message(&ctx.discord(), |m| m.content(&dm_reply_msg))
        .await?;

    let reply = ctx
        .send(|b| b.ephemeral(true).content("I'm sending results to dm!"))
        .await?;

    let mut search_from = reply.into_message().await?.id;

    loop {
        let typing = serenity::Typing::start(ctx.discord().http.clone(), dm.channel_id.0)?;
        for _ in 0.._count {
            let messages = channel_to_search
                .messages(&ctx.discord(), |retriever| {
                    retriever.limit(100).before(search_from)
                })
                .await?;

            if messages.len() == 0 {
                ctx.author()
                    .direct_message(&ctx.discord(), |m| {
                        m.content("end of channel, no more chat to find!")
                    })
                    .await?;
                return Ok(());
            }

            search_from = messages.last().unwrap().id; // for next search

            let results: Vec<Message> = messages
                .into_iter()
                .filter(|msg| msg.content.contains(&text))
                .collect();

            // send result
            // max size of discord embed field is 1024 (max embed size is 6000)
            // 10 is heuristic (msg(max 50) + author + time + etc... * 10 < 6000)
            let chunks: Vec<&[Message]> = results.chunks(10).collect();
            for chunk in chunks {
                ctx.author()
                    .direct_message(&ctx.discord().http, |b| {
                        b.content("_ _");
                        for msg in chunk {
                            let name = format!(
                                "{}\t{}",
                                &msg.author.name,
                                &timestamp_to_readable(msg.timestamp)
                            );
                            let value = format!(
                                "[{}]({})\n",
                                &substr(msg.content.clone(), 50),
                                &msg.link()
                            );
                            b.add_embed(|e| e.field(&name, &value, false));
                        }
                        b
                    })
                    .await?;
            }
        }
        serenity::Typing::stop(typing).unwrap();

        // create search more button
        let button_msg = ctx
            .author()
            .direct_message(&ctx.discord().http, |b| {
                b.components(|c| c.create_action_row(|row| row.add_button(search_more_button())))
            })
            .await?;

        // wait user's button click for 60 sec
        let interaction = match button_msg
            .await_component_interaction(&ctx.discord())
            .timeout(std::time::Duration::from_secs(60))
            .await
        {
            Some(x) => x,
            None => {
                button_msg
                    .reply(&ctx.discord(), dm_reply_msg + " : Search session end")
                    .await?;
                return Ok(());
            }
        };

        // todo : disable button after click
        interaction
            .create_interaction_response(&ctx.discord(), |r| {
                r.kind(InteractionResponseType::UpdateMessage)
                    .interaction_response_data(|d| d)
            })
            .await
            .unwrap();
    }
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
