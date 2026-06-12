set dotenv-load := true

setup:
    cargo run -- setup

dev:
    docker compose up --build

serve:
    cargo run -- serve

worker:
    cargo run -- worker

check:
    bash scripts/check
