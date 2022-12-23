Unlike Discord's default search, this bot search every messages that "contains" a keyword.  
한글 검색을 제대로 못하는 디스코드 기본 검색 기능과 달리 이 봇은 키워드를 포함한 모든 메세지를 검색합니다.  


Invite bot or run it your self.  
https://discord.com/oauth2/authorize?client_id=1032354931673407620&permissions=19327372288&scope=bot


# How to run
```
git clone https://github.com/80ROkWOC4j/discord_search_bot.git
cd discord_search_bot
docker build -t discord_search_bot .
docker run -d --restart=unless-stopped -e DISCORD_TOKEN="YOUR_TOKEN" --name dsb discord_search_bot
```

# How to setup
1. Invite bot. you need read msg, send msg, use slash command permission.
2. Type mention bot register.  
```@SearchBot register```
3. Bot gonna reply with buttons. click "Register in guild" button.


# Usage
```
/search text:text_to_search count:number_of_msg_to_scan
```
Bot gonna send result to dm.
