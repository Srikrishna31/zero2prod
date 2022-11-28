# A Dockerfile is a *recipe* for your application environment.
# They are organized in layers: you start from a base *image*(usually an OS enriched with a programming language toolchain)
# and execute a series of commands (COPY, RUN, etc), one after the other, to build the environment you need.
# We use the latest Rust stable release as the base image
# Builder stage
# This plays nicely with *multi-stage* builds, a useful Docker feature. We can split our build stage in two stages:
# * a builder stage, to generate a compiled binary;
# * a runtime stage, to run the binary.
# The builder stage doesnot contribute to its size - it is an intermediate step and it is discarded as the end of the build.
# The only piece of the builder stage that is found in the final artifact is what we explicitly copy over - the compiled
# binary
FROM rust:1.65.0 AS builder

# Let's switch our working directory to `app` (equivalent to `cd app`)
# The `app` folder will be created for us by Docker in case it does not exist already.
WORKDIR /app

# Install the required system dependencies for our linking configuration
RUN apt update && apt install lld clang -y

# Copy all files from our working environment to our Docker image
# Build Context
# docker build generates an image starting from a recipe (the Dockerfile) and a *build context*.
# You can picture the Docker image you are building as its own fully isolated environment.
# The only point of contact between the image and your local machine are commands like COPY or ADD: the build context
# determines what files on your host machine are visible inside the Docker container to COPY and its friends.
# Using . we are telling Docker to use the current directory as the buld context for this image; COPY . app will therefore
# copy all files from the current directory (including our source code!) into the app directory of our Docker image.
# Using . as build context implies, for example, that Docker will not allow COPY to see files from the parent directory
# or from arbitrary paths on your machine into the image.
# You could use a different path or even a URL(!) as build context depending on your needs.
COPY . .

ENV SQLX_OFFLINE true

# Let's build our binary!
# We'll use the release profile to make it faaaast
RUN cargo build --release

# Runtime stage
# Use the bare operating system as base image for our runtime stage:
FROM debian:bullseye-slim AS runtime

WORKDIR /app

# Install OpenSSL - it is dynamically linked by some of our dependencies
# Install ca-certificates - it is needed to verify TLS Certificates when establishing HTTPS connections
RUN apt update -y \
    && apt install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt autoremove -y \
    && apt clean -y \
    && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder environment to our runtime environment
COPY --from=builder /app/target/release/zero2prod zero2prod
# We need the configuration file at runtime!
COPY configuration configuration

ENV APP_ENVIRONMENT production
# When `docker run` is executed, launch the binary!
ENTRYPOINT ["./zero2prod"]
