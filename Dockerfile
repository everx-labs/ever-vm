ARG TON_LABS_TYPES_IMAGE=tonlabs/ton-labs-types:latest
ARG TON_LABS_VM_IMAGE=tonlabs/ton-labs-vm:latest

FROM alpine:latest as ton-labs-vm-src
RUN addgroup --gid 1000 jenkins && \
    adduser -D -G jenkins jenkins
COPY --chown=jenkins:jenkins ./Cargo.* ./*.md ./*.rs /tonlabs/ton-labs-vm/
COPY --chown=jenkins:jenkins ./src /tonlabs/ton-labs-vm/src
VOLUME ["/tonlabs/ton-labs-vm"]
USER jenkins

FROM $TON_LABS_TYPES_IMAGE as ton-labs-types-src
FROM $TON_LABS_VM_IMAGE as ton-labs-vm-source
FROM alpine:latest as ton-labs-vm-full
RUN addgroup --gid 1000 jenkins && \
    adduser -D -G jenkins jenkins && \
    apk update && apk add zip
COPY --from=ton-labs-types-src --chown=jenkins:jenkins /tonlabs/ton-labs-types /tonlabs/ton-labs-types
COPY --from=ton-labs-vm-source --chown=jenkins:jenkins /tonlabs/ton-labs-vm /tonlabs/ton-labs-vm
VOLUME ["/tonlabs"]

FROM rust:latest as ton-labs-vm-rust
RUN apt -qqy update && apt -qyy install apt-utils && \
    curl -sL https://deb.nodesource.com/setup_12.x | bash - && \
    apt-get install -qqy nodejs && \
    adduser --group jenkins && \
    adduser -q --disabled-password --gid 1000 jenkins && \
    mkdir /tonlabs && chown -R jenkins:jenkins /tonlabs
COPY --from=ton-labs-vm-full --chown=jenkins:jenkins /tonlabs/ton-labs-types /tonlabs/ton-labs-types
COPY --from=ton-labs-vm-full --chown=jenkins:jenkins /tonlabs/ton-labs-vm    /tonlabs/ton-labs-vm
WORKDIR /tonlabs/ton-labs-vm
