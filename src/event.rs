use crate::{database, Data, Error};
use poise::serenity_prelude::{Context, FullEvent};
use poise::FrameworkContext;

pub async fn event_handler(
    ctx: &Context,
    event: &FullEvent,
    framework: FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    register_command(ctx, event, &framework).await?;

    if let Err(e) = handle_cache_event(data, event).await {
        println!("Cache error: {e:?}");
    }

    Ok(())
}

async fn register_command(
    ctx: &Context,
    event: &FullEvent,
    framework: &FrameworkContext<'_, Data, Error>,
) -> Result<(), Error> {
    if let FullEvent::GuildCreate { guild, is_new } = event {
        let is_new = is_new.unwrap_or(false);
        if is_new || cfg!(debug_assertions) {
            println!(
                "Registering commands in guild: {} (ID: {}) (new: {})",
                guild.name, guild.id, is_new
            );
            poise::builtins::register_in_guild(ctx, &framework.options().commands, guild.id)
                .await?;
        }
    }

    Ok(())
}

async fn handle_cache_event(data: &Data, event: &FullEvent) -> Result<(), Error> {
    // 1. 캐싱 대상 이벤트인지 확인하고 Channel ID 추출
    let channel_id = match event {
        FullEvent::Message { new_message } => new_message.channel_id,
        FullEvent::MessageUpdate { event, .. } => event.channel_id,
        FullEvent::MessageDelete { channel_id, .. } => *channel_id,
        FullEvent::MessageDeleteBulk { channel_id, .. } => *channel_id,
        _ => return Ok(()), // 캐싱과 무관한 이벤트는 무시
    };

    // 2. 캐싱 활성화 여부 확인
    if !database::is_channel_caching_enabled(&data.database, channel_id)
        .await
        .unwrap_or(false)
    {
        return Ok(());
    }

    // 3. DB 작업 수행
    // 봇이 꺼져있던 시간 중에 발생한 것 중 새 메세지는 검색 로직에서 lazy하게 처리되나
    // 그 외 것들은 처리 못함
    match event {
        FullEvent::Message { new_message } => {
            if !new_message.author.bot {
                database::insert_message(&data.database, new_message).await?;

                // Update Range Logic (Session Continuity)
                let msg_id = new_message.id.get() as i64;
                let channel_id = new_message.channel_id;

                let mut range_to_update = database::Range::new(msg_id, msg_id);

                // Check in-memory session range
                if let Some(mut session_range) = data.live_ranges.get_mut(&channel_id) {
                    // live는 range 무조건 확장
                    let new_merged = database::Range::new(session_range.start, msg_id);
                    *session_range = new_merged;
                    range_to_update = new_merged;
                } else {
                    // New session for this channel
                    data.live_ranges.insert(channel_id, range_to_update);
                }

                // Sync to DB
                database::add_sync_range(
                    &data.database,
                    channel_id.get() as i64,
                    range_to_update.start,
                    range_to_update.end,
                )
                .await?;
            }
        }
        FullEvent::MessageUpdate { event, .. } => {
            database::update_message(&data.database, event).await?;
        }
        FullEvent::MessageDelete {
            deleted_message_id, ..
        } => {
            database::delete_message(&data.database, *deleted_message_id).await?;
        }
        FullEvent::MessageDeleteBulk {
            multiple_deleted_messages_ids,
            ..
        } => {
            database::delete_messages(&data.database, multiple_deleted_messages_ids).await?;
        }
        _ => {}
    }
    Ok(())
}
