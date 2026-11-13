#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
int xxh_run_cases(FILE *o);
int main(void) {
#ifdef _WIN32
    /* Keep pipe bytes identical to Linux ELF stdout: no CRT CRLF translation. */
    if (_setmode(_fileno(stdout), _O_BINARY) == -1) return 2;
#endif
    return xxh_run_cases(stdout) ? 1 : 0;
}
