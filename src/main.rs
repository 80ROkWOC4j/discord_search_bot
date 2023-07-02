use std::vec;

use chrono::{DateTime, NaiveDateTime, Utc};
use poise::serenity_prelude::{self as serenity, CreateButton, Message};

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
    let naive = NaiveDateTime::from_timestamp_opt(timestamp.timestamp(), 0).unwrap();
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
    #[description = "search until find something. false by default and search latest 1000 messages."] search_until_find: Option<bool>,
) -> Result<(), Error> {
    const SEARCH_MESSAGE_LIMIT: u64 = 100; // discord api limit
    const SEARCH_COUNT: u64 = 10;  // search 10 times, so search latest 1000 messages
    let _search_until_find = match search_until_find {
        Some(b) => b,
        None => false,
    };

    // TODO : send results to thread for multiple search
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

    let mut last_msg = ctx
        .send(|b| b.ephemeral(true).content("I'm sending results to dm!"))
        .await?
        .into_message()
        .await?;

    loop {
        let typing_indicator =
            serenity::Typing::start(ctx.discord().http.clone(), dm.channel_id.0)?;
        let first_msg = last_msg.clone();

        let mut _count = 0;
        loop {
            let messages = channel_to_search
                .messages(&ctx.discord(), |retriever| {
                    retriever.limit(SEARCH_MESSAGE_LIMIT).before(last_msg.id)
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

            last_msg = messages.last().unwrap().clone(); // for next search

            // TODO : change search algorithm
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

            // stop search when find something and _search_until_find flag is true
            if _search_until_find {
                if results.len() == 0 {
                    continue;
                } else {
                    break;
                }
            }
            // or search until _count == SEARCH_COUNT
            _count += 1;
            if _count == SEARCH_COUNT {
                break;
            }
        }
        serenity::Typing::stop(typing_indicator).unwrap();

        let _footer = format!(
            "Search result from {} ~ {}",
            &timestamp_to_readable(last_msg.timestamp),
            &timestamp_to_readable(first_msg.timestamp)
        );
        let mut footer_message = ctx.author()
            .direct_message(&ctx.discord(), |b| {
                b.content(_footer.clone())
            })
            .await?;

        // create search more button
        let button_msg = ctx
            .author()
            .direct_message(&ctx.discord().http, |b| {
                b.components(|c| c.create_action_row(|row| row.add_button(search_more_button())))
            })
            .await?;

        // wait user's button click for 60 sec
        match button_msg
            .await_component_interaction(&ctx.discord())
            .timeout(std::time::Duration::from_secs(60))
            .await
        {
            Some(_) => {
                button_msg.delete(&ctx.discord()).await?;
            }
            None => {
                button_msg.delete(&ctx.discord()).await?;
                footer_message.edit(&ctx.discord(), |m| {
                    m.content(_footer + "\nSearch session end")
                }).await?;
                return Ok(());
            }
        };
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
