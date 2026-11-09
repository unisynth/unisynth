// MIT shim (unisynth, change add-tf3-import): the small runtime the Tunefish 3 synth sources expect, written for the
// reference renderer only. It is placed next to a copy of the synth sources outside the repository, so their
// `#include "../system/system.hpp"` finds it instead of Enigma Studio's Windows runtime.
//
// Semantics follow the 32-bit Windows intro build (research/tunefish3-functional-spec.md, "fi" and quirk 13):
// float → int rounds half to even, "log" is log10, SSE vectors hold (left, right) in lanes 3 and 2.
#ifndef TF3_SHIM_SYSTEM_HPP
#define TF3_SHIM_SYSTEM_HPP

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include <xmmintrin.h>
#include <emmintrin.h>

#define eUSE_SSE
#define eINLINE inline
#define eFORCEINLINE inline
#define eALIGN16 __attribute__((aligned(16)))
#define eTRUE true
#define eFALSE false
#define eNULL nullptr
#define eNOID 0
#define ePI 3.1415926535897932384626433832795f
#define eTWOPI ePI*2.0f
#define eMAX_NAME_LENGTH 64
#define eMAX_RAND 2147483647
#define eASSERT(x) ((void)0)
#define eCLEAR_UNDENORMALIZED()
#define eASSERT_UNDENORMALIZED()
#define eSAFE_DELETE(p) do { delete (p); (p) = nullptr; } while (0)
#define eSAFE_DELETE_ARRAY(p) do { delete[] (p); (p) = nullptr; } while (0)

typedef char eChar;
typedef int8_t eS8;
typedef uint8_t eU8;
typedef int16_t eS16;
typedef uint16_t eU16;
typedef int32_t eS32;
typedef uint32_t eU32;
typedef int eInt;
typedef float eF32;
typedef double eF64;
typedef bool eBool;
typedef void *ePtr;
typedef eU32 eID;
#define eF32A alignas(16) float  // macro: the sources declare aligned arrays of it
typedef __m128 eF32x2;

// ---------- memory and strings ----------

inline void eMemSet(void *p, int v, size_t n) { std::memset(p, v, n); }
inline void eMemCopy(void *d, const void *s, size_t n) { std::memcpy(d, s, n); }
inline void *eMemAllocAlignedAndZero(size_t n, size_t align) {
  void *p = nullptr;
  if (posix_memalign(&p, align < sizeof(void *) ? sizeof(void *) : align, n) != 0) return nullptr;
  std::memset(p, 0, n);
  return p;
}
inline void eFreeAligned(void *p) { std::free(p); }
inline void eStrCopy(eChar *d, const eChar *s) { std::strcpy(d, s); }
inline void eStrNCopy(eChar *d, const eChar *s, eU32 n) { std::strncpy(d, s, n); if (n) d[n - 1] = 0; }

struct eString {
  std::string s;
  eString(const char *c = "") : s(c) {}
  operator const eChar *() const { return s.c_str(); }
};

template <class T> class eArray {
  std::vector<T> v;
public:
  eU32 size() const { return (eU32)v.size(); }
  void resize(eU32 n) { v.resize(n); }
  void append(const T &x) { v.push_back(x); }
  void removeAt(eU32 i) { v.erase(v.begin() + i); }
  void clear() { v.clear(); }
  eBool isEmpty() const { return v.empty(); }
  T &operator[](eU32 i) { return v[i]; }
  const T &operator[](eU32 i) const { return v[i]; }
};

// ---------- math (x87 / Windows semantics) ----------

inline eInt eFtoL(eF32 x) { return (eInt)std::lrintf(x); }
inline eF32 eRound(eF32 x) { return std::nearbyintf(x); }
inline eF32 eLog(eF32 x) { return (eF32)std::log10((eF64)x); }
inline eF32 eSin(eF32 x) { return (eF32)std::sin((eF64)x); }
inline eF32 eCos(eF32 x) { return (eF32)std::cos((eF64)x); }
inline eF32 eSinH(eF32 x) { return (eF32)std::sinh((eF64)x); }
inline eF32 eExp(eF32 x) { return (eF32)std::exp((eF64)x); }
inline eF32 ePow(eF32 b, eF32 e) { return (eF32)std::pow((eF64)b, (eF64)e); }
inline eF32 eSqrt(eF32 x) { return std::sqrt(x); }
inline eF32 eMod(eF32 a, eF32 b) { return std::fmod(a, b); }
inline eF32 eAbs(eF32 x) { return std::fabs(x); }
inline eInt eAbs(eInt x) { return x < 0 ? -x : x; }
inline eF32 eSign(eF32 x) { return x == 0.0f ? 0.0f : std::copysign(1.0f, x); }
template <class T> inline T eMin(const T &a, const T &b) { return a < b ? a : b; }
template <class T> inline T eMax(const T &a, const T &b) { return a > b ? a : b; }
template <class T> inline T eClamp(const T &lo, const T &x, const T &hi) { return x < lo ? lo : x > hi ? hi : x; }
template <class T> inline T eLerp(const T &a, const T &b, eF32 t) { return a + (b - a) * t; }

