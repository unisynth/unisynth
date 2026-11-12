"""Carve a song of Mentor's 4k synth out of an unpacked intro (change add-receptor-import) into the music.asm +
music.h pair that `synth-convert elevated` reads. Ours, MIT; the dump comes from crinkler-unpack.py and stays local.

  python3 tools/elevated-parity/carve.py <song> <unpacked.bin> <out-dir>      songs: receptor

The data addresses are the build's (read from its generateMusic and synth machine; research/elevated-synth.md §12).
Checks: the tree walks to its terminator with one track left under the 8-machine table, every sequence index names a
pattern, and the build's constants (step, rows) sit where its code reads them."""
import struct
import sys

# song -> image base of the dump, tree, sequence, patterns, step (MAX_NOTE_SAMPLES), rows (NUM_ROWS),
# and code addresses holding step and rows*16 as immediates (cross-checks)
BUILDS = {
    "receptor": dict(base=0x410000, tree=0x422154, seq=0x4218C8, pat=0x422B10, step=4010, rows=133,
                     step_at=0x420739, rows16_at=0x420729),
}
SIZE = [72, 12, 32, 12, 8, 8, 8, 12]  # synth, delay, filter, compressor, mixer, sine, atan, allpass


def main(song, dump, out):
    b = BUILDS[song]
    img = open(dump, "rb").read() + bytes(0x10000)
    at = lambda a, n: img[a - b["base"]:a - b["base"] + n]
    u32 = lambda a: struct.unpack("<I", at(a, 4))[0]
    assert u32(b["step_at"]) == b["step"], "the step is not where the build's synth reads it"
    assert u32(b["rows16_at"]) == b["rows"] * 16, "NUM_ROWS*16 is not where the build's synth reads it"
    # walk the tree
    p, op, depth, synths = b["tree"], 0, 0, 0
    while True:
        depth += {0: 1, 4: -1}.get(op, 0)
        synths += op == 0
        assert depth >= 1, f"stack underflow at {p:#x}"
        p += SIZE[op]
        nxt = at(p, 1)[0]
        if nxt >= 128:
            break
        assert nxt < 8, f"unknown opcode {nxt} at {p:#x}"
        op, p = nxt, p + 1
    assert depth == 1, f"{depth} tracks left"
    tree = at(b["tree"], p + 1 - b["tree"])
    seq = at(b["seq"], synths * b["rows"])
    npat = max(seq) + 1
    pat = at(b["pat"], 16 * npat)
    assert b["seq"] + len(seq) <= b["tree"] or b["seq"] >= b["tree"] + len(tree), "sequence overlaps the tree"

    def db(label, data, per=16):
        rows = [f"{label}:"]
        for i in range(0, len(data), per):
            rows.append("\tdb " + ", ".join(str(x) for x in data[i:i + per]))
        return "\n".join(rows)

    asm = "\n\n".join([
        f"; {song}: carved from the unpacked intro by tools/elevated-parity/carve.py",
        f"_tempo_data:\n\tdd {b['step']}\n\tdb {b['rows']}\n\tdd -1",
        db("_pattern_data", pat),
        db("_sequence_data", seq, b["rows"]),
        db("_machine_tree", tree),
    ]) + "\n"
    total = (b["rows"] * 16 * b["step"] + 65535) & ~0xFFFF
    hdr = "\n".join([
        f"static const int NUM_ROWS equ {b['rows']};",
        f"static const int MAX_NOTE_SAMPLES equ {b['step']};",
        f"static const int TOTAL_SAMPLES equ {total};",
        "static const int MAX_DELAY_SAMPLES equ 65536;",
        "static const int MAX_STACK_HEIGHT equ 4;",
    ]) + "\n"
    open(f"{out}/music.asm", "w").write(asm)
    open(f"{out}/music.h", "w").write(hdr)
    print(f"{out}: {synths} synths, {npat} patterns, tree {len(tree)} B, {total} frames")


if __name__ == "__main__":
    if len(sys.argv) != 4 or sys.argv[1] not in BUILDS:
        sys.exit(__doc__)
    main(*sys.argv[1:])
