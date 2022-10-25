invite bot or run it your self  
https://discord.com/oauth2/authorize?client_id=1032354931673407620&permissions=19327372288&scope=bot


how to run
```
git clone https://github.com/80ROkWOC4j/discord_search_bot.git
cd discord_search_bot
docker build -t discord_search_bot .
docker run -d --restart=unless-stopped -e DISCORD_TOKEN="YOUR TOKEN" --name dsb discord_search_bot
```

how to use
1. invite bot. you need read msg, send msg, use slash command permission.
2. type mention bot register.  
```@SearchBot register```
3. bot gonna reply with buttons. click "Register in guild" button.


usage
```
/search text:text_to_search count:number_of_msg_to_scan
```
then bot gonna send result to dm.