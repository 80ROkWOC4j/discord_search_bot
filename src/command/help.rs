use super::{Context, Error};

/// Show help information
#[poise::command(slash_command, prefix_command)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    let version = env!("CARGO_PKG_VERSION");
    
    let response = match ctx.locale() {
        Some("ko") => format!(
            "**Discord Search Bot v{}**\n\
            이 봇은 채널의 과거 메시지를 검색하는 기능을 제공합니다.\n\n\
            **명령어:**\n\
            `/search <text>` - 현재 채널에서 특정 텍스트를 검색하고 결과를 DM으로 전송합니다.\n\
            `/help` - 현재 이 도움말을 표시합니다.",
            version
        ),
        _ => format!(
            "**Discord Search Bot v{}**\n\
            This bot searches for past messages in the channel.\n\n\
            **Commands:**\n\
            `/search <text>` - Search for specific text in the current channel and send results via DM.\n\
            `/help` - Show this help message.",
            version
        ),
    };

    ctx.send(poise::CreateReply::default()
        .content(response)
        .ephemeral(true))
        .await?;
    Ok(())
}
