#!/bin/sh
# /w = ws-intros; jobs.txt lines: <id>\t<path under dl/>
while IFS="$(printf '\t')" read id path; do
  [ -n "$(ls /out/$id-*.bin 2>/dev/null)" ] && continue
  cp "/w/dl/$path" /tmp/a.exe
  timeout 200 python3 /w/docker/dump.py /tmp/a.exe "$id" 150 2>&1 | grep -E "found|blob" | sed "s|^|$id |"
  wineserver -k 2>/dev/null; WINEPREFIX=/root/.wine64 wineserver -k 2>/dev/null; sleep 1
done < /w/${JOBS:-jobs.txt}
