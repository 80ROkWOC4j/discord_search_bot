how to run
```
git clone https://github.com/80ROkWOC4j/discord_search_bot.git
cd discord_search_bot
docker build -t discord_search_bot .
docker run -d --restart=unless-stopped -e DISCORD_TOKEN="YOUR TOKEN" --name dsb discord_search_bot
```

how to use
1. invite bot.
2. type mention bot register.  
```@SearchBot register```
3. bot gonna reply with buttons. click "Register in guild" button.

usage
```
/search text:text_to_search count:count_of_search_messages
```
