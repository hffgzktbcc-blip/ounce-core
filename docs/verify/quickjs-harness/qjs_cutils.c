/* Wrapper so the upstream cutils.c (staged as cutils.inc, to keep build globs of *.c from compiling it twice) builds with the version string and feature macros the real build passes on the command line. */
#define _GNU_SOURCE
#define CONFIG_VERSION "2026-06-04"
#include "cutils.inc"
