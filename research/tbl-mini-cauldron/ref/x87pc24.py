"""x87 arithmetic with precision control = 24 bits (FPU control word 0x007F).

TBL's intros run `fninit` then clear the PC bits, so every x87 add/sub/mul/div
result is rounded ONCE to a 24-bit significand (round-to-nearest-even), while
the exponent keeps the extended range.  Loads (fld/fild) and stores of qword
values are exact.  fistp rounds to nearest-even.

Values here are Python floats (IEEE f64) that already hold a 24-bit
significand (or an exact f64 constant from the patch data).  Each op computes
the exact result as an unevaluated sum hi+lo (error-free transformation) and
rounds that once to 24 bits, so there is no double rounding.
"""
import math

_TWO24 = float(1 << 24)


def _round24(hi, lo=0.0):
    """Round the exact value hi+lo (|lo| <= ulp64(hi)/2) to a 24-bit significand."""
    if hi == 0.0 or not math.isfinite(hi):
        return hi + lo
    m, e = math.frexp(hi)            # hi = m * 2**e, 0.5 <= |m| < 1
    s = m * _TWO24                   # exact (f64 holds 53 bits)
    f = math.floor(s)
    frac = s - f
    if frac == 0.5 and lo != 0.0:    # hi sits exactly on an f32 midpoint: lo decides
        q = f + 1 if lo > 0 else f
    else:
        q = round(s)                 # ties-to-even when exactly halfway
    return math.ldexp(q, e - 24)


def _two_sum(a, b):
    s = a + b
    bb = s - a
    err = (a - (s - bb)) + (b - bb)
    return s, err


def add(a, b):
    return _round24(*_two_sum(a, b))


def sub(a, b):
    return _round24(*_two_sum(a, -b))


def mul(a, b):
    p = a * b
    return _round24(p, math.fma(a, b, -p))


def div(a, b):
    # f64 quotient then 24-bit rounding is innocuous (53 >= 2*24+2).
    return _round24(a / b)


def fistp(x):
    """fistp with RC=nearest: ties-to-even integer."""
    return int(round(x))
