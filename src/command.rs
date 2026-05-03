mod search;
mod help;
mod config;
mod version;
mod notify;

use crate::{Data, Error};

pub fn commands() -> Vec<poise::Command<Data, Error>> {
    vec![
        search::search(),
        help::help(),
        config::config(),
        version::version(),
        notify::notify_version(),
    ]
}

pub async fn check_latest_version() -> Result<Option<String>, Error> {
    version::check_latest_version().await
}
