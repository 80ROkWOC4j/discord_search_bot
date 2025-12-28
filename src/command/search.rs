pub mod logic;
#[cfg(test)]
mod tests;

use crate::{
    Context, Error,
    database::{self, SearchResult},
};
use logic::{substr, timestamp_to_readable};
use poise::CreateReply;
use poise::serenity_prelude::{
    self as serenity, ChannelId, CreateActionRow, CreateButton, CreateEmbed, CreateMessage,
    EditMessage, GetMessages, GuildId, Message, MessageId,
};
use sqlx::SqlitePool;
use std::vec;

const SEARCH_MESSAGE_LIMIT: usize = 100; // discord api limit
const SEARCH_COUNT: usize = 10; // search 10 times, so search latest 1000 messages
const DB_PAGE_SIZE: u32 = 10; // 메세지 하나에 최대 10개 결과 표시

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

    let guild_name = ctx
        .guild()
        .map(|g| g.name.clone())
        .unwrap_or("Direct Message".to_owned());
    let guild_id = ctx.guild_id().map(|id| id.get() as i64).unwrap_or(0);
    let channel_name = channel_to_search.name(ctx).await?;

    // 사용자 요청에 대한 답장이 검색 시작 기준점
    let last_msg = ctx
        .send(
            CreateReply::default()
                .ephemeral(true)
                .content("검색 결과를 dm으로 보냅니다"),
        )
        .await?
        .into_message()
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

    if let Some(guild_id) = ctx.guild_id()
        && caching_enabled
    {
        cache_search(
            ctx,
            text,
            pool,
            search_until_find,
            channel_to_search,
            dm,
            guild_id,
        )
        .await
    } else {
        non_cache_search(
            ctx,
            text,
            search_until_find,
            channel_to_search,
            guild_id,
            last_msg,
            dm,
        )
        .await
    }
}

async fn cache_search(
    ctx: Context<'_>,
    text: String,
    pool: &SqlitePool,
    search_until_find: bool,
    channel_to_search: ChannelId,
    dm: Message,
    guild_id: GuildId,
) -> Result<(), Error> {
    let mut current_range = match ctx.data().live_ranges.get(&channel_to_search) {
        Some(r) => *r,
        None => {
            // 캐싱 킨 후 아무런 대화가 없어서 live range 갱신이 안된 경우
            // 봇이 꺼져있어 last sync range와 현재 사이 큰 공백이 있을 수 있음
            // 따라서 단순하게 현재 id로 처리
            let now_id = MessageId::new(ctx.id()).get() as i64;
            database::Range::new(now_id, now_id)
        }
    };

    let mut search_cursor = current_range.end;
    loop {
        while {
            let messages_from_db = database::search_messages_range(
                pool,
                guild_id.get() as i64,
                channel_to_search.get() as i64,
                &text,
                current_range.start,
                search_cursor,
                DB_PAGE_SIZE,
            )
            .await?;

            if !messages_from_db.is_empty() {
                send_search_results(&ctx, &dm, &messages_from_db).await?;
                search_cursor = messages_from_db.last().unwrap().message_id - 1;
            } else {
                // db에 있는 live range 다 긁어옴. 이제 api 호출로 db 채우고 live range 확장하고 루프 반복
                let before_id = MessageId::new(current_range.start as u64);

                let messages =
                    get_messages_from_discord_api(&ctx, channel_to_search, &dm, before_id).await?;

                if messages.is_empty() {
                    send_dm(ctx, "End of channel history.").await?;
                    return Ok(());
                }

                database::insert_messages(pool, &messages, guild_id.get() as i64).await?;

                let min_id = messages.last().unwrap().id.get() as i64;
                let max_id = messages.first().unwrap().id.get() as i64;

                let extended_range =
                    database::add_sync_range(pool, channel_to_search.get() as i64, min_id, max_id)
                        .await?;

                current_range = database::Range::new(extended_range.start, current_range.end);

                ctx.data()
                    .live_ranges
                    .insert(channel_to_search, current_range);
            }

            search_until_find && messages_from_db.is_empty()
        } {}

        if !search_more(ctx).await? {
            return Ok(());
        }
    }
}

async fn non_cache_search(
    ctx: Context<'_>,
    text: String,
    search_until_find: bool,
    channel_to_search: ChannelId,
    guild_id: i64,
    last_msg: Message,
    dm: Message,
) -> Result<(), Error> {
    loop {
        let mut last_msg_id = last_msg.id;
        while {
            let messages =
                get_messages_from_discord_api(&ctx, channel_to_search, &dm, last_msg_id).await?;

            if messages.is_empty() {
                send_dm(ctx, "end of channel, no more chat to find!").await?;
                return Ok(());
            }

            last_msg_id = messages.last().unwrap().id;

            let results = messages
                .iter()
                .filter(|msg| msg.content.contains(&text))
                .map(|msg| SearchResult::from_message(msg, guild_id))
                .collect::<Vec<_>>();

            // send result
            send_search_results(&ctx, &dm, &results).await?;

            search_until_find && results.is_empty()
        } {}

        if !search_more(ctx).await? {
            return Ok(());
        };
    }
}

async fn get_messages_from_discord_api(
    ctx: &Context<'_>,
    channel_to_search: ChannelId,
    dm: &Message,
    last_msg_id: MessageId,
) -> poise::serenity_prelude::Result<Vec<Message>> {
    // api 호출 느리니까 타이핑 인디케이터 ux
    let _typing = dm.channel_id.start_typing(&ctx.serenity_context().http);
    let mut oldest_message_id = last_msg_id;

    // 1000개 긁어옴
    let mut result = Vec::with_capacity(SEARCH_MESSAGE_LIMIT * SEARCH_COUNT);
    for _ in 0..SEARCH_COUNT {
        // 참고 : 여기서 api 검색한 결과는 guild_id가 비워져서 올 수 있음
        let maybe_search_result = channel_to_search
            .messages(
                ctx,
                GetMessages::new()
                    .limit(SEARCH_MESSAGE_LIMIT as u8)
                    .before(oldest_message_id),
            )
            .await;

        match maybe_search_result {
            Ok(search_result) if !search_result.is_empty() => {
                oldest_message_id = search_result.last().unwrap().id;
                result.extend(search_result.into_iter());
            }
            _ => break,
        }
    }

    Ok(result)
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
