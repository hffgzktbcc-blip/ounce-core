// Synthetic harness program (self-made, NOT outside homebrew). See ../README.md.
// Draws three known frames into a 1280x720 RGBA8888 libnx framebuffer: a fill and two rectangles,
// the second moving 16 px right per frame, so the oracle also checks frame order.
#include <switch.h>

#define W 1280
#define H 720
#define RGBA(r, g, b) ((u32)(r) | ((u32)(g) << 8) | ((u32)(b) << 16) | 0xFF000000u)

static void rect(u32 *px, u32 stride, int x, int y, int w, int h, u32 c) {
    for (int row = y; row < y + h; row++)
        for (int col = x; col < x + w; col++)
            px[row * stride + col] = c;
}

int main(int argc, char *argv[]) {
    Framebuffer fb;
    framebufferCreate(&fb, nwindowGetDefault(), W, H, PIXEL_FORMAT_RGBA_8888, 2);
    framebufferMakeLinear(&fb);
    for (int frame = 0; frame < 3 && appletMainLoop(); frame++) {
        u32 stride;
        u32 *px = (u32 *)framebufferBegin(&fb, &stride);
        stride /= sizeof(u32);
        rect(px, stride, 0, 0, W, H, RGBA(0x20, 0x30, 0x40));
        rect(px, stride, 100, 100, 200, 120, RGBA(0xFF, 0x80, 0x00));
        rect(px, stride, 400 + 16 * frame, 300, 64, 64, RGBA(0x00, 0xC0, 0xFF));
        framebufferEnd(&fb);
    }
    framebufferClose(&fb);
    return 0;
}
