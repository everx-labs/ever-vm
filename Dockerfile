FROM alpine:latest
RUN addgroup --gid 1000 jenkins && \
    adduser -D -G jenkins jenkins
COPY --chown=jenkins:jenkins ./Cargo.* ./*.md ./*.rs /tonlabs/ton-labs-vm/
COPY --chown=jenkins:jenkins ./src /tonlabs/ton-labs-vm/src
VOLUME ["/tonlabs/ton-labs-vm"]
USER jenkins