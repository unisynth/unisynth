// MIT harness (unisynth, change add-tf3-import): prints the original point oscillator's cycle for one set of point
// parameters, so the unified synth's cycle builder can be compared sample by sample.
//   cycle-ref <points p> <p1off> <p1val> <p1int> … <p4int> <final int>    (14 normalized values, spec §5)
// Prints 257 lines, the cycle after its mean is removed.
#include "system/system.hpp"
#include "math/math.hpp"
#define private public
#include "synth/tunefish3.hpp"
#undef private
#include <cstdio>

eU32 g_tf3Seed = 1;
eU32 g_tf3Tick = 1000;

int main(int argc, char **argv) {
  if (argc != 15) { std::fprintf(stderr, "usage: cycle-ref <14 values>\n"); return 2; }
  static eF32 params[TF_PARAM_COUNT];
  for (int i = 0; i < 14; i++) params[TF_OSC_POINTCOUNT + i] = std::strtof(argv[1 + i], nullptr);
  tfSpline s;
  s.update(params, nullptr, nullptr);
  for (eU32 i = 0; i <= TF_OSCILLATOR_SAMPLES; i++) std::printf("%.9g\n", s.m_samples[i]);
  return 0;
}
