/* Reference renderer harness for Mentor's 4k synth (change add-elevated-import). MIT, ours: it only provides the
 * buffer the original code renders into, calls the original generateMusic, and writes the 16-bit stereo result.
 *
 *   render-ref.exe <out.wav> [-t 7|8] [--solo k]
 *
 * -t: the build's machine table (8 = synth, delay, filter, compressor, mixer, sine, atan, allpass (Atrium);
 *     7 = the same without atan (Elevated)). --solo k zeroes the volume of every synth machine except the k-th
 *     (0-based, tree order), so the WAV holds that channel after its effects. Built against the song's music.h. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "music_c.h" /* music.h as #defines (build-ref.sh) */

unsigned char MusicBuffer[TOTAL_SAMPLES * 8 * MAX_STACK_HEIGHT];
extern unsigned char machine_tree[] __asm__("_machine_tree");
void __stdcall generateMusic(void);

static void solo(int table, int keep) {
    static const int full[8] = {72, 12, 32, 12, 8, 8, 8, 12};
    static const int elev[7] = {72, 12, 32, 12, 8, 8, 12};
    const int *size = table == 7 ? elev : full;
    unsigned char *p = machine_tree;
    int op = 0, synth = 0;
    for (;;) {
        if (op == 0) {
            if (synth != keep) memset(p + 28, 0, 4); /* the synth's volume float */
            synth++;
        }
        p += size[op];
        op = *p++;
        if (op >= 128) break;
        if (op >= table) {
            fprintf(stderr, "render-ref: opcode %d outside table %d at offset %d\n", op, table, (int)(p - 1 - machine_tree));
            exit(1);
        }
    }
    if (keep >= synth) {
        fprintf(stderr, "render-ref: solo %d but only %d synths\n", keep, synth);
        exit(1);
    }
}

static void le(FILE *f, unsigned v, int n) {
    for (int i = 0; i < n; i++) fputc((v >> (8 * i)) & 255, f);
}

int main(int argc, char **argv) {
    int table = 8, keep = -1;
    const char *out = NULL;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "-t") && i + 1 < argc) table = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--solo") && i + 1 < argc) keep = atoi(argv[++i]);
        else out = argv[i];
    }
    if (!out || (table != 7 && table != 8)) {
        fprintf(stderr, "usage: render-ref <out.wav> [-t 7|8] [--solo k]\n");
        return 2;
    }
    if (keep >= 0) solo(table, keep);
    generateMusic();
    FILE *f = fopen(out, "wb");
    if (!f) return 1;
    unsigned bytes = TOTAL_SAMPLES * 4u;
    fwrite("RIFF", 1, 4, f); le(f, 36 + bytes, 4); fwrite("WAVEfmt ", 1, 8, f);
    le(f, 16, 4); le(f, 1, 2); le(f, 2, 2); le(f, 44100, 4); le(f, 44100 * 4, 4); le(f, 4, 2); le(f, 16, 2);
    fwrite("data", 1, 4, f); le(f, bytes, 4);
    fwrite(MusicBuffer, 1, bytes, f);
    fclose(f);
    printf("%s: %d frames%s\n", out, TOTAL_SAMPLES, keep >= 0 ? " (solo)" : "");
    return 0;
}
