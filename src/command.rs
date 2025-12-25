pub mod search;
pub mod help;
pub mod config;

use sqlx::SqlitePool;

pub struct Data {
    pub database: SqlitePool,
} // User data, which is stored and accessible in all command invocations
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
