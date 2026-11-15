/* blake3_neon.c includes <arm_neon.h> unconditionally; BLAKE3's own build only compiles it on AArch64.
 * The manifest copies it as blake3_neon.inc so a host build on another architecture still links. */
#if defined(__aarch64__) || defined(_M_ARM64)
#include "blake3_neon.inc"
#endif
