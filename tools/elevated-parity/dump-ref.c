/* Reference renderer that runs an intro's OWN unpacked synth code (change add-receptor-import). MIT, ours: it maps
 * the user's local dump (crinkler-unpack.py) at its address, calls the build's generateMusic and writes the 16-bit
 * stereo result. Used for builds without a released source (Receptor); nothing of the intro is in the repo.
 *
 *   dump-ref.exe <dump.bin> <out.wav> [--solo k] [--fpu 24|53|64]
 *
 * --fpu: x87 precision control while rendering. The intro calls generateMusic right after IDirect3D9::CreateDevice
 * without D3DCREATE_FPU_PRESERVE, which leaves the FPU at 24-bit precision: the default here. --solo k zeroes the
 * volume of every synth machine except the k-th (tree order), as render-ref.c does. Build constants: receptor. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

#define BASE 0x410000u
#define SPAN 0x10E00000u     /* up to the build's last bss word (0x111F0304) */
#define GENERATE 0x4206CDu   /* generateMusic */
#define TREE 0x422154u       /* machine tree */
#define MUSIC 0xBF0100u      /* the 16-bit result, written over the final float buffer */
#define FRAMES 0x830000u     /* TOTAL_SAMPLES */

/* linked at image base 0x400000 (-Wl,--image-base,0x400000 -Wl,--disable-dynamicbase): .text/.data stay below BASE, this covers the rest */
static unsigned char area[BASE - 0x400000u + SPAN];

static void solo(int keep) {
    static const int size[8] = {72, 12, 32, 12, 8, 8, 8, 12};
    unsigned char *p = (unsigned char *)TREE;
    int op = 0, synth = 0;
    for (;;) {
        if (op == 0) {
            if (synth != keep) memset(p + 28, 0, 4); /* the synth's volume float */
            synth++;
        }
        p += size[op];
        op = *p++;
        if (op >= 128) break;
        if (op >= 8) { fprintf(stderr, "dump-ref: opcode %d at %p\n", op, (void *)(p - 1)); exit(1); }
    }
    if (keep >= synth) { fprintf(stderr, "dump-ref: solo %d but only %d synths\n", keep, synth); exit(1); }
}

static void le(FILE *f, unsigned v, int n) {
    for (int i = 0; i < n; i++) fputc((v >> (8 * i)) & 255, f);
}

int main(int argc, char **argv) {
    const char *dump = NULL, *out = NULL;
    int keep = -1, fpu = 24;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--solo") && i + 1 < argc) keep = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--fpu") && i + 1 < argc) fpu = atoi(argv[++i]);
        else if (!dump) dump = argv[i];
        else out = argv[i];
    }
    if (!dump || !out || (fpu != 24 && fpu != 53 && fpu != 64)) {
        fprintf(stderr, "usage: dump-ref <dump.bin> <out.wav> [--solo k] [--fpu 24|53|64]\n");
        return 2;
    }
    /* the image's own .bss holds the build's address range (the loader reserves it before any heap exists) */
    unsigned char *mem = (unsigned char *)BASE;
    DWORD prot;
    if (mem < area || mem + SPAN > area + sizeof area || !VirtualProtect(mem, SPAN, PAGE_EXECUTE_READWRITE, &prot)) {
        fprintf(stderr, "dump-ref: .bss %p..%p does not cover %#x (+%#x)\n", (void *)area, (void *)(area + sizeof area), BASE, SPAN);
        return 1;
    }
    FILE *f = fopen(dump, "rb");
    if (!f) { perror(dump); return 1; }
    size_t n = fread(mem, 1, SPAN, f);
    fclose(f);
    if (n < GENERATE - BASE) { fprintf(stderr, "dump-ref: %s is too short (%u bytes)\n", dump, (unsigned)n); return 1; }
    if (keep >= 0) solo(keep);
    unsigned short cw = fpu == 24 ? 0x007F : fpu == 53 ? 0x027F : 0x037F, old;
    __asm__ volatile("fnstcw %0\n\tfldcw %1" : "=m"(old) : "m"(cw));
    ((void(__stdcall *)(void))GENERATE)();
    __asm__ volatile("fldcw %0" : : "m"(old));
    f = fopen(out, "wb");
    if (!f) return 1;
    unsigned bytes = FRAMES * 4u;
    fwrite("RIFF", 1, 4, f); le(f, 36 + bytes, 4); fwrite("WAVEfmt ", 1, 8, f);
    le(f, 16, 4); le(f, 1, 2); le(f, 2, 2); le(f, 44100, 4); le(f, 44100 * 4, 4); le(f, 4, 2); le(f, 16, 2);
    fwrite("data", 1, 4, f); le(f, bytes, 4);
    fwrite((void *)MUSIC, 1, bytes, f);
    fclose(f);
    printf("%s: %u frames, fpu %d%s\n", out, FRAMES, fpu, keep >= 0 ? " (solo)" : "");
    return 0;
}
