use super::{Context, Error};

#[poise::command(prefix_command)]
pub async fn 등록(ctx: Context<'_>) -> Result<(), Error> {
    register_logic(ctx).await
}

#[poise::command(prefix_command)]
pub async fn register(ctx: Context<'_>) -> Result<(), Error> {
    register_logic(ctx).await
}

pub async fn register_logic(ctx: Context<'_>) -> Result<(), Error> {
    let register_button_id = format!("{}register_guild", ctx.id());
    let unregister_button_id = format!("{}unregister_guild", ctx.id());

    let reply = poise::CreateReply::default()
        .content("서버에 `SearchBot`의 기능들을 등록하시겠습니까?")
        .components(vec![poise::serenity_prelude::CreateActionRow::Buttons(
            vec![
                poise::serenity_prelude::CreateButton::new(&register_button_id)
                    .label("등록")
                    .style(poise::serenity_prelude::ButtonStyle::Primary),
                poise::serenity_prelude::CreateButton::new(&unregister_button_id)
                    .label("등록 해제")
                    .style(poise::serenity_prelude::ButtonStyle::Danger),
            ],
        )]);

    let sent_msg = ctx.send(reply).await?;

    while let Some(mci) = poise::serenity_prelude::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .channel_id(ctx.channel_id())
        .timeout(std::time::Duration::from_secs(300))
        .filter({
            let register_button_id = register_button_id.clone();
            let unregister_button_id = unregister_button_id.clone();
            move |mci| {
                mci.data.custom_id == register_button_id
                    || mci.data.custom_id == unregister_button_id
            }
        })
        .await
    {
        if let Some(guild_id) = ctx.guild_id() {
            mci.defer(ctx.http()).await?;

            let content = if mci.data.custom_id == register_button_id {
                poise::builtins::register_in_guild(
                    ctx,
                    &ctx.framework().options().commands,
                    guild_id,
                )
                .await?;
                "등록 되었습니다!"
            } else {
                poise::serenity_prelude::GuildId::set_commands(guild_id, ctx.http(), vec![])
                    .await?;
                "등록 해제 되었습니다!"
            };

            sent_msg
                .edit(
                    ctx,
                    poise::CreateReply::default()
                        .content(content)
                        .components(vec![]),
                )
                .await?;
            return Ok(());
        } else {
            mci.create_response(
                ctx,
                poise::serenity_prelude::CreateInteractionResponse::Message(
                    poise::serenity_prelude::CreateInteractionResponseMessage::new()
                        .content("유효하지 않은 서버입니다.")
                        .ephemeral(true),
                ),
            )
            .await?;
        }
    }

    Ok(())
}