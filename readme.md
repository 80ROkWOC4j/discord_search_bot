한글 검색을 제대로 못하는 디스코드 기본 검색 기능과 달리 이 봇은 키워드를 포함한 모든 메세지를 검색합니다.

# 실행 방법

## 봇 초대
기본적으로 작동하고 있는 봇을 초대합니다.  
https://discord.com/oauth2/authorize?client_id=1032354931673407620&permissions=19327372288&scope=bot  
이 봇은 아무런 데이터도 수집하지 않지만, **프라이버시**(와 저의 서버비)**를 위해 직접 봇을 운영하는 것을 권장합니다.**

## 직접 실행
1. https://discord.com/developers/applications 에서 새 애플리케이션을 만듭니다.  
2. OAuth2 메뉴에서 Scopes는 `bot`, 봇 권한은 `Send Messages`, `Read Message History`, `Use Slash Commands` 체크해서 초대 링크를 만들어 봇을 서버에 초대합니다.  
3. Bot 메뉴에서 이후에 사용할 토큰을 발급받습니다.

### Docker(권장)
도커를 통해 직접 봇 서버를 띄우고 사용합니다.
1. Clone
    ```shell
    $ git clone https://github.com/80ROkWOC4j/discord_search_bot.git
    $ cd discord_search_bot
    ```
2. `docker-compose.yml` 안의 `DISCORD_TOKEN`에 본인의 토큰을 넣습니다. 환경 변수를 통해 토큰이 주입될 것입니다.
3. 실행
    ```shell
    $ docker compose up -d --build
    ```

### 바이너리 실행
항상 봇을 온라인 상태로 유지할 수 없고 검색할 때만 사용할 것이라면 이 방법을 권장합니다.  
직접 빌드하거나, 최신 릴리즈에서 바이너리를 다운받아서 실행합니다.  
환경 변수에 `DISCORD_TOKEN`을 등록하거나, 혹은 첫번째 인자로 디스코드 토큰을 입력합니다.
```shell
$ discord_search_bot <YourToken>
```

### 디버깅
디버깅 빌드에서는 `DISCORD_TOKEN_DEBUG`를 사용합니다. 개발 시 자세한 내용은 코드 참고.

# 사용 전 설정
1. 준비된 봇을 서버로 초대합니다.
2. `/help`를 입력해서 봇이 정상적으로 작동하는지 확인하세요. 봇이 서버에 들어온 후 활성화 되는데 몇초 지연이 있을 수도 있습니다.

# 사용법

## search
명령어를 입력한 채널에서 특정 텍스트를 찾아 그 결과를 dm으로 보냅니다.
```
/search text:text_to_search search_until_find:True or False
```
* text : 검색할 텍스트
* search_until_find : 찾는 검색 결과가 나올 때 까지 과거 채팅 기록을 찾음

## help
```
/help
```
버전 정보와 명령어 설명을 출력합니다. 

## config
### caching
```
/config caching True
```
활성화 할 경우 대화 내용을 기록해 검색 속도를 빠르게 합니다.  
**메세지가 평문으로 저장되니 직접 실행할 경우에만 사용하세요.**
