/* Replace the body of run_cases with calls into the program you are testing.
   Print deterministic text only (no pointers, times or random seeds you did not fix).
   Return the number of failed self-checks. */
#include <stdio.h>
int run_cases(FILE *o) {
    int fails = 0;
    fprintf(o, "hello %d\n", 1 + 1);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