inline void eUndenormalise(eF32 &s) {
  eU32 bits;
  std::memcpy(&bits, &s, 4);
  if ((bits & 0x7f800000) == 0) s = 0.0f;
}

// ---------- random: Park–Miller, and a tick count the harness advances with the render clock ----------

extern eU32 g_tf3Seed;  // global generator state
extern eU32 g_tf3Tick;  // stands in for GetTickCount (ms), set by the harness per block

inline eU32 eRandom(eU32 &seed) {
  eU32 lo = 16807 * (seed & 0xffff);
  eU32 hi = 16807 * (seed >> 16);
  lo += (hi & 0x7fff) << 16;
  hi >>= 15;
  lo += hi;
  lo = (lo & 0x7fffffff) + (lo >> 31);
  seed = lo;
  return seed;
}
inline eU32 eRandom() { return eRandom(g_tf3Seed); }
inline void eRandomize(eU32 seed) { g_tf3Seed = seed + 1 ? seed + 1 : 1; }
inline eU32 eRandomSeed() { return g_tf3Tick; }
inline eInt eRandom(eInt lo, eInt hi) { return (eInt)(eRandom() % (eU32)(hi - lo)) + lo; }
inline eInt eRandom(eInt lo, eInt hi, eU32 &seed) { return (eInt)(eRandom(seed) % (eU32)(hi - lo)) + lo; }
inline eF32 eRandomF() { return (eF32)eRandom() / (eF32)eMAX_RAND; }
inline eF32 eRandomF(eU32 &seed) { return (eF32)eRandom(seed) / (eF32)eMAX_RAND; }
inline eF32 eRandomF(eF32 lo, eF32 hi) { return eRandomF() * (hi - lo) + lo; }
inline eF32 eRandomF(eF32 lo, eF32 hi, eU32 &seed) { return eRandomF(seed) * (hi - lo) + lo; }

// ---------- two-lane vectors (left in lane 3, right in lane 2, as the original's SSE build) ----------

inline eF32x2 eSIMDLoad2(eF32 l, eF32 r) { return _mm_set_ps(l, r, 0.0f, 0.0f); }
inline eF32x2 eSIMDSet2(eF32 v) { return _mm_set1_ps(v); }
inline void eSIMDStore(eF32x2 v, eF32 *l, eF32 *r) {
  eF32A o[4];
  _mm_store_ps(o, v);
  *l = o[3];
  *r = o[2];
}
inline eF32x2 eSIMDMul(eF32x2 a, eF32x2 b) { return _mm_mul_ps(a, b); }
inline eF32x2 eSIMDDiv(eF32x2 a, eF32x2 b) { return _mm_div_ps(a, b); }
inline eF32x2 eSIMDAdd(eF32x2 a, eF32x2 b) { return _mm_add_ps(a, b); }
inline eF32x2 eSIMDSub(eF32x2 a, eF32x2 b) { return _mm_sub_ps(a, b); }
inline eF32x2 eSIMDMulAdd(eF32x2 add, eF32x2 a, eF32x2 b) { return _mm_add_ps(add, _mm_mul_ps(a, b)); }
inline eF32x2 eSIMDMulSub(eF32x2 sub, eF32x2 a, eF32x2 b) { return _mm_sub_ps(sub, _mm_mul_ps(a, b)); }
inline eF32x2 eSIMDMin(eF32x2 a, eF32x2 b) { return _mm_min_ps(a, b); }
inline eF32x2 eSIMDMax(eF32x2 a, eF32x2 b) { return _mm_max_ps(a, b); }
inline void eSIMDUndenormalise(eF32x2 &) {}  // flush-to-zero mode handles it in the SSE build
inline eBool eSSECmpTrue(eF32x2 v) { eF32A o[4]; _mm_store_ps(o, v); return o[0] != 0.0f; }
inline void eSetSSEFlushToZeroMode() { _mm_setcsr(_mm_getcsr() | 0x8000); }

// ---------- threads, locks, streams: the harness drives the player synchronously ----------

class eCriticalSection {
public:
  void enter() const {}
  void leave() const {}
};
typedef void (*eThreadFunc)(ePtr);
inline ePtr eThreadStart(eThreadFunc, ePtr, eBool) { return nullptr; }
inline void eThreadEnd(ePtr, eBool) {}
inline void eSleep(eU32) {}

class eDataStream {
public:
  eU8 readByte() { return 0; }
  eU16 readWord() { return 0; }
  eU32 readDword() { return 0; }
  eString readString() { return eString(); }
  void writeByte(eU8) {}
  void writeWord(eU16) {}
  void writeDword(eU32) {}
  void writeString(const eChar *) {}
};

#endif
