// MIT harness (ours) for tools/tf-parity: renders a .tfm with the upstream Tunefish 4 player (GPL-3, built outside
// the repo by build-ref.sh) into a 16-bit stereo WAV, 512-frame blocks at 44.1 kHz. It computes the song length
// itself from the file (skipping instruments without events), so the upstream length helper — which reads before
// an empty instrument's event array — is never called.
//   render-ref <song.tfm> <out.wav> [tail-seconds]
#include "tf4player.hpp"
#include <cstdio>
#include <cstdlib>
#include <vector>

static unsigned u16(const std::vector<unsigned char> &b, size_t o) { return b[o] | (b[o + 1] << 8); }

// Latest last-event time over instruments with events (rows at 4 per beat), 0 if none.
static float song_length(const std::vector<unsigned char> &b) {
  unsigned n = u16(b, 0), tempo = u16(b, 2);
  float spr = 1.0f / ((float)(tempo * 4) / 60.0f);
  size_t o = 4 + 2 * n + 4 + 112 * n + 4;
  float len = 0.0f;
  for (unsigned i = 0; i < n; i++) {
    unsigned e = u16(b, 4 + 2 * i), row = 0;
    for (unsigned k = 0; k < e; k++) row += u16(b, o + 2 * k);
    if (e && (float)row * spr > len) len = (float)row * spr;
    o += 4 * e;
  }
  return len;
}

int main(int argc, char **argv) {
  if (argc < 3) {
    fprintf(stderr, "usage: render-ref <song.tfm> <out.wav> [tail-seconds]\n");
    return 2;
  }
  FILE *f = fopen(argv[1], "rb");
  if (!f) return 1;
  std::vector<unsigned char> song;
  for (int c; (c = fgetc(f)) != EOF;) song.push_back((unsigned char)c);
  fclose(f);
  float tail = argc > 3 ? (float)atof(argv[3]) : 0.0f;
  static eTfPlayer player;  // large: keep off the stack
  eTfPlayerInit(player);
  eTfPlayerSetSampleRate(player, 44100);
  eTfPlayerLoadSong(player, song.data(), (eU32)song.size(), 0.0f);
  unsigned blocks = (unsigned)(44100.0f * song_length(song) / 512.0f) + 1 + (unsigned)(tail * 44100.0f / 512.0f);
  eTfPlayerStart(player, 0.0f);
  std::vector<short> pcm;
  pcm.reserve(blocks * 1024);
  for (unsigned b = 0; b < blocks; b++) {
    const eS16 *out = nullptr;
    eTfPlayerProcess(player, &out);
    pcm.insert(pcm.end(), out, out + 1024);
  }
  FILE *w = fopen(argv[2], "wb");
  unsigned nb = (unsigned)pcm.size() * 2;
  unsigned h[] = {0x46464952, 36 + nb, 0x45564157, 0x20746d66, 16, 0x00020001, 44100, 44100 * 4, 0x00100004, 0x61746164, nb};
  fwrite(h, 1, sizeof h, w);
  fwrite(pcm.data(), 2, pcm.size(), w);
  fclose(w);
  return 0;
}
