// MIT shim (unisynth, change add-tf3-import): the one math type the Tunefish 3 synth sources use.
#ifndef TF3_SHIM_MATH_HPP
#define TF3_SHIM_MATH_HPP

struct eVector2 {
  eF32 x = 0.0f, y = 0.0f;
  void set(eF32 nx, eF32 ny) { x = nx; y = ny; }
};

#endif
