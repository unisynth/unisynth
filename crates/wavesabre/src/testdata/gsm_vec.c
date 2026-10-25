/* Rebuild: P=$(brew --prefix libgsm); cc -I$P/include gsm_vec.c -L$P/lib -lgsm -lm -o /tmp/gsm_vec && /tmp/gsm_vec  (run in this dir; writes gsm_vectors.gsm/.pcm) */
#include <gsm.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define NBLK 16 /* 8 sine+noise, 4 silence, 4 square; 320 samples (2 GSM frames) per block */

int main(void) {
    static short pcm_in[NBLK * 320], pcm_out[NBLK * 320];
    static unsigned char enc[NBLK * 65];
    unsigned s = 1;
    int n;
    for (n = 0; n < NBLK * 320; n++) {
        int blk = n / 320;
        double v;
        if (blk < 8) {
            s = s * 1103515245u + 12345u;
            double noise = ((int)((s >> 16) & 0x7fff) - 16384) * 3000.0 / 16384.0;
            v = 12000.0 * sin(2.0 * 3.14159265358979323846 * 440.0 * n / 8000.0) + noise;
        } else if (blk < 12) {
            v = 0;
        } else {
            v = (n % 100) < 50 ? 32767 : -32767;
        }
        if (v > 32767) v = 32767;
        if (v < -32768) v = -32768;
        pcm_in[n] = (short)lrint(v);
    }
    int one = 1;
    gsm e = gsm_create(), d = gsm_create();
    gsm_option(e, GSM_OPT_WAV49, &one);
    gsm_option(d, GSM_OPT_WAV49, &one);
    int b;
    for (b = 0; b < NBLK; b++) {
        gsm_encode(e, pcm_in + b * 320, enc + b * 65);          /* writes 32 bytes (+ pending nibble) */
        gsm_encode(e, pcm_in + b * 320 + 160, enc + b * 65 + 32); /* writes 33 bytes */
        if (gsm_decode(d, enc + b * 65, pcm_out + b * 320) < 0) return 1;
        if (gsm_decode(d, enc + b * 65 + 33, pcm_out + b * 320 + 160) < 0) return 1;
    }
    FILE *f = fopen("gsm_vectors.gsm", "wb");
    fwrite(enc, 1, sizeof enc, f);
    fclose(f);
    f = fopen("gsm_vectors.pcm", "wb");
    for (n = 0; n < NBLK * 320; n++) {
        unsigned char le[2] = {pcm_out[n] & 0xff, (pcm_out[n] >> 8) & 0xff};
        fwrite(le, 1, 2, f);
    }
    fclose(f);
    return 0;
}
