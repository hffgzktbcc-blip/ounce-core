// libnx GPU channel fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Brings up the GPU the way libnx graphics code does (nvhost-ctrl, nvhost-ctrl-gpu, an address space,
// a GPFIFO channel), maps a command buffer, submits it and waits for the channel's fence.
// Reports through svcOutputDebugString: "ounce-gpu result=0x00000EEE" when every step worked, else
// 0xBAD0000 | failed steps, after an "rc=" line with the first failing result code.
#include <switch.h>
#include <malloc.h>
#include <string.h>

void __appInit(void) {}
void __appExit(void) {}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-gpu ";
    int n = 10;
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

// Command buffer contents: one increasing-method header writing two words to subchannel 0, method
// 0x40 (a harmless scratch range in the 3D class), so the kernel model sees a real GPFIFO entry.
static const u32 commands[] = { 0x20020040, 0x11111111, 0x22222222 };

int main(int argc, char **argv)
{
    u32 fails = 0;
    Result first_rc = 0;
#define STEP(bit, expr) do { if (!fails) { Result rc_ = (expr); if (R_FAILED(rc_)) { fails |= 1 << (bit); first_rc = rc_; } } } while (0)

    NvAddressSpace as;
    NvGpuChannel ch;
    NvMap cmdmap;
    void *mem = memalign(0x1000, 0x1000);
    iova_t iova = 0;
    NvFence fence;
    u32 opened = 0;

    STEP(0, smInitialize());
    STEP(1, nvInitialize());
    STEP(2, nvMapInit());
    STEP(3, nvFenceInit());
    STEP(4, nvGpuInit());
    STEP(5, nvAddressSpaceCreate(&as, 0x10000));
    if (!fails) opened = 1;
    STEP(6, nvGpuChannelCreate(&ch, &as, NvChannelPriority_Medium));
    if (!fails) opened = 2;
    STEP(7, nvMapCreate(&cmdmap, mem, 0x1000, 0x1000, NvKind_Pitch, true));
    if (!fails) opened = 3;
    STEP(8, nvAddressSpaceMap(&as, nvMapGetHandle(&cmdmap), true, NvKind_Pitch, &iova));
    if (!fails) {
        memcpy(mem, commands, sizeof(commands));
        armDCacheFlush(mem, sizeof(commands));
    }
    STEP(9, nvGpuChannelAppendEntry(&ch, iova, sizeof(commands) / 4, 0, 0));
    STEP(10, nvGpuChannelKickoff(&ch));
    if (!fails) nvGpuChannelGetFence(&ch, &fence);
    STEP(11, nvFenceWait(&fence, -1));

    if (opened >= 3) {
        if (iova) nvAddressSpaceUnmap(&as, iova);
        nvMapClose(&cmdmap);
    }
    if (opened >= 2) nvGpuChannelClose(&ch);
    if (opened >= 1) nvAddressSpaceClose(&as);
    free(mem);

    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
