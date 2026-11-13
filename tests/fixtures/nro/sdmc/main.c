// libnx SD card fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Mounts sdmc:/ and drives it through newlib stdio/POSIX calls (libnx fsdev), the way homebrew does.
// The test runs it with a host folder behind sdmc:/ that holds host.txt ("from host\n").
// Reports through svcOutputDebugString: "ounce-sdmc result=0x00000EEE" when every step worked, else
// 0xBAD0000 | failed steps, after an "rc=" line with the first failing service result code.
#include <switch.h>
#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

void __appInit(void) {}
void __appExit(void) {}

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-sdmc ";
    int n = 11;
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

static int read_all(const char *path, char *buf, int cap)
{
    FILE *f = fopen(path, "r");
    if (!f)
        return -1;
    int n = fread(buf, 1, cap - 1, f);
    fclose(f);
    buf[n < 0 ? 0 : n] = 0;
    return n;
}

int main(int argc, char **argv)
{
    u32 fails = 0;
    Result first_rc = 0;
#define STEP(bit, expr) do { Result rc_ = (expr); if (R_FAILED(rc_)) { fails |= 1 << (bit); if (!first_rc) first_rc = rc_; } } while (0)
#define CHECK(bit, cond) do { if (!(cond)) fails |= 1 << (bit); } while (0)

    STEP(0, smInitialize());
    if (!fails)
        STEP(1, fsInitialize());
    if (!fails)
        STEP(2, fsdevMountSdmc());
    if (!fails) {
        char buf[64];
        struct stat st;

        CHECK(3, mkdir("sdmc:/ounce", 0777) == 0);

        FILE *f = fopen("sdmc:/ounce/a.txt", "w");
        CHECK(4, f && fwrite("hello sdmc\n", 1, 11, f) == 11);
        if (f)
            fclose(f);

        CHECK(5, stat("sdmc:/ounce/a.txt", &st) == 0 && S_ISREG(st.st_mode) && st.st_size == 11);

        f = fopen("sdmc:/ounce/a.txt", "a");
        CHECK(6, f && fwrite("more", 1, 4, f) == 4);
        if (f)
            fclose(f);
        CHECK(6, read_all("sdmc:/ounce/a.txt", buf, sizeof(buf)) == 15 && strcmp(buf, "hello sdmc\nmore") == 0);

        CHECK(7, rename("sdmc:/ounce/a.txt", "sdmc:/ounce/b.txt") == 0);
        CHECK(7, stat("sdmc:/ounce/a.txt", &st) != 0 && errno == ENOENT);
        CHECK(7, stat("sdmc:/ounce/b.txt", &st) == 0 && st.st_size == 15);

        DIR *d = opendir("sdmc:/ounce");
        int seen = 0;
        if (d) {
            struct dirent *e;
            while ((e = readdir(d)))
                seen += strcmp(e->d_name, "b.txt") == 0 && e->d_type == DT_REG ? 1 : 100;
            closedir(d);
        }
        CHECK(8, seen == 1);

        // A path above the SD card root must not open (and must not create anything on the host).
        f = fopen("sdmc:/../escape.txt", "w");
        CHECK(9, f == NULL);
        if (f)
            fclose(f);

        CHECK(10, remove("sdmc:/ounce/b.txt") == 0);
        CHECK(10, rmdir("sdmc:/ounce") == 0);
        CHECK(10, stat("sdmc:/ounce", &st) != 0);

        CHECK(11, read_all("sdmc:/host.txt", buf, sizeof(buf)) == 10 && strcmp(buf, "from host\n") == 0);

        fsdevUnmountAll();
    }
    if (!(fails & 3))
        fsExit();
    if (!(fails & 1))
        smExit();

    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
