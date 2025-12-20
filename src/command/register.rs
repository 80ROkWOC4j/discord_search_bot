use super::{Context, Error};

#[poise::command(prefix_command)]
pub async fn register(ctx: Context<'_>) -> Result<(), Error> {
    let register_guild_id = format!("{}register_guild", ctx.id());
    let unregister_guild_id = format!("{}unregister_guild", ctx.id());

    let reply = poise::CreateReply::default()
        .content("Register or unregister commands?")
        .components(vec![poise::serenity_prelude::CreateActionRow::Buttons(
            vec![
                poise::serenity_prelude::CreateButton::new(&register_guild_id)
                    .label("Register in guild")
                    .style(poise::serenity_prelude::ButtonStyle::Primary),
                poise::serenity_prelude::CreateButton::new(&unregister_guild_id)
                    .label("Unregister in guild")
                    .style(poise::serenity_prelude::ButtonStyle::Danger),
            ],
        )]);

    let sent_msg = ctx.send(reply).await?;

    while let Some(mci) = poise::serenity_prelude::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .channel_id(ctx.channel_id())
        .timeout(std::time::Duration::from_secs(300))
        .filter({
            let register_guild_id = register_guild_id.clone();
            let unregister_guild_id = unregister_guild_id.clone();
            move |mci| {
                mci.data.custom_id == register_guild_id || mci.data.custom_id == unregister_guild_id
            }
        })
        .await
    {
        if let Some(guild_id) = ctx.guild_id() {
            mci.defer(ctx.http()).await?;

            let content = if mci.data.custom_id == register_guild_id {
                poise::builtins::register_in_guild(
                    ctx,
                    &ctx.framework().options().commands,
                    guild_id,
                )
                .await?;
                "Registered commands in guild!"
            } else {
                poise::serenity_prelude::GuildId::set_commands(guild_id, ctx.http(), vec![])
                    .await?;
                "Unregistered commands in guild!"
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
                        .content("Must be in a guild")
                        .ephemeral(true),
                ),
            )
            .await?;
        }
    }

    Ok(())
}
