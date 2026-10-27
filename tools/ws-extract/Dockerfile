FROM debian:bookworm-slim
RUN dpkg --add-architecture i386 && apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends wine wine32 wine64 xvfb xauth python3 procps ca-certificates && rm -rf /var/lib/apt/lists/*
ENV WINEDEBUG=-all WINEARCH=win32 WINEPREFIX=/root/.wine32
RUN xvfb-run -a wineboot -i >/dev/null 2>&1 || true
