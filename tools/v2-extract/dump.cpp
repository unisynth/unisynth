// v2dumpjson: reads a .v2m with the v2redux loader (external, CC0) and prints one JSON line: detected format version,
// channel/event counts, globals, the used patches (89 v6 params + mod entries) and the speech size. With
// `--canon <out>` it also writes the canonical (v6-layout) bytes, the oracle for synth-convert's own upgrade.
// Built by `v2.ts` against $V2REDUX/src; never part of the repo build.
#include "v2load.h"
#include <cstdio>
#include <cstring>
#include <set>
#include <vector>
using namespace v2redux;

static unsigned u32(const unsigned char *p) { return p[0] | p[1] << 8 | p[2] << 16 | (unsigned)p[3] << 24; }

int main(int argc, char **argv) {
  if (argc < 2) { fprintf(stderr, "usage: v2dumpjson song.v2m [--canon out.bin]\n"); return 2; }
  FILE *f = fopen(argv[1], "rb");
  if (!f) { fprintf(stderr, "cannot open %s\n", argv[1]); return 2; }
  fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
  std::vector<unsigned char> b(n);
  if (fread(b.data(), 1, n, f) != (size_t)n) return 2;
  fclose(f);
  V2LoadResult r = v2loadCanonicalize(b.data(), n);
  if (r.result != Result::OK) { printf("{\"error\":%d,\"version\":%d}\n", (int)r.result, r.version); return 0; }
  if (argc >= 4 && !strcmp(argv[2], "--canon")) {
    FILE *o = fopen(argv[3], "wb");
    fwrite(r.data, 1, r.size, o);
    fclose(o);
  }
  const unsigned char *d = r.data;
  unsigned gdnum = u32(d + 8);
  d += 12 + 10 * gdnum;
  std::set<int> used; int chans = 0; long notes = 0, pbs = 0; long cc[7] = {0};
  for (int ch = 0; ch < 16; ch++) {
    unsigned nn = u32(d); d += 4;
    if (!nn) continue;
    chans++; notes += nn; d += 5 * nn;
    unsigned pc = u32(d); d += 4;
    int prog = 0;
    if (pc == 0) used.insert(0);
    for (unsigned i = 0; i < pc; i++) { prog = (prog + d[3 * pc + i]) & 0xff; used.insert(prog); }
    d += 4 * pc;
    unsigned pb = u32(d); d += 4; pbs += pb; d += 5 * pb;
    for (int k = 0; k < 7; k++) { unsigned c = u32(d); d += 4; cc[k] += c; d += 4 * c; }
  }
  unsigned gs = u32(d); d += 4; const unsigned char *g = d; d += gs;
  unsigned ps = u32(d); d += 4; const unsigned char *pm = d; d += ps;
  int np = ps ? (int)(u32(pm) / 4) : 0;
  unsigned sp = (size_t)(d - r.data) + 4 <= r.size ? u32(d) : 0;
  printf("{\"version\":%d,\"gdnum\":%u,\"chans\":%d,\"notes\":%ld,\"pb\":%ld,\"cc\":[%ld,%ld,%ld,%ld,%ld,%ld,%ld],"
         "\"speech\":%u,\"globals\":[",
         r.version, gdnum, chans, notes, pbs, cc[0], cc[1], cc[2], cc[3], cc[4], cc[5], cc[6], sp < 8192 ? sp : 0);
  for (unsigned i = 0; i < gs; i++) printf("%s%d", i ? "," : "", g[i]);
  printf("],\"patches\":[");
  bool first = true;
  for (int p : used) {
    if (p >= np) continue;
    const unsigned char *q = pm + u32(pm + 4 * p);
    printf("%s{\"n\":%d,\"p\":[", first ? "" : ",", p);
    first = false;
    for (int i = 0; i < kNumPatchParms; i++) printf("%s%d", i ? "," : "", q[i]);
    int mn = q[kNumPatchParms];
    printf("],\"m\":[");
    for (int i = 0; i < mn; i++) printf("%s[%d,%d,%d]", i ? "," : "", q[90 + 3 * i], q[91 + 3 * i], q[92 + 3 * i]);
    printf("]}");
  }
  printf("]}\n");
  return 0;
}
