# This Dockerfile was generated from github.com/sourcegraph/godockerize. It
# was not written by a human, and as such looks janky. As you change this
# file, please don't be scared to make it more pleasant / remove hadolint
# ignores.

FROM sourcegraph/alpine-3.14:159028_2022-07-07_1f3b17ce1db8@sha256:25d682b5fd069c716c2b29dcf757c0dc0ce29810a07f91e1347901920272b4a7

RUN apk --no-cache add pcre sqlite-libs libev

# The comby/comby image is a small binary-only distribution. See the bin and src directories
# here: https://github.com/comby-tools/comby/tree/master/dockerfiles/alpine
# hadolint ignore=DL3022
COPY --from=comby/comby:alpine-3.14-1.8.1@sha256:a5e80d6bad6af008478679809dc8327ebde7aeff7b23505b11b20e36aa62a0b2 /usr/local/bin/comby /usr/local/bin/comby

ARG COMMIT_SHA="unknown"
ARG DATE="unknown"
ARG VERSION="unknown"

LABEL org.opencontainers.image.revision=${COMMIT_SHA}
LABEL org.opencontainers.image.created=${DATE}
LABEL org.opencontainers.image.version=${VERSION}
LABEL com.sourcegraph.github.url=https://github.com/sourcegraph/sourcegraph/commit/${COMMIT_SHA}

ENV CACHE_DIR=/mnt/cache/searcher
RUN mkdir -p ${CACHE_DIR} && chown -R sourcegraph:sourcegraph ${CACHE_DIR}
USER sourcegraph
ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/searcher"]
COPY searcher /usr/local/bin/
