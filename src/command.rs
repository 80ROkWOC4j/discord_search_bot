mod search;
mod help;
mod config;
mod version;

use crate::{Data, Error};

pub fn commands() -> Vec<poise::Command<Data, Error>> {
    vec![
        search::search(),
        help::help(),
        config::config(),
        version::version(),
    ]
}
