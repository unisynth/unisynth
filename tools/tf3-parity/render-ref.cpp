// MIT harness (unisynth, change add-tf3-import): renders a Tunefish 3 song with the original synth and player, built
// outside the repository by build-ref.sh. Input is the plain text written by render-ref.ts (parsed project, values
// already quantised in player mode); output is a 16-bit stereo 44.1 kHz WAV of the player's own i16 output.
//   render-ref <song.txt> <out.wav> <tail seconds>
// Song text: "bpm B", "instrument ID v0 … v128", "pattern ID ROWS TRACKS", "ev PATTERN TRACK ROW NOTE INSTR VEL EFFECT",
// "instance ROWOFFSET TRACK PATTERN". TF3_TICK sets the tick count at load (default 1000).
#include "system/system.hpp"
#include "math/math.hpp"
#define private public  // drive the player's block step directly instead of its audio thread
#include "synth/tunefish3.hpp"
#undef private

#include <cstdio>
#include <fstream>
#include <sstream>

eU32 g_tf3Seed = 1;
eU32 g_tf3Tick = 1000;

// A sound output that accepts exactly one block per player step and keeps the samples.
class Capture : public tfISoundOut {
public:
  std::vector<eS16> data;
  bool full = false;
  eBool initialize(eU32, eU32) override { return true; }
  void shutdown() override {}
  void play() override {}
  void stop() override {}
  void clear() override {}
  void fill(const eU8 *d, eU32 count) override {
    const eS16 *s = (const eS16 *)d;
    data.insert(data.end(), s, s + count / 2);
    full = true;
  }
  eBool isFilled() const override { return full; }
  eU32 getSampleRate() const override { return 44100; }
};

static void writeWav(const char *path, const std::vector<eS16> &d) {
  FILE *f = std::fopen(path, "wb");
  if (!f) { std::perror(path); std::exit(1); }
  const eU32 bytes = (eU32)(d.size() * 2), rate = 44100, byteRate = rate * 4, fmtLen = 16, riff = 36 + bytes;
  const eU16 pcm = 1, ch = 2, align = 4, bits = 16;
  std::fwrite("RIFF", 1, 4, f); std::fwrite(&riff, 4, 1, f); std::fwrite("WAVEfmt ", 1, 8, f);
  std::fwrite(&fmtLen, 4, 1, f); std::fwrite(&pcm, 2, 1, f); std::fwrite(&ch, 2, 1, f); std::fwrite(&rate, 4, 1, f);
  std::fwrite(&byteRate, 4, 1, f); std::fwrite(&align, 2, 1, f); std::fwrite(&bits, 2, 1, f);
  std::fwrite("data", 1, 4, f); std::fwrite(&bytes, 4, 1, f); std::fwrite(d.data(), 2, d.size(), f);
  std::fclose(f);
}

int main(int argc, char **argv) {
  if (argc < 4) { std::fprintf(stderr, "usage: render-ref <song.txt> <out.wav> <tail seconds>\n"); return 2; }
  std::ifstream in(argv[1]);
  if (!in) { std::perror(argv[1]); return 1; }
  const float tail = std::strtof(argv[3], nullptr);

  // Never destroyed: the source's pattern destructor calls its track arrays' destructors explicitly, which assumes the
  // original array type; the process exits right after writing the WAV.
  // TF3_TICK: the tick count at load (the original seeds its PAD phases and note phases from it), for measuring how
  // much the original varies between runs
  if (const char *t = std::getenv("TF3_TICK")) g_tf3Tick = (eU32)std::strtoul(t, nullptr, 10);
  const eU32 tick0 = g_tf3Tick;
  Capture out;
  tfPlayer &player = *new tfPlayer(&out);
  player.setLoop(0, 0);
  tfSong &song = *new tfSong;
  std::vector<tfSong::Pattern *> patterns;
  std::string line;
  for (int n = 1; std::getline(in, line); n++) {
    std::istringstream ls(line);
    std::string kind;
    ls >> kind;
    if (kind.empty() || kind[0] == '#') continue;
    if (kind == "bpm") { eU32 b; ls >> b; song.setBpm(b); }
    else if (kind == "instrument") {
      eU32 id; ls >> id;
      if (id >= TF_MAX_INPUTS) { std::fprintf(stderr, "line %d: instrument %u out of range\n", n, id); return 1; }
      player.addInstrument(id);
      // strtof, not >>: presets hold subnormal values (5e-42), which a stream refuses as out of range
      for (eU32 i = 0; i < TF_PARAM_COUNT; i++) {
        std::string tok;
        if (!(ls >> tok)) break;
        player.getInstrument(id)->setParam(i, std::strtof(tok.c_str(), nullptr));
      }
      if (!ls) { std::fprintf(stderr, "line %d: instrument needs %d values\n", n, TF_PARAM_COUNT); return 1; }
      player.getInstrument(id)->updateAddSynth();
    } else if (kind == "pattern") {
      eU32 id, rows, tracks; ls >> id >> rows >> tracks;
      patterns.push_back(&song.newPattern(rows, tracks, (eInt)id));
    } else if (kind == "ev") {
      eU32 p, t, r; eInt note, instr, vel, fx; ls >> p >> t >> r >> note >> instr >> vel >> fx;
      tfSong::Pattern *pat = nullptr;
      for (auto *q : patterns) if (q->getPatternNumber() == p) pat = q;
      if (!pat || t >= pat->getTrackCount() || r >= pat->getRowCount()) { std::fprintf(stderr, "line %d: bad event\n", n); return 1; }
      tfSong::NoteEvent &ev = pat->getTrack(t)[r];
      ev.noteOct = (eU8)note; ev.instrument = (eS8)instr; ev.velocity = (eS8)vel; ev.effect = (eU16)fx;
    } else if (kind == "instance") {
      eU32 off, t, p; ls >> off >> t >> p;
      tfSong::Pattern *pat = nullptr;
      for (auto *q : patterns) if (q->getPatternNumber() == p) pat = q;
      if (!pat) { std::fprintf(stderr, "line %d: missing pattern %u\n", n, p); return 1; }
      song.newPatternInstance(*pat, off, t);
    } else { std::fprintf(stderr, "line %d: unknown record %s\n", n, kind.c_str()); return 1; }
  }

  player.setSong(&song);
  player.play(0.0f);
  const double songSecs = song.getLengthInRows() * 15.0 / song.getBpm();
  const eU32 blocks = (eU32)((songSecs + tail) * 44100.0 / TF_BLOCKSIZE) + 1;
  bool stopped = false;
  for (eU32 b = 0; b < blocks; b++) {
    const double t = (double)b * TF_BLOCKSIZE / 44100.0;
    // no loop: stop where the player's rounded row reaches the length (half a row before length·15/bpm, where it
    // would wrap to row 0 and fire it again); the tail rings out
    const eF32 ft = (eF32)(b * TF_BLOCKSIZE) / 44100.0f;
    if (!stopped && (eU32)eFtoL(ft * (eF32)song.getBpm() / 60.0f * 4.0f) >= song.getLengthInRows()) { player.stop(); stopped = true; }
    g_tf3Tick = tick0 + (eU32)(t * 1000.0);
    out.full = false;
    player._processAudio();
  }
  writeWav(argv[2], out.data);
  return 0;
}
