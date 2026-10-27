// libnx nvdrv fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Opens nvdrv the way libnx graphics code does (transfer memory, Initialize, cloned session) and
// creates an nvmap buffer object. Reports through svcOutputDebugString:
// "ounce-nvdrv result=0x00000EEE" when every step worked, else 0xBAD0000 | failed steps, after an
// "rc=" line with the first failing result code.
#include <switch.h>
#include <malloc.h>

void __appInit(void) {}
void __appExit(void) {}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-nvdrv ";
    int n = 12;
    while (*tag)
        msg[n++] = *tag++;
    msg[n++] = '=';
    msg[n++] = '0';
    msg[n++] = 'x';
    for (int i = 7; i >= 0; i--)
        msg[n++] = "0123456789ABCDEF"[(value >> (4 * i)) & 0xF];
    msg[n++] = '\n';
    svcOutputDebugString(msg, n);
}

int main(int argc, char **argv)
{
    u32 fails = 0;
    Result first_rc = 0;
#define STEP(bit, expr) do { Result rc_ = (expr); if (R_FAILED(rc_)) { fails |= 1 << (bit); if (!first_rc) first_rc = rc_; } } while (0)

    STEP(0, smInitialize());
    if (!fails)
        STEP(1, nvInitialize());
    if (!fails)
        STEP(2, nvMapInit());
    if (!fails) {
        NvMap map;
        void *mem = memalign(0x1000, 0x10000);
        STEP(3, nvMapCreate(&map, mem, 0x10000, 0x1000, NvKind_Pitch, true));
        if (!(fails & 8)) {
            // The handle names the object in this process; the ID is how other processes refer to it.
            if (!nvMapGetHandle(&map) || !nvMapGetId(&map) || nvMapGetSize(&map) != 0x10000)
                fails |= 1 << 4;
            nvMapClose(&map);
        }
        free(mem);
        nvMapExit();
    }
    if (!(fails & 3))
        nvExit();
    if (!(fails & 1))
        smExit();

    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
