// MIT harness (ours) for tools/ws-parity: renders a WaveSabre song blob with an upstream WaveSabre build the way
// upstream's players do — one render thread, 100-frame RenderSamples steps — into a 16-bit stereo WAV.
// Built against an uncommitted upstream checkout; usage: render-device <song.bin> <out.wav>  ($GM_DLS for Adultery).
#include <WaveSabreCore.h>
#include <WaveSabrePlayerLib/SongRenderer.h>
#include <cstdio>
#include <vector>

using namespace WaveSabrePlayerLib;

static WaveSabreCore::Device *factory(SongRenderer::DeviceId id) {
  using D = SongRenderer::DeviceId;
  switch (id) {
  case D::Falcon: return new WaveSabreCore::Falcon();
  case D::Slaughter: return new WaveSabreCore::Slaughter();
  case D::Thunder: return new WaveSabreCore::Thunder();
  case D::Scissor: return new WaveSabreCore::Scissor();
  case D::Leveller: return new WaveSabreCore::Leveller();
  case D::Crusher: return new WaveSabreCore::Crusher();
  case D::Echo: return new WaveSabreCore::Echo();
  case D::Smasher: return new WaveSabreCore::Smasher();
  case D::Chamber: return new WaveSabreCore::Chamber();
  case D::Twister: return new WaveSabreCore::Twister();
  case D::Cathedral: return new WaveSabreCore::Cathedral();
  case D::Adultery: return new WaveSabreCore::Adultery();
  case D::Specimen: return new WaveSabreCore::Specimen();
  }
  return nullptr;
}

int main(int argc, char **argv) {
  if (argc != 3) {
    fprintf(stderr, "usage: render-device <song.bin> <out.wav>\n");
    return 2;
  }
  FILE *f = fopen(argv[1], "rb");
  if (!f) return 1;
  std::vector<unsigned char> blob;
  for (int c; (c = fgetc(f)) != EOF;) blob.push_back((unsigned char)c);
  fclose(f);
  SongRenderer::Song song = {factory, blob.data()};
  SongRenderer r(&song, 1);
  int n = (int)(r.GetLength() * r.GetSampleRate()) * 2;
  std::vector<short> out(n);
  const int step = 100 * 2; // upstream PreRenderPlayer / RealtimePlayer stepSize
  for (int i = 0; i < n; i += step) r.RenderSamples(&out[i], n - i < step ? n - i : step);
  FILE *w = fopen(argv[2], "wb");
  unsigned nb = (unsigned)n * 2;
  unsigned h[] = {0x46464952, 36 + nb, 0x45564157, 0x20746d66, 16, 0x00020001, 44100, 44100 * 4, 0x00100004, 0x61746164, nb};
  fwrite(h, 1, sizeof h, w);
  fwrite(out.data(), 2, n, w);
  fclose(w);
  return 0;
}
