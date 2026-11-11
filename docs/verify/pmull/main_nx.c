#include <stdio.h>
#include <switch.h>
int pmull_run(FILE *o);
int main(void) {
    consoleInit(NULL);
    FILE *f = fopen("sdmc:/results.txt", "w");
    pmull_run(f ? f : stdout);
    if (f) fclose(f);
    consoleUpdate(NULL);
    return 0;
}
