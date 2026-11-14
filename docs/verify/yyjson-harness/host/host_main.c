#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
int yyjson_run_cases(FILE *o);
int main(void) {
#ifdef _WIN32
    if (_setmode(_fileno(stdout), _O_BINARY) == -1) return 2;
#endif
    return yyjson_run_cases(stdout) ? 1 : 0;
}
