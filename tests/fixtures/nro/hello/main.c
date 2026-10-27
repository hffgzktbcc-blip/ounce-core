// Homebrew loader fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Uses no services: it checks what the loader and crt0 set up and returns 0xEEE when all of it holds.
#include <switch.h>
#include <stdlib.h>
#include <string.h>

// No sm/applet/hid/fs: the fixture only exercises loading, relocation and crt0.
void __appInit(void) {}
void __appExit(void) {}

static int ctor_ran;
__attribute__((constructor)) static void fixture_ctor(void) { ctor_ran = 0x5A; }

// Pointers in .data: each one needs an R_AARCH64_RELATIVE relocation.
static const char word_a[] = "ounce";
static const char word_b[] = "core";
static const char *const words[] = { word_a, word_b };

static u64 zeroed[64]; // .bss

static __thread int tls_counter = 7; // .tdata

int main(int argc, char **argv)
{
    u32 fails = 0;
    if (ctor_ran != 0x5A) fails |= 1 << 0;
    if (strcmp(words[0], "ounce") || strcmp(words[1], "core")) fails |= 1 << 1;
    for (int i = 0; i < 64; i++)
        if (zeroed[i]) fails |= 1 << 2;
    if (argc != 2 || strcmp(argv[1], "fixture")) fails |= 1 << 3;

    void *p = malloc(4096); // newlib heap from the loader's OverrideHeap
    if (!p) fails |= 1 << 4;
    memset(p, 0xAB, 4096);
    free(p);

    tls_counter += 1;
    if (tls_counter != 8) fails |= 1 << 5;

    return fails ? (int)(0xBAD0000 | fails) : 0xEEE;
}
