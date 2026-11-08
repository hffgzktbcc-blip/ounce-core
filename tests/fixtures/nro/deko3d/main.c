// libnx + deko3d "clear + one triangle" fixture for ounce-core. Built with devkitA64, libnx and deko3d
// (see Makefile). Clears a 256x256 pitch-linear RGBA8 render target to dark blue, draws one orange
// triangle with shaders compiled by uam, waits for the queue and reads pixels back on the CPU.
// Reports through svcOutputDebugString: "ounce-deko3d corner=0x........ center=0x........" and
// "ounce-deko3d result=0x00000EEE" when the corner holds the clear color and the center the triangle
// color, else 0xBAD0000 | failed checks.
#include <switch.h>
#include <deko3d.h>
#include <string.h>
#include "tri_vsh.h"
#include "tri_fsh.h"

void __appInit(void) {}
void __appExit(void) {}

#define W 256
#define H 256
#define CLEAR_RGBA 0xFF400000u  // R=0, G=0, B=0x40, A=0xFF as little-endian RGBA8 bytes
#define TRI_RGBA   0xFF0080FFu  // R=0xFF, G=0x80, B=0, A=0xFF

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-deko3d ";
    int n = 13;
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

// A DKSH file is its control section followed by the code (deko3d's shader container).
typedef struct { u32 magic, header_sz, control_sz, code_sz, programs_off, num_programs; } DkshHeader;

static void load_shader(DkShader *sh, DkMemBlock code, u32 *code_off, const unsigned char *dksh)
{
    const DkshHeader *hdr = (const DkshHeader *)dksh;
    memcpy((u8 *)dkMemBlockGetCpuAddr(code) + *code_off, dksh + hdr->control_sz, hdr->code_sz);
    DkShaderMaker maker;
    dkShaderMakerDefaults(&maker, code, *code_off);
    maker.control = dksh;
    dkShaderInitialize(sh, &maker);
    *code_off = (*code_off + hdr->code_sz + DK_SHADER_CODE_ALIGNMENT - 1) & ~(DK_SHADER_CODE_ALIGNMENT - 1);
}

