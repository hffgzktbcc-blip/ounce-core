/* CoreMark's main() is renamed with -Dmain=coremark_main (see differential.json), so this
 * wrapper undefines the macro before defining the real entry point. */
#undef main
#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
int coremark_main(int argc, char *argv[]);
int main(void) {
    char *args[] = {"coremark", 0};
#ifdef _WIN32
    if (_setmode(_fileno(stdout), _O_BINARY) == -1) return 2;
#endif
    return coremark_main(1, args);
}
