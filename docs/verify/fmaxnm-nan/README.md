# Scalar FMAX/FMIN/FMAXNM/FMINNM NaN handling

Every pair of 15 single and 14 double edge values (zeros, infinities, quiet and signalling NaNs with payloads,
denormals) through FMAX, FMIN, FMAXNM, FMINNM and FADD (inline asm, so the compiler cannot fold them).

Before the fix, FMAXNM/FMINNM returned the number when the other operand was a signalling NaN (240 mismatches;
FMAX/FMIN/FADD already matched). Now Tier-3, Tier-4 and native are byte-identical to the Apple Silicon host (2105 lines).
Run with OUNCE_NRO_BUDGET=4000000000. Not covered: the vector forms (see vector-fp).
