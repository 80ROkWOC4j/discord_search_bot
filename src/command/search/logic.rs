use chrono::DateTime;
use poise::serenity_prelude as serenity;

pub fn timestamp_to_readable(timestamp: serenity::Timestamp) -> String {
    let datetime = DateTime::from_timestamp(timestamp.unix_timestamp(), 0).unwrap_or_default();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn substr(content: &str, n: usize) -> &str {
    match content.char_indices().nth(n) {
        Some((idx, _)) => &content[..idx],
        None => content,
    }
}
