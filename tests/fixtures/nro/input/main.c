// libnx controller input fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Sets up input the way libnx applications do (padConfigureInput, padInitializeDefault) and reads the
// pad a few times with padUpdate. Reports through svcOutputDebugString:
//   "ounce-input buttons=0x........", "ounce-input stick_l=0x....____" (x in the high half, y low),
//   then "ounce-input result=0x00000EEE" when the pad read as connected, else 0xBAD0000 | bits.
#include <switch.h>

void __appInit(void) {}
void __appExit(void) {}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-input ";
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
#define STEP(bit, expr) do { if (!fails) { Result rc_ = (expr); if (R_FAILED(rc_)) { fails |= 1 << (bit); first_rc = rc_; } } } while (0)

    STEP(0, smInitialize());
    STEP(1, hidInitialize());
    if (!fails) {
        padConfigureInput(1, HidNpadStyleSet_NpadStandard);
        PadState pad;
        padInitializeDefault(&pad);
        // A few frames' worth of reads, as a main loop would do.
        for (int i = 0; i < 4; i++) {
            padUpdate(&pad);
            svcSleepThread(1000000);
        }
        if (!padIsConnected(&pad))
            fails |= 1 << 2;
        HidAnalogStickState l = padGetStickPos(&pad, 0);
        report("buttons", (u32)padGetButtons(&pad));
        report("stick_l", ((u32)(u16)l.x << 16) | (u16)l.y);
        hidExit();
    }
    if (!(fails & 1))
        smExit();

    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
