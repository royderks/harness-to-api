FROM rust:1.97.1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
RUN cargo build --locked --release --bin h2a

FROM node:24.22.2-bookworm-slim AS runtime
# pi-bob is a third-party Pi extension, not an IBM CLI.
RUN npm install --global --no-audit --no-fund @openai/codex@0.166.0 @earendil-works/pi-coding-agent@1.5.0 pi-bob@0.2.6 \
    && npm cache clean --force \
    && groupadd --gid 10001 h2a \
    && useradd --uid 10001 --gid h2a --create-home --shell /bin/bash h2a \
    && mkdir -p /var/lib/h2a \
    && chown h2a:h2a /var/lib/h2a
COPY --from=build /src/target/release/h2a /usr/local/bin/h2a
COPY deploy/pi-bob /usr/local/bin/pi-bob
ENV HOME=/home/h2a HARNESS_API_STATE_DIR=/var/lib/h2a
USER 10001:10001
WORKDIR /home/h2a
EXPOSE 8787
ENTRYPOINT ["h2a"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8787", "--provider", "codex"]
