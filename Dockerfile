# Dedalo container image.
#
# This image assembles the statically linked musl binaries the release
# workflow already built and checksummed, rather than rebuilding them: the
# artifact people download and the artifact inside the image are then the same
# bytes. `image/<arch>/dedalo` is laid out by .github/workflows/release.yml.
#
# To reproduce locally for one architecture:
#
#   cargo build --release --target x86_64-unknown-linux-musl -p dedalo
#   mkdir -p image/amd64 && cp target/x86_64-unknown-linux-musl/release/dedalo image/amd64/
#   docker build -t dedalo .
#
# To build from source instead, use the flake: `nix build github:4137314/dedalo`.

FROM alpine:3.22

# Dedalo reads history through the git binary, so git is not optional here.
# ca-certificates is needed the moment a settlement backend talks to an RPC.
RUN apk add --no-cache git ca-certificates tini \
    && adduser -D -u 10001 dedalo

# buildx sets TARGETARCH per platform; plain `docker build` does not set it at
# all, which made the local instructions above fail. The default keeps a
# single-arch build working, and buildx still overrides it.
ARG TARGETARCH=amd64
COPY image/${TARGETARCH}/dedalo /usr/local/bin/dedalo
RUN chmod 0755 /usr/local/bin/dedalo && dedalo --version

# Repositories are mounted here; running as a non-root user means the image
# cannot rewrite a host checkout it was not given.
WORKDIR /repo
USER dedalo

# Git refuses to operate on a checkout owned by another user unless told the
# directory is trusted, which is exactly the case for a mounted repository.
ENV GIT_CONFIG_COUNT=1 \
    GIT_CONFIG_KEY_0=safe.directory \
    GIT_CONFIG_VALUE_0=/repo

ENTRYPOINT ["/sbin/tini", "--", "dedalo"]
CMD ["status"]

LABEL org.opencontainers.image.title="dedalo" \
      org.opencontainers.image.description="Turn code merges into sustainable open-source funding" \
      org.opencontainers.image.source="https://github.com/4137314/dedalo" \
      org.opencontainers.image.licenses="MIT"
