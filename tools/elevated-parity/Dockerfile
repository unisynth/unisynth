# Reference renderer image for Mentor's 4k synth (change add-elevated-import): the ws-extract Wine base plus nasm and
# the i686 MinGW compiler, so the original synth.asm + music.asm assemble (nasm -f win32) and link with our MIT harness.
FROM debian:bookworm-slim
RUN dpkg --add-architecture i386 && apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends wine wine32 nasm gcc-mingw-w64-i686 ca-certificates && rm -rf /var/lib/apt/lists/*
ENV WINEDEBUG=-all WINEARCH=win32 WINEPREFIX=/root/.wine32
RUN wineboot -i >/dev/null 2>&1 || true
