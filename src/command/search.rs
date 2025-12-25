use crate::{
    Context, Error,
    database::{self, SearchResult},
};
use poise::CreateReply;
use poise::serenity_prelude::{
    self as serenity, CreateActionRow, CreateButton, CreateEmbed, CreateMessage, EditMessage,
    GetMessages, Message, MessageId,
};
use std::vec;

pub mod logic;

#[cfg(test)]
mod tests;

use logic::{substr, timestamp_to_readable};

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
        .unwrap_or("Direct Message".to_owned());
    let channel_name = channel_to_search.name(ctx).await?;

    ctx.send(
        CreateReply::default()
            .ephemeral(true)
            .content("검색 결과를 dm으로 보냅니다"),
    )
    .await?;

    let dm = match send_dm(
        ctx,
        &format!("Search [{text}] in {guild_name}::{channel_name}"),
    )
    .await
    {
        Ok(msg) => msg,
        Err(_) => {
            ctx.send(
                CreateReply::default()
                    .ephemeral(true)
                    .content("검색 결과를 DM으로 보낼 수 없습니다! 권한을 확인해주세요"),
            )
            .await?;
            return Ok(());
        }
    };

    // === Phase 1: DB Search (Only if caching is enabled) ===
    if let Some(guild_id) = ctx.guild_id()
        && caching_enabled
    {
        let mut offset = 0;
        const DB_PAGE_SIZE: u32 = 10;

        loop {
            // Check if we have results in DB
            let results = database::search_messages_like(
                pool,
                guild_id.get() as i64,
                channel_to_search.get() as i64,
                &text,
                DB_PAGE_SIZE,
                offset,
            )
            .await?;

            if results.is_empty() {
                // try api search
                break;
            }

            send_search_results(&ctx, &dm, &results).await?;

            offset += results.len() as u32;

            if !search_more(ctx).await? {
                return Ok(());
            };
        }
    }

    // === Phase 2: API Search ===
    let mut last_msg_id = if caching_enabled {
        // Try to find the earliest message in DB to avoid overlap
        database::get_earliest_message_id(pool, channel_to_search.get() as i64)
            .await?
            .map(|id| MessageId::new(id as u64))
            .unwrap_or(MessageId::new(ctx.id())) // If DB empty, start from now
        // todo : 최근 메세지 id 말고 range 기반으로 변경
    } else {
        MessageId::new(ctx.id())
    };

    loop {
        {
            // Typing RAII
            let _typing = dm.channel_id.start_typing(&ctx.serenity_context().http);
            let mut count = 0;

            loop {
                // Auto-fetch Loop
                // 여기서 api 검색한 결과는 guild_id가 비워져서 옴
                let messages = channel_to_search
                    .messages(
                        ctx,
                        GetMessages::new()
                            .limit(SEARCH_MESSAGE_LIMIT)
                            .before(last_msg_id),
                    )
                    .await?;

                if messages.is_empty() {
                    send_dm(ctx, "end of channel, no more chat to find!").await?;
                    return Ok(());
                }

                last_msg_id = messages.last().unwrap().id;

                let guild_id = ctx.guild_id().map(|id| id.get() as i64).unwrap_or(0);
                let results: Vec<SearchResult> = if caching_enabled {
                    // 1. DB 저장
                    if let Err(e) = database::insert_messages(pool, &messages, guild_id).await {
                        println!("Failed to cache messages: {e:?}");
                    }

                    // 2. 방금 저장한거에서 검색어로 쿼리
                    let max_id = messages.first().unwrap().id.get() as i64;
                    let min_id = messages.last().unwrap().id.get() as i64;

                    database::search_messages_range(
                        pool,
                        guild_id,
                        channel_to_search.get() as i64,
                        &text,
                        min_id,
                        max_id,
                    )
                    .await?
                } else {
                    // 캐싱 안쓰면 contains 체크
                    messages
                        .iter()
                        .filter(|msg| msg.content.contains(&text))
                        .map(|msg| SearchResult::from_message(msg, guild_id))
                        .collect()
                };

                // send result
                send_search_results(&ctx, &dm, &results).await?;

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

        if !search_more(ctx).await? {
            return Ok(());
        };
    }
}

async fn send_search_results(
    ctx: &Context<'_>,
    dm: &Message,
    results: &[SearchResult],
) -> Result<(), Error> {
    // max size of discord embed field is 1024 (max embed size is 6000)
    // 10 is heuristic (msg(max 50) + author + time + etc... * 10 < 6000)
    let chunks = results.chunks(10);
    for chunk in chunks {
        let mut msg_builder = CreateMessage::new();
        for msg in chunk {
            let timestamp =
                serenity::Timestamp::from_unix_timestamp(msg.created_at).unwrap_or_default();
            let title = format!(
                "{}\t{}",
                &msg.author_name,
                &timestamp_to_readable(timestamp)
            );
            let content = format!("[{}]({})\n", substr(&msg.content, 50), msg.link());
            msg_builder = msg_builder
                .add_embed(CreateEmbed::new().field(&title, &content, false))
                .reference_message(dm);
        }
        ctx.author().direct_message(ctx, msg_builder).await?;
    }
    Ok(())
}

async fn search_more(ctx: Context<'_>) -> Result<bool, Error> {
    fn search_more_button() -> CreateButton {
        CreateButton::new("search more")
            .label("search more")
            .style(serenity::ButtonStyle::Primary)
    }

    let mut button_msg = ctx
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
            Ok(true)
        }
        None => {
            button_msg
                .edit(
                    ctx,
                    EditMessage::new()
                        .content("Search session end")
                        .components(vec![]),
                )
                .await?;
            Ok(false)
        }
    }
}

async fn send_dm(ctx: Context<'_>, msg: &str) -> poise::serenity_prelude::Result<Message> {
    ctx.author()
        .direct_message(ctx, CreateMessage::new().content(msg))
        .await
}
