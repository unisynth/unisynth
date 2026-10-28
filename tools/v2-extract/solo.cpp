// v2solo: renders a .v2m with v2redux, only the channels in `mask` audible (their voices and effects run as in the
// full song; muted channels feed no bus). Writes float32 stereo WAV of `secs` seconds. Optional engine profile
// (`kkrieger2004`: v2redux eras::kkrieger2004). Parity references for songs that need a profile, and a debugging aid.
#include "v2redux.h"
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>
using namespace v2redux;
int main(int argc, char **argv) {
  if (argc < 5) { fprintf(stderr, "usage: v2solo song.v2m out.wav mask secs [kkrieger2004]\n"); return 2; }
  FILE *f = fopen(argv[1], "rb");
  if (!f) return 2;
  fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
  std::vector<unsigned char> b(n);
  if (fread(b.data(), 1, n, f) != (size_t)n) return 2;
  fclose(f);
  static Player p;
  bool k = argc > 5 && std::string(argv[5]) == "kkrieger2004";
  if ((k ? p.open(b.data(), n, eras::kkrieger2004) : p.open(b.data(), n)) != Result::OK) { fprintf(stderr, "open failed\n"); return 1; }
  p.setChannelMask((uint32_t)strtoul(argv[3], 0, 0));
  p.play();
  long frames = (long)(atof(argv[4]) * 44100);
  std::vector<float> buf(frames * 2);
  for (long i = 0; i < frames; i += 4096) p.render(&buf[i * 2], (uint32_t)(frames - i < 4096 ? frames - i : 4096));
  FILE *o = fopen(argv[2], "wb");
  int ds = (int)(frames * 8);
  auto w32 = [&](int v) { fwrite(&v, 4, 1, o); };
  auto w16 = [&](short v) { fwrite(&v, 2, 1, o); };
  fwrite("RIFF", 1, 4, o); w32(36 + ds); fwrite("WAVEfmt ", 1, 8, o); w32(16); w16(3); w16(2); w32(44100); w32(44100 * 8); w16(8); w16(32);
  fwrite("data", 1, 4, o); w32(ds); fwrite(buf.data(), 8, frames, o); fclose(o);
  return 0;
}
