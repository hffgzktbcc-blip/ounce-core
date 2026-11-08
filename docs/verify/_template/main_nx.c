#include <stdio.h>
#include <switch.h>
int run_cases(FILE *o);
int main(int argc, char **argv) {
    consoleInit(NULL);
    FILE *f = fopen("sdmc:/results.txt", "w");   /* --sdmc=DIR on the command line maps this to DIR */
    int fails = run_cases(f ? f : stdout);
    if (f) fclose(f);
    consoleUpdate(NULL);
    return fails ? 1 : 0;
}
