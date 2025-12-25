use crate::{database, Context, Error};
use poise::CreateReply;
use poise::serenity_prelude::{
    self as serenity, CreateActionRow, CreateButton, CreateEmbed, CreateMessage, EditMessage,
    GetMessages, MessageId,
};
use std::vec;

pub mod logic;

#[cfg(test)]
mod tests;

use logic::{substr, timestamp_to_readable};

fn search_more_button() -> CreateButton {
    CreateButton::new("search more")
        .label("search more")
        .style(serenity::ButtonStyle::Primary)
}
const SEARCH_MESSAGE_LIMIT: u8 = 100; // discord api limit
const SEARCH_COUNT: u64 = 10; // search 10 times, so search latest 1000 messages

/// 메세지를 검색합니다
#[poise::command(slash_command, prefix_command)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "검색할 단어"] text: String,
    #[description = "결과물을 찾을 때 까지 검색"] search_until_find: Option<bool>,
) -> Result<(), Error> {
    let pool = &ctx.data().database;
    let search_until_find = search_until_find.unwrap_or(false);

    let channel_to_search = ctx.channel_id();
    let caching_enabled = database::is_channel_caching_enabled(pool, channel_to_search)
        .await
        .unwrap_or(false);

    // Initial DM Setup
    let guild_name = ctx
        .guild()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| "Direct Message".to_string());
    let channel_name = channel_to_search
        .name(ctx)
        .await
        .unwrap_or_else(|_| "Unknown Channel".to_string());

    let dm_reply_msg = format!("Search [{}] in {}::{}", &text, &guild_name, &channel_name);

    let dm = match ctx
        .author()
        .direct_message(ctx, CreateMessage::new().content(&dm_reply_msg))
        .await
    {
        Ok(msg) => msg,
        Err(_) => {
            ctx.say("I cannot send you a DM. Please enable DMs from server members.")
                .await?;
            return Ok(());
        }
    };

    let mut status_msg = ctx
        .send(
            CreateReply::default()
                .ephemeral(true)
                .content("Searching... Check your DM."),
        )
        .await?
        .into_message()
        .await?;

    // === Phase 1: DB Search (Only if caching is enabled) ===
    if let Some(guild_id) = ctx.guild_id()
        && caching_enabled
    {
        let mut offset = 0;
        const DB_PAGE_SIZE: u32 = 10;

        loop {
            // Check if we have results in DB
            let results = database::search_messages(
                pool,
                guild_id.get() as i64,
                channel_to_search.get() as i64,
                &text,
                DB_PAGE_SIZE,
                offset,
            )
            .await?;

            if results.is_empty() {
                // DB exhausted, switch to API Phase
                status_msg
                    .edit(
                        ctx.serenity_context(),
                        EditMessage::new().content("Switching to API search (history backfill)..."),
                    )
                    .await?;
                break;
            }

            // Display DB Results
            let mut msg_builder = CreateMessage::new();
            for msg in &results {
                let timestamp =
                    serenity::Timestamp::from_unix_timestamp(msg.created_at).unwrap_or_default();
                let name = format!(
                    "{}\t{}",
                    &msg.author_name,
                    &timestamp_to_readable(timestamp)
                );
                let link = msg.link();
                let value = format!("[{}]({})\n", substr(&msg.content, 50), link);
                msg_builder = msg_builder
                    .add_embed(CreateEmbed::new().field(&name, &value, false))
                    .reference_message(&dm);
            }
            ctx.author().direct_message(ctx, msg_builder).await?;

            offset += results.len() as u32;

            // Ask for more (DB Interaction)
            let footer = format!(
                "Showing cached results {} ~ {}",
                offset - results.len() as u32 + 1,
                offset
            );
            let mut footer_message = ctx
                .author()
                .direct_message(ctx, CreateMessage::new().content(footer.clone()))
                .await?;
            let button_msg = ctx
                .author()
                .direct_message(
                    ctx,
                    CreateMessage::new()
                        .components(vec![CreateActionRow::Buttons(vec![search_more_button()])]),
                )
                .await?;

            match button_msg
                .await_component_interaction(ctx)
                .timeout(std::time::Duration::from_secs(60))
                .await
            {
                Some(_) => {
                    button_msg.delete(ctx).await?;
                    // Continue DB loop
                }
                None => {
                    button_msg.delete(ctx).await?;
                    footer_message
                        .edit(
                            ctx.serenity_context(),
                            EditMessage::new().content(format!("{}\nSearch session end", footer)),
                        )
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    // === Phase 2: API Search (Legacy Logic + Caching) ===
    // Determine start point
    let mut last_msg_id = if caching_enabled {
        // Try to find the earliest message in DB to avoid overlap
        database::get_earliest_message_id(pool, channel_to_search.get() as i64)
            .await?
            .map(|id| MessageId::new(id as u64))
            .unwrap_or(MessageId::new(ctx.id())) // If DB empty, start from now
    } else {
        MessageId::new(ctx.id()) // 
    };

    let first_msg_in_session = last_msg_id;
    loop {
        {
            // Typing RAII
            let _typing = dm.channel_id.start_typing(&ctx.serenity_context().http);
            let mut count = 0;

            loop {
                // Auto-fetch Loop
                let messages = channel_to_search
                    .messages(
                        ctx,
                        GetMessages::new()
                            .limit(SEARCH_MESSAGE_LIMIT)
                            .before(last_msg_id),
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

                last_msg_id = messages.last().unwrap().id;

                // CACHING: Save fetched messages
                if let Err(e) = database::insert_messages(pool, &messages).await
                    && caching_enabled
                {
                    println!("Failed to cache messages: {e:?}");
                }

                // Filter
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
                        let value = format!("[{}]({})\n", substr(&msg.content, 50), &msg.link());
                        msg_builder = msg_builder
                            .add_embed(CreateEmbed::new().field(&name, &value, false))
                            .reference_message(&dm);
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
        }

        let footer = format!(
            "Scanned from {} ~ {}",
            timestamp_to_readable(first_msg_in_session.created_at()),
            timestamp_to_readable(last_msg_id.created_at())
        );
        let mut footer_message = ctx
            .author()
            .direct_message(ctx, CreateMessage::new().content(footer.clone()))
            .await?;
        let button_msg = ctx
            .author()
            .direct_message(
                ctx,
                CreateMessage::new()
                    .components(vec![CreateActionRow::Buttons(vec![search_more_button()])]),
            )
            .await?;

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
