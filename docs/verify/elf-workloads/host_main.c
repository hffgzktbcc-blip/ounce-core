/* Shared host/ELF entry point for the original elf-workloads (MIT, ounce-core).
 * Each workload provides `int workload_run(FILE *out)`: 0 on success, non-zero on a
 * self-check failure. All output goes to stdout; there is no input, clock or environment. */
#include <stdio.h>
int workload_run(FILE *out);
int main(void) {
    int rc = workload_run(stdout);
    fflush(stdout);
    return rc ? 1 : 0;
}
