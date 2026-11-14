# The parity suite's runtime (tests/parity/README.md): Ubuntu 24.04 amd64 with the pinned Poppler and fonts that
# scripts/parity-install-runtime.sh installs, Rust, and Chrome for Testing for new oracles. Built and run by
# docspring/parity/run.sh.
FROM ubuntu:24.04

ARG CHROME_VERSION=156.0.8078.80

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
       build-essential ca-certificates curl git jq python3 sudo unzip fontconfig \
       libasound2t64 libatk-bridge2.0-0t64 libatk1.0-0t64 libcups2t64 libdbus-1-3 libdrm2 libgbm1 libnss3 \
       libpango-1.0-0 libxcomposite1 libxdamage1 libxfixes3 libxkbcommon0 libxrandr2 libcairo2 \
    && rm -rf /var/lib/apt/lists/*

RUN curl --fail --location --silent --show-error \
       "https://storage.googleapis.com/chrome-for-testing-public/${CHROME_VERSION}/linux64/chrome-linux64.zip" \
       --output /tmp/chrome.zip \
    && unzip -q /tmp/chrome.zip -d /opt \
    && rm /tmp/chrome.zip \
    && ln -s /opt/chrome-linux64/chrome /usr/local/bin/chromium

COPY scripts/parity-install-runtime.sh /runtime/scripts/
COPY tests/parity/fonts /runtime/tests/parity/fonts
RUN /runtime/scripts/parity-install-runtime.sh

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
