/* Deterministic CoreMark port layer (EEMBC CoreMark, Apache-2.0, is fetched at a pinned commit; this
 * file only implements the timing hooks its core_portme.h declares). There is no clock: the timed
 * region reports zero ticks, so the output is byte-reproducible and CoreMark's own "ERROR! Must
 * execute for at least 10 secs" line is part of the expected output. The meaningful result is the
 * CRC section and "Correct operation validated", which CoreMark computes against fixed constants. */
#include <stdlib.h>
#include "coremark.h"

volatile ee_s32 seed1_volatile = 0x0;
volatile ee_s32 seed2_volatile = 0x0;
volatile ee_s32 seed3_volatile = 0x66;
volatile ee_s32 seed4_volatile = ITERATIONS;
volatile ee_s32 seed5_volatile = 0;

void start_time(void) {}
void stop_time(void) {}
CORE_TICKS get_time(void) { return 0; }
secs_ret time_in_secs(CORE_TICKS ticks) { (void)ticks; return 0; }
ee_u32 default_num_contexts = 1;

void portable_init(core_portable *p, int *argc, char *argv[]) {
    (void)argc;
    (void)argv;
    p->portable_id = 1;
}
void portable_fini(core_portable *p) { p->portable_id = 0; }

/* Only used when MEM_METHOD is MEM_MALLOC; the posix header selects it. */
void *portable_malloc(ee_size_t size) { return malloc(size); }
void portable_free(void *p) { free(p); }
