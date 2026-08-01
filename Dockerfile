# Multi-stage build for utterance: Angular frontend + Rust backend, served from
# one image (the backend serves the bundle and the API). Mirrors the fleet's
# image convention, xinutec/<app>:latest.

# --- frontend: build the Angular bundle ---
FROM node:24-alpine AS frontend
WORKDIR /fe
# pnpm-workspace.yaml belongs in this layer, not with the sources: it carries the
# install-script allowlist, and without it esbuild never unpacks its binary and
# the build below fails on a dependency that looks installed.
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml ./
# git: the shared layout harness is a git dependency (github:xinutec/ui-harness)
# and the install covers devDependencies, so it gets cloned — and node:alpine
# ships no git.
#
# pnpm is taken unpinned. The host gets its copy from the flake, and pinning a
# second version here would be two numbers held level by hand; the lockfile is
# what has to match, and --frozen-lockfile fails rather than drift.
RUN apk add --no-cache git ca-certificates \
    && npm install -g pnpm \
    && pnpm install --frozen-lockfile
COPY frontend/ .
RUN pnpm exec ng build --configuration production

# --- backend: build the Rust binary, deps in their own cached layer ---
FROM rust:1-bookworm AS backend
WORKDIR /app
# Every workspace member's manifest, not just the root's. Cargo cannot *load* a
# workspace unless all of them exist, so a missing one fails the build in a
# tenth of a second with "failed to load manifest for workspace member" —
# before compiling anything. Each also needs a stub source, or the priming
# build has nothing to compile for it.
COPY Cargo.toml Cargo.lock ./
COPY utterance-analysis/Cargo.toml utterance-analysis/
COPY utterance-mapping/Cargo.toml utterance-mapping/
COPY utterance-realisation/Cargo.toml utterance-realisation/
RUN mkdir -p src utterance-analysis/src utterance-mapping/src utterance-realisation/src \
    && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs \
    && echo '' > utterance-analysis/src/lib.rs \
    && echo '' > utterance-mapping/src/lib.rs \
    && echo '' > utterance-realisation/src/lib.rs \
    && cargo build --release \
    && rm -rf src utterance-analysis/src utterance-mapping/src utterance-realisation/src
COPY src/ src/
COPY utterance-analysis/src/ utterance-analysis/src/
COPY utterance-mapping/src/ utterance-mapping/src/
COPY utterance-realisation/src/ utterance-realisation/src/
# Touched so cargo rebuilds them against the primed dependency cache rather than
# trusting mtimes that came out of a COPY.
RUN touch src/main.rs src/lib.rs \
    utterance-analysis/src/lib.rs utterance-mapping/src/lib.rs utterance-realisation/src/lib.rs \
    && cargo build --release

# --- runtime ---
FROM debian:bookworm-slim
# ca-certificates for the sign-in call to Nextcloud, which is TLS whenever the
# public URL is used rather than the in-cluster one.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Unlike the other fleet apps this one *writes*: recordings and their derived
# voiceprints land in DATA_DIR, which is a volume. The binary and bundle stay
# root-owned and read-only; only the volume is writable, and the pod's fsGroup
# hands it to this gid.
RUN groupadd --gid 65532 utterance \
    && useradd --uid 65532 --gid utterance --no-create-home --shell /usr/sbin/nologin utterance
WORKDIR /app
COPY --from=backend /app/target/release/utterance /usr/local/bin/utterance
COPY --from=frontend /fe/dist/utterance-web/browser ./public
ENV STATIC_DIR=/app/public \
    DATA_DIR=/data \
    BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
USER utterance
CMD ["utterance"]
