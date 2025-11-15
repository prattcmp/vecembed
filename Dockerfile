FROM --platform=linux/arm64 rust:1.79 as builder

RUN apt-get update && apt install -y protobuf-compiler libprotobuf-dev bash curl libgomp1

WORKDIR /usr/src

RUN cargo new vecembed

WORKDIR /usr/src/vecembed

# We do this to cache dependencies
COPY Cargo.toml .
COPY Cargo.lock .

RUN cargo build --release

COPY . .
COPY .env .env

RUN touch src/main.rs

RUN cargo build --release

# Set the startup command
CMD ["./target/release/silatus_vecembed"]

