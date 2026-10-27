// libnx filesystem + RomFS fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Mounts the SD card and its own RomFS the way NRO homebrew does (fsp-srv SD card filesystem, then the
// .nro named by argv[0]) and reads romfs:/greeting.txt. Reports through svcOutputDebugString:
// "ounce-romfs result=0x00000EEE" when every step worked, else 0xBAD0000 | failed steps, after an
// "rc=" line with the first failing result code.
#include <switch.h>
#include <fcntl.h>
#include <string.h>
#include <unistd.h>

void __appInit(void) {}
void __appExit(void) {}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-romfs ";
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
        STEP(1, fsInitialize());
    if (!fails)
        STEP(2, fsdevMountSdmc());
    if (!fails)
        STEP(3, romfsInit());
    if (!fails) {
        char buf[64] = {0};
        int fd = open("romfs:/greeting.txt", O_RDONLY);
        int n = fd >= 0 ? read(fd, buf, sizeof(buf) - 1) : -1;
        if (fd >= 0)
            close(fd);
        if (n != 17 || strcmp(buf, "hello from romfs\n") != 0)
            fails |= 1 << 4;
        romfsExit();
    }
    if (!(fails & 7))
        fsdevUnmountAll();
    if (!(fails & 3))
        fsExit();
    if (!(fails & 1))
        smExit();

    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
