use std::env;

use serenity::async_trait;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{CommandResult, StandardFramework};
use serenity::http::Typing;
use serenity::model::channel::Message;
use serenity::prelude::*;

#[group]
#[commands(search)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {}

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~")) // set the bot's prefix to "~"
        .group(&GENERAL_GROUP);

    // Login with a bot token from the environment
    let token = "MTAzMjM1NDkzMTY3MzQwNzYyMA.GWDGC9.9waK6J3lcJLqMKZur8g-G7o8MWV3F5TmUlOlqo"; // env::var("DISCORD_TOKEN").expect("token");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

#[command]
async fn search(ctx: &Context, msg: &Message) -> CommandResult {
    let keyword = &msg.content[7..].trim();
    if keyword.len() == 0 {
        msg.reply(ctx, "사용법 : ~search 검색할내용").await?;
        return Ok(());
    }
    
    use serenity::model::gateway::Activity;

    let search_start_msg = keyword.clone().to_owned() + " 검색";
    let activity = Activity::playing(search_start_msg);
    let channel_id = msg.channel_id;

    ctx.set_activity(activity).await;
    let typing = Typing::start(ctx.http.clone(), channel_id.0)?;

    let mut result = keyword.clone().to_owned() + " 검색 결과 \n";

    let _messages = channel_id
        .messages(&ctx, |retriever| retriever.before(msg.id).limit(200))
        .await?;

    
    for history in _messages {
        if history.content.contains(keyword) {
            result += &history.link();
            result += "\n";
        }
    }

    Typing::stop(typing);
    ctx.set_activity(Activity::playing("~search")).await;
    msg.reply(ctx, result).await?;

    Ok(())
}
