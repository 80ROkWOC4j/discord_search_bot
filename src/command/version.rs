use crate::{Context, Error};
use poise::CreateReply;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TagsResponse {
    tags: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct TokenResponse {
    token: String,
}

const IMAGE_NAME: &str = "80rokwoc4j/discord_search_bot";

/// Show version information
#[poise::command(slash_command, prefix_command)]
pub async fn version(ctx: Context<'_>) -> Result<(), Error> {
    let current_version = env!("CARGO_PKG_VERSION");
    let mut response = format!("현재 버전: {}\n", current_version);

    // GHCR 최신 버전 확인 (비동기 시도)
    match check_latest_version().await {
        Ok(Some(latest_version)) => {
            response.push_str(&format!("최신 버전: {}\n", latest_version));
            if current_version != latest_version {
                response.push_str("\n업데이트 가능합니다!");
            } else {
                response.push_str("\n최신입니다!");
            }
        }
        Ok(None) => {
            response.push_str("GHCR에서 태그 검색 실패!");
        }
        Err(e) => {
            // 실패해도 현재 버전은 출력
            eprintln!("Failed to check GHCR: {}", e);
            response.push_str(&format!("\n(최신버전 확인 실패: {})", e));
        }
    }
    ctx.send(CreateReply::default().ephemeral(true).content(response))
        .await?;
    Ok(())
}

async fn check_latest_version() -> Result<Option<String>, Error> {
    let client = reqwest::Client::new();

    // 1. 인증 토큰 획득 (Public 이미지라도 토큰 필요)
    let token_url = format!(
        "https://ghcr.io/token?service=ghcr.io&scope=repository:{}:pull",
        IMAGE_NAME
    );
    let token_resp: TokenResponse = client.get(&token_url).send().await?.json().await?;

    // 2. 태그 목록 조회
    let tags_url = format!("https://ghcr.io/v2/{}/tags/list", IMAGE_NAME);
    let tags_resp: TagsResponse = client
        .get(&tags_url)
        .bearer_auth(token_resp.token)
        .send()
        .await?
        .json()
        .await?;

    // 태그 중 SemVer 형식(x.y.z) 또는 vX.Y.Z 형식을 찾아 가장 최신을 반환하는 로직이 필요하지만,
    // 여기서는 간단히 tags 리스트의 마지막을 가져오거나 'latest'를 제외한 최신을 찾습니다.
    // 실제로는 정렬이 보장되지 않을 수 있으므로 semver 파싱이 좋지만, 문자열 정렬로 대체합니다.

    let mut tags = tags_resp.tags;
    
    // 숫자와 점으로만 구성된 시맨틱 버전 형식(x.y.z)만 필터링
    tags.retain(|t| {
        if t == "latest" { return false; }
        
        // 너무 긴 태그(해시값 등) 제외
        if t.len() > 20 { return false; }

        let chars: Vec<char> = t.chars().collect();
        if chars.is_empty() { return false; }

        // 숫자로 시작해야 함 (vX.Y.Z 지원 안 함)
        let is_numeric_start = chars[0].is_numeric();
            
        // .을 포함해야 함 (최소한의 버전 구조)
        let has_dot = t.contains('.');

        // 모든 문자가 숫자이거나 점(.)이어야 함
        let is_clean_version = t.chars().all(|c| c.is_numeric() || c == '.');

        is_numeric_start && has_dot && is_clean_version
    });

    // 시맨틱 버전 정렬 (0.9.0 < 0.10.0)
    tags.sort_by(|a, b| {
        let parts_a: Vec<&str> = a.split('.').collect();
        let parts_b: Vec<&str> = b.split('.').collect();

        let len = std::cmp::max(parts_a.len(), parts_b.len());

        for i in 0..len {
            let num_a = parts_a.get(i).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let num_b = parts_b.get(i).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);

            match num_a.cmp(&num_b) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        std::cmp::Ordering::Equal
    });

    Ok(tags.last().cloned())
}
