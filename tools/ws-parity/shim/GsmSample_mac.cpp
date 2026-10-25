// macOS replacement for WaveSabre's ACM-based GsmSample: MS GSM 6.10 (WAV49, 65-byte blocks = 320 samples) via libgsm.
#include <WaveSabreCore/GsmSample.h>
#include <string.h>
#include <gsm.h>
namespace WaveSabreCore {
GsmSample::GsmSample(char *data, int compressedSize, int uncompressedSize, WAVEFORMATEX *waveFormat)
  : CompressedSize(compressedSize), UncompressedSize(uncompressedSize) {
  WaveFormatData = new char[sizeof(WAVEFORMATEX) + waveFormat->cbSize];
  memcpy(WaveFormatData, waveFormat, sizeof(WAVEFORMATEX) + waveFormat->cbSize);
  CompressedData = new char[compressedSize];
  memcpy(CompressedData, data, compressedSize);
  gsm g = gsm_create(); int one = 1; gsm_option(g, GSM_OPT_WAV49, &one);
  int blocks = compressedSize / 65;
  short *pcm = new short[blocks * 320];
  int n = 0;
  for (int b = 0; b < blocks; b++) {
    gsm_byte *p = (gsm_byte *)CompressedData + b * 65;
    if (gsm_decode(g, p, pcm + n) < 0) break;
    if (gsm_decode(g, p + 33, pcm + n + 160) < 0) break;
    n += 320;
  }
  gsm_destroy(g);
  int maxLen = uncompressedSize > 0 && uncompressedSize < n ? uncompressedSize : n;
  SampleLength = maxLen;
  SampleData = new float[SampleLength];
  for (int i = 0; i < SampleLength; i++) SampleData[i] = (float)((double)pcm[i] / 32768.0);
  delete[] pcm;
}
GsmSample::~GsmSample() { delete[] WaveFormatData; delete[] CompressedData; delete[] SampleData; }
HACMDRIVERID GsmSample::driverId = 0;
}
