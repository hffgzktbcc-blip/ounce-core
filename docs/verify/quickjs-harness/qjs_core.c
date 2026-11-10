/* Wrapper so the upstream quickjs.c (staged as quickjs.inc, to keep build globs of *.c from compiling it twice)
   builds with the version string the real build passes on the command line. */
#define _GNU_SOURCE
#define CONFIG_VERSION "2026-06-04"
#ifdef __SWITCH__
/* devkitA64's newlib has no tm_gmtoff unless __TM_GMTOFF names it, and its localtime_r would not fill it. The guest has
   no time zone database, so treat local time as UTC: enable the field and zero it with a gmtime_r shim. The harness
   scripts only use UTC date functions, so this does not change what is compared. */
#define __TM_GMTOFF tm_gmtoff
#include <malloc.h>   /* newlib declares malloc_usable_size here; upstream's generic fallback branch calls it */
#include <string.h>
#include <time.h>
static struct tm *qjs_localtime_r(const time_t *t, struct tm *r) { memset(r, 0, sizeof *r); return gmtime_r(t, r); }
#define localtime_r qjs_localtime_r
#endif
#include "quickjs.inc"
