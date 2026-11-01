// Project-authored libnx fixture: four npad styles and a touch point.
// Built with devkitA64 + libnx; see README.md.
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
    if (R_FAILED(smInitialize()) || R_FAILED(hidInitialize())) {
        report("result", 0xBAD0001);
        return 0;
    }
    padConfigureInput(8, HidNpadStyleSet_NpadStandard);
    hidInitializeTouchScreen();
    svcSleepThread(1000000);
    HidNpadCommonState state;
#define CHECK_PAD(bit, func, id, expected_buttons, expected_x, expected_y) do { \
    if (func(id, &state, 1) != 1 || state.buttons != (expected_buttons) || \
        !(state.attributes & HidNpadAttribute_IsConnected) || \
        state.analog_stick_l.x != (expected_x) || state.analog_stick_l.y != (expected_y)) fails |= 1u << (bit); \
} while (0)
    CHECK_PAD(1, hidGetNpadStatesFullKey, HidNpadIdType_No1, HidNpadButton_A, 123, -456);
    CHECK_PAD(2, hidGetNpadStatesJoyDual, HidNpadIdType_No2, HidNpadButton_B, 234, -567);
    CHECK_PAD(3, hidGetNpadStatesJoyLeft, HidNpadIdType_No3, HidNpadButton_Left, 345, -678);
    CHECK_PAD(4, hidGetNpadStatesJoyRight, HidNpadIdType_No4, HidNpadButton_X, 456, -789);
    HidTouchScreenState touch;
    if (hidGetTouchScreenStates(&touch, 1) != 1 || touch.count != 1 ||
        touch.touches[0].finger_id != 7 || touch.touches[0].x != 321 ||
        touch.touches[0].y != 123) fails |= 1u << 5;
    hidExit();
    smExit();
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
