use chrono::DateTime;
use poise::serenity_prelude::{
    self as serenity, CreateActionRow, CreateButton, CreateEmbed, CreateMessage, EditMessage,
    GetMessages,
};
use poise::CreateReply;
use std::vec;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

struct Data {}

fn search_more_button() -> CreateButton {
    CreateButton::new("search more")
        .label("search more")
        .style(serenity::ButtonStyle::Primary)
}

fn timestamp_to_readable(timestamp: serenity::Timestamp) -> String {
    let datetime = DateTime::from_timestamp(timestamp.unix_timestamp(), 0).unwrap_or_default();
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
    #[description = "search until find something. false by default and search latest 1000 messages."]
    search_until_find: Option<bool>,
) -> Result<(), Error> {
    const SEARCH_MESSAGE_LIMIT: u8 = 100; // discord api limit
    const SEARCH_COUNT: u64 = 10; // search 10 times, so search latest 1000 messages
    let search_until_find = search_until_find.unwrap_or(false);

    // TODO : send results to thread for multiple search
    let channel_to_search = ctx.channel_id();
    let guild_name = ctx
        .guild()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| "Direct Message".to_string());
    let channel_name = channel_to_search
        .name(ctx)
        .await
        .unwrap_or_else(|_| "Unknown Channel".to_string());

    let dm_reply_msg = format!("Search [{}] in {}::{}", &text, &guild_name, &channel_name);

    let dm = ctx
        .author()
        .direct_message(ctx, CreateMessage::new().content(&dm_reply_msg))
        .await?;

    let mut last_msg = ctx
        .send(
            CreateReply::default()
                .ephemeral(true)
                .content("I'm sending results to dm! test"),
        )
        .await?
        .into_message()
        .await?;

    loop {
        let first_msg = last_msg.clone();

        // Typing scope
        {
            let _typing = dm.channel_id.start_typing(&ctx.serenity_context().http);

            let mut count = 0;
            loop {
                let messages = channel_to_search
                    .messages(
                        ctx,
                        GetMessages::new()
                            .limit(SEARCH_MESSAGE_LIMIT)
                            .before(last_msg.id),
                    )
                    .await?;

                if messages.is_empty() {
                    ctx.author()
                        .direct_message(
                            ctx,
                            CreateMessage::new().content("end of channel, no more chat to find!"),
                        )
                        .await?;
                    return Ok(());
                }

                last_msg = messages.last().unwrap().clone(); // for next search

                // TODO : change search algorithm
                let results: Vec<serenity::Message> = messages
                    .into_iter()
                    .filter(|msg| msg.content.contains(&text))
                    .collect();

                // send result
                // max size of discord embed field is 1024 (max embed size is 6000)
                // 10 is heuristic (msg(max 50) + author + time + etc... * 10 < 6000)
                let chunks: Vec<&[serenity::Message]> = results.chunks(10).collect();
                for chunk in chunks {
                    let mut msg_builder = CreateMessage::new();
                    for msg in chunk {
                        let name = format!(
                            "{}\t{}",
                            &msg.author.name,
                            &timestamp_to_readable(msg.timestamp)
                        );
                        let value =
                            format!("[{}]({})\n", &substr(msg.content.clone(), 50), &msg.link());
                        msg_builder =
                            msg_builder.add_embed(CreateEmbed::new().field(&name, &value, false));
                    }
                    ctx.author().direct_message(ctx, msg_builder).await?;
                }

                // stop search when find something and _search_until_find flag is true
                if search_until_find {
                    if results.is_empty() {
                        continue;
                    } else {
                        break;
                    }
                }
                // or search until _count == SEARCH_COUNT
                count += 1;
                if count == SEARCH_COUNT {
                    break;
                }
            }
        } // _typing is dropped here automatically

        let footer = format!(
            "Search result from {} ~ {}",
            &timestamp_to_readable(last_msg.timestamp),
            &timestamp_to_readable(first_msg.timestamp)
        );
        let mut footer_message = ctx
            .author()
            .direct_message(ctx, CreateMessage::new().content(footer.clone()))
            .await?;

        // create search more button
        let button_msg = ctx
            .author()
            .direct_message(
                ctx,
                CreateMessage::new()
                    .components(vec![CreateActionRow::Buttons(vec![search_more_button()])]),
            )
            .await?;

        // wait user's button click for 60 sec
        match button_msg
            .await_component_interaction(ctx)
            .timeout(std::time::Duration::from_secs(60))
            .await
        {
            Some(_) => {
                button_msg.delete(ctx).await?;
            }
            None => {
                button_msg.delete(ctx).await?;
                footer_message
                    .edit(
                        ctx,
                        EditMessage::new().content(format!("{footer}\nSearch session end")),
                    )
                    .await?;
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
    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![search(), register()],
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
