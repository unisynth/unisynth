// MIT harness (unisynth, change add-tf3-import): prints the original PAD table (2^16 samples) for one set of additive
// parameters, so the unified synth's table can be compared by its magnitude spectrum (phases are random in both).
//   pad-ref <bandwidth> <scale> <damp> <harmonics> <profile>    (normalized values, spec §5)
#include "system/system.hpp"
#include "math/math.hpp"
#define private public
#include "synth/tunefish3.hpp"
#undef private
#include <cstdio>

eU32 g_tf3Seed = 1;
eU32 g_tf3Tick = 1000;

int main(int argc, char **argv) {
  if (argc != 6) { std::fprintf(stderr, "usage: pad-ref <bandwidth> <scale> <damp> <harmonics> <profile>\n"); return 2; }
  static eF32 params[TF_PARAM_COUNT];
  params[TF_ADD_VOLUME] = 1.0f;
  params[TF_ADD_BANDWIDTH] = std::strtof(argv[1], nullptr);
  params[TF_ADD_SCALE] = std::strtof(argv[2], nullptr);
  params[TF_ADD_DAMP] = std::strtof(argv[3], nullptr);
  params[TF_ADD_HARMONICS] = std::strtof(argv[4], nullptr);
  params[TF_ADD_PROFILE] = std::strtof(argv[5], nullptr);
  tfInstrument::m_freqTableReady = eFALSE;
  tfInstrument *ins = new tfInstrument();  // prepares the tuning table the build reads
  tfAddSynth a;
  a.update(params, 44100);
  for (eU32 i = 0; i < TF_ADDSYNTHTABLESIZE / 2; i++) std::printf("%.9g\n", a.m_padSynthTable[i]);
  (void)ins;
  return 0;
}
