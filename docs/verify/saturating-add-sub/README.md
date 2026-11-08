# SQADD/UQADD/SQSUB/UQSUB and SMOV

300 operand sets (random plus edge values) through the saturating add and subtract forms: vector 8/16/32/64-bit lanes
(128-bit) plus 64-bit 8x8 and 4x16 forms, and scalar b/h/s/d forms (inline asm so the compiler cannot reorder around the
FPSR clear and read). Each case prints the result and QC. 7800 lines, 2202 with qc=0, the rest saturate.

Found because wasm3 (see ../wasm3-harness) executes scalar `UQSUB h31, h30, h31`; and the compiler's `SMOV x3, v31.b[0]`
(sign-extending lane move) was also undecoded. Both added here.

Result: Tier-3, Tier-4 and native are byte-identical to the Apple Silicon host. Run with OUNCE_NRO_BUDGET=4000000000.
