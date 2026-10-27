// libnx service handshake fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Opens sm: and hid the way libnx applications do, then reports through svcOutputDebugString:
// "ounce-services result=0x00000EEE" when every step worked, else 0xBAD0000 | failed steps, plus
// the first failing result code.
#include <switch.h>

void __appInit(void) {}
void __appExit(void) {}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-services ";
    int n = 15;
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
    if (!fails) {
        STEP(1, hidInitialize());
        if (!(fails & 2)) {
            // The shared memory is mapped somewhere libnx chose and must be readable.
            volatile u8 *shm = hidGetSharedmemAddr();
            if (!shm)
                fails |= 1 << 2;
            else
                (void)shm[0];
            hidExit();
        }
        smExit();
    }

    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
