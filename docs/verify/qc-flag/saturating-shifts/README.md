# QC flag for saturating narrows and shifts

300 operand sets x ~20 saturating ops (SQXTN/UQXTN/SQXTUN, SQSHRN/UQSHRN/SQSHRUN and rounding forms,
SQSHL/UQSHL/SQSHLU immediate, plus scalar forms). Each op clears FPSR, runs, and prints the result and QC bit.

Result: Tier-3, Tier-4 and native outputs are byte-identical to the Apple Silicon host (5700 lines, 5684 with qc=1).
Run with OUNCE_NRO_BUDGET=4000000000. Not covered: cumulating forms (SQADD etc. are in qc-flag/), FPSR.QC via FP converts.