int main(int argc, char **argv)
{
    u32 fails = 0;
    // deko3d opens nvdrv through sm:, which libnx's default __appInit would have initialized.
    if (R_FAILED(smInitialize())) {
        report("result", 0xBAD0000 | 1 << 4);
        return 0;
    }

    DkDeviceMaker dev_maker;
    dkDeviceMakerDefaults(&dev_maker);
    DkDevice dev = dkDeviceCreate(&dev_maker);

    // Memory: shader code, command buffer + vertex data, and the render target (CPU-visible).
    DkMemBlockMaker mm;
    dkMemBlockMakerDefaults(&mm, dev, 0x10000);
    mm.flags = DkMemBlockFlags_CpuUncached | DkMemBlockFlags_GpuCached | DkMemBlockFlags_Code;
    DkMemBlock code = dkMemBlockCreate(&mm);
    dkMemBlockMakerDefaults(&mm, dev, 0x10000);
    DkMemBlock data = dkMemBlockCreate(&mm);

    DkImageLayoutMaker lm;
    dkImageLayoutMakerDefaults(&lm, dev);
    lm.flags = DkImageFlags_UsageRender | DkImageFlags_PitchLinear;
    lm.format = DkImageFormat_RGBA8_Unorm;
    lm.dimensions[0] = W;
    lm.dimensions[1] = H;
    lm.pitchStride = W * 4;
    DkImageLayout layout;
    dkImageLayoutInitialize(&layout, &lm);
    u32 img_size = (dkImageLayoutGetSize(&layout) + DK_MEMBLOCK_ALIGNMENT - 1) & ~(DK_MEMBLOCK_ALIGNMENT - 1);
    dkMemBlockMakerDefaults(&mm, dev, img_size);
    mm.flags = DkMemBlockFlags_CpuUncached | DkMemBlockFlags_GpuCached | DkMemBlockFlags_Image;
    DkMemBlock img_mem = dkMemBlockCreate(&mm);
    DkImage image;
    dkImageInitialize(&image, &layout, img_mem, 0);
    DkImageView view;
    dkImageViewDefaults(&view, &image);

    DkShader shaders[2];
    u32 code_off = 0;
    load_shader(&shaders[0], code, &code_off, tri_vsh_dksh);
    load_shader(&shaders[1], code, &code_off, tri_fsh_dksh);
    const DkShader *shader_list[] = { &shaders[0], &shaders[1] };

    // Vertex data after the command buffer space: one triangle covering the center.
    const u32 vtx_off = 0x8000;
    static const float verts[] = { -0.75f, -0.75f, 0.75f, -0.75f, 0.0f, 0.75f };
    memcpy((u8 *)dkMemBlockGetCpuAddr(data) + vtx_off, verts, sizeof(verts));

    DkCmdBufMaker cm;
    dkCmdBufMakerDefaults(&cm, dev);
    DkCmdBuf cmd = dkCmdBufCreate(&cm);
    dkCmdBufAddMemory(cmd, data, 0, vtx_off);

    DkQueueMaker qm;
    dkQueueMakerDefaults(&qm, dev);
    qm.flags = DkQueueFlags_Graphics;
    DkQueue queue = dkQueueCreate(&qm);

    const DkImageView *targets[] = { &view };
    dkCmdBufBindRenderTargets(cmd, targets, 1, NULL);
    DkViewport vp = { 0.0f, 0.0f, W, H, 0.0f, 1.0f };
    dkCmdBufSetViewports(cmd, 0, &vp, 1);
    DkScissor sc = { 0, 0, W, H };
    dkCmdBufSetScissors(cmd, 0, &sc, 1);
    dkCmdBufClearColorFloat(cmd, 0, DkColorMask_RGBA, 0.0f, 0.0f, 0.25f, 1.0f);
    dkCmdBufBindShaders(cmd, DkStageFlag_GraphicsMask, shader_list, 2);
    DkRasterizerState rs;
    dkRasterizerStateDefaults(&rs);
    rs.cullMode = DkFace_None;
    dkCmdBufBindRasterizerState(cmd, &rs);
    DkColorState cs;
    dkColorStateDefaults(&cs);
    dkCmdBufBindColorState(cmd, &cs);
    DkColorWriteState cws;
    dkColorWriteStateDefaults(&cws);
    dkCmdBufBindColorWriteState(cmd, &cws);
    DkBufExtents vb = { dkMemBlockGetGpuAddr(data) + vtx_off, sizeof(verts) };
    dkCmdBufBindVtxBuffers(cmd, 0, &vb, 1);
    DkVtxAttribState attr = { .bufferId = 0, .isFixed = 0, .offset = 0, .size = DkVtxAttribSize_2x32, .type = DkVtxAttribType_Float };
    dkCmdBufBindVtxAttribState(cmd, &attr, 1);
    DkVtxBufferState vbs = { 2 * sizeof(float), 0 };
    dkCmdBufBindVtxBufferState(cmd, &vbs, 1);
    dkCmdBufDraw(cmd, DkPrimitive_Triangles, 3, 1, 0, 0);

    dkQueueSubmitCommands(queue, dkCmdBufFinishList(cmd));
    dkQueueWaitIdle(queue);

    const u32 *px = (const u32 *)dkMemBlockGetCpuAddr(img_mem);
    u32 corner = px[2 * W + 2];
    u32 center = px[(H / 2) * W + W / 2];
    if (corner != CLEAR_RGBA)
        fails |= 1 << 0;
    if (center != TRI_RGBA)
        fails |= 1 << 1;

    dkQueueDestroy(queue);
    dkCmdBufDestroy(cmd);
    dkMemBlockDestroy(img_mem);
    dkMemBlockDestroy(data);
    dkMemBlockDestroy(code);
    dkDeviceDestroy(dev);
    smExit();

    report("corner", corner);
    report("center", center);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
