// v2events: the V2 sequencer oracle. Compiles v2redux's v2seq.cpp against stub synth entry points that record
// every MIDI message with the output frame it is sent at, then plays the song (already upgraded to version 6) at
// 44100 Hz. Prints one line per message: `frame status data…`, and `end <frames>` (CalcSongSamples).
#include "v2seq.h"
#include <cstdio>
#include <cstdlib>
#include <vector>
static unsigned long long g_frame = 0;
static bool g_on = false; // CalcSongSamples also ticks: print only during playback
void __stdcall synthInit(void *, const void *, int) {}
void __stdcall synthRender(void *, void *, int smp, void *, int) { g_frame += (unsigned)smp; }
void __stdcall synthProcessMIDI(void *, const void *ptr) {
  if (!g_on) return;
  const unsigned char *m = (const unsigned char *)ptr;
  unsigned char stat = 0;
  while (*m != 0xfd) {
    if (*m & 0x80) stat = *m++;
    int n = ((stat & 0xf0) == 0xc0) ? 1 : 2;
    printf("%llu %d", g_frame, stat);
    for (int i = 0; i < n; i++) printf(" %d", *m++);
    printf("\n");
  }
}
void __stdcall synthSetGlobals(void *, const void *) {}
void __stdcall synthSetEra(void *, int, unsigned, unsigned) {}
void __stdcall synthSetSeed(void *, unsigned long long) {}
void __stdcall synthSetChanMute(void *, unsigned int) {}
void __stdcall synthGetPoly(void *, void *) {}
void __stdcall synthGetChannelPeaks(void *, float *) {}
void __stdcall synthSetLyrics(void *, const char **) {}
int main(int argc, char **argv) {
  FILE *f = fopen(argv[1], "rb");
  if (!f) return 2;
  fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
  std::vector<unsigned char> b(n);
  if (fread(b.data(), 1, n, f) != (size_t)n) return 2;
  static V2MPlayer p;
  p.Init();
  if (!p.Open(b.data(), 44100)) { fprintf(stderr, "open failed\n"); return 1; }
  unsigned total = p.CalcSongSamples();
  g_frame = 0;
  g_on = true;
  p.Play(0);
  static float buf[2 * 4096];
  while (p.IsPlaying()) p.Render(buf, 4096);
  printf("end %u\n", total);
  return 0;
}
