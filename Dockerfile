FROM rust:1.66.0 as build_image

COPY . .
RUN cargo build --release


FROM debian:stable-20230612-slim

COPY --from=build_image ./target/release/discord_search_bot .

CMD [ "./discord_search_bot" ]
