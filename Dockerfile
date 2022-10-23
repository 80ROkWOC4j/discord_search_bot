FROM rust

COPY . .
RUN cargo build --release

CMD [ "./target/release/discord_search_bot" ]
