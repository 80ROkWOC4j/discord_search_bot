FROM rust as build_image

COPY . .
RUN cargo build --release


FROM debian:buster-slim

COPY --from=build_image ./target/release/discord_search_bot .

CMD [ "./discord_search_bot" ]
