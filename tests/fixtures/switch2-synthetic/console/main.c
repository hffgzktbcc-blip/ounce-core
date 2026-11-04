// Synthetic harness program (self-made, NOT outside homebrew). See ../README.md.
// Prints known text on libnx's CPU console and queues three frames, one per consoleUpdate.
#include <stdio.h>
#include <switch.h>

int main(int argc, char *argv[]) {
    consoleInit(NULL);
    printf("\x1b[2;2Hounce-core switch2 harness: synthetic");
    for (int frame = 1; frame <= 3 && appletMainLoop(); frame++) {
        printf("\x1b[%d;2Hframe %d", 3 + frame, frame);
        consoleUpdate(NULL);
    }
    consoleExit(NULL);
    return 0;
}
