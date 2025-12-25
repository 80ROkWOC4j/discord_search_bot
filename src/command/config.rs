use crate::command::{Context, Error};
/// 서치봇 설정을 관리합니다.
#[poise::command(slash_command, subcommands("caching"), guild_only)]
pub async fn config(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 이 채널에서 메세지 캐싱을 활성화 합니다. 서버 관리 권한 필요.
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD")]
pub async fn caching(
    ctx: Context<'_>,
    #[description = "Enable or disable caching"] enable: bool,
) -> Result<(), Error> {
    let Some(guild_channel) = ctx.guild_channel().await else {
        ctx.say("서버 내에서만 활성화 할 수 있는 옵션입니다.")
            .await?;
        return Ok(());
    };
    
    let pool = &ctx.data().database;
    crate::database::set_channel_caching(pool, ctx.channel_id(), enable).await?;

    if !enable {
        crate::database::delete_channel_messages(pool, ctx.channel_id()).await?;
    }

    let status = if enable {
        "활성화"
    } else {
        "비활성화 (저장된 데이터 삭제됨)"
    };
    ctx.say(format!(
        "{}에 의해 채널 `<#{}>`에서 메세지 캐싱이 **{}** 되었습니다.",
        ctx.author().display_name(),
        guild_channel.name(),
        status
    ))
    .await?;

    Ok(())
}
