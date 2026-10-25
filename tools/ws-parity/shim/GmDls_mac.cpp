// macOS replacement for GmDls::Load: reads $GM_DLS (must be the Windows gm.dls; WaveSabre uses fixed offsets).
#include <WaveSabreCore/GmDls.h>
#include <stdio.h>
#include <stdlib.h>
namespace WaveSabreCore {
unsigned char *GmDls::Load() {
  const char *p = getenv("GM_DLS"); FILE *f = p ? fopen(p, "rb") : 0;
  if (!f) { fprintf(stderr, "GM_DLS not set or unreadable\n"); exit(2); }
  fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
  unsigned char *d = new unsigned char[n]; fread(d, 1, n, f); fclose(f); return d;
}
}
