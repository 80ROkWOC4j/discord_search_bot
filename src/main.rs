use poise::serenity_prelude::{self as serenity, Activity};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
// User data, which is stored and accessible in all command invocations
struct Data {}

/// Displays your or another user's account creation date
#[poise::command(slash_command, prefix_command)]
async fn search(
    ctx: Context<'_>,
    #[description = "text to search"] text: String,
    #[description = "검색할 최근 채팅 갯수(기본 200)"] count: Option<u64>,
) -> Result<(), Error> {
    ctx.discord().set_activity(Activity::playing(&text)).await;
    let typing = serenity::Typing::start(ctx.discord().http.clone(), ctx.channel_id().0)?;
    
    let limit = match count {
        None => 200,
        Some(i) => i,
    };
    let _messages = ctx.channel_id().messages(&ctx.discord(), |retriever| retriever.limit(limit)).await?;
    let mut result = text.clone().to_owned() + " 검색 결과 \n";
    for history in _messages {
        if history.content.contains(&text) {
            result += &history.link();
            result += "\n";
        }
    }

    // let u = user.as_ref().unwrap_or_else(|| ctx.author());
    // let response = format!("{}'s account was created at {}", u.name, u.created_at());
    ctx.say(&result).await?;
    serenity::Typing::stop(typing);
    ctx.discord().set_activity(Activity::playing("/search")).await;
    Ok(())
}

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![search(), register()],
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(serenity::GatewayIntents::non_privileged())
        .user_data_setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(Data {}) }));

    framework.run().await.unwrap();
}
