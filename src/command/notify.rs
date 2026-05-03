use crate::{Context, Error};
use poise::CreateReply;

/// 새 버전 DM 알림을 설정합니다.
#[poise::command(slash_command)]
pub(super) async fn notify_version(
    ctx: Context<'_>,
    #[description = "Enable or disable update notification DM"] enable: bool,
) -> Result<(), Error> {
    let pool = &ctx.data().database;
    crate::database::set_version_subscription(pool, ctx.author().id.get(), enable).await?;

    let message = if enable {
        "새 버전 감지 시 DM 알림을 활성화했습니다."
    } else {
        "새 버전 DM 알림을 비활성화했습니다."
    };
    ctx.send(CreateReply::default().ephemeral(true).content(message))
        .await?;
    Ok(())
}
