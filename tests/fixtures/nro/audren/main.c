// libnx audio renderer fixture for ounce-core. Built with devkitA64 + libnx (see Makefile).
// Plays a 50 ms 1 kHz tone (mono PCM16, 48 kHz) through libnx's audio driver: audrenInitialize, a memory
// pool, a stereo device sink, one voice mixed to both channels, then audrvUpdate / audrenWaitFrame until
// the wave buffer is done. Reports through svcOutputDebugString: "ounce-audren played=0x........"
// and "ounce-audren result=0x00000EEE" when every step worked, else 0xBAD0000 | failed steps.
// With the argument "long" the tone loops for 2 seconds (400 frames) at 20% volume, to listen to under
// `ounce-core run`.
#include <switch.h>
#include <malloc.h>
#include <string.h>

void __appInit(void) {}
void __appExit(void) {}

#define SAMPLES 2400
#define AMPLITUDE 8000

static void report(const char *tag, u32 value)
{
    char msg[48] = "ounce-audren ";
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

static const AudioRendererConfig cfg = {
    .output_rate = AudioRendererOutputRate_48kHz,
    .num_voices = 4,
    .num_effects = 0,
    .num_sinks = 1,
    .num_mix_objs = 1,
    .num_mix_buffers = 2,
};

int main(int argc, char **argv)
{
    u32 fails = 0, played = 0;
    bool long_mode = argc > 1 && strcmp(argv[1], "long") == 0;
    Result first_rc = 0;
#define STEP(bit, expr) do { if (!fails) { Result rc_ = (expr); if (R_FAILED(rc_)) { fails |= 1 << (bit); first_rc = rc_; } } } while (0)
#define CHECK(bit, cond) do { if (!fails && !(cond)) fails |= 1 << (bit); } while (0)

    // A 1 kHz square-ish tone: 24 samples high, 24 low, so the mixed output is easy to check.
    size_t size = (SAMPLES * sizeof(s16) + AUDREN_MEMPOOL_ALIGNMENT - 1) & ~(AUDREN_MEMPOOL_ALIGNMENT - 1);
    s16 *pcm = memalign(AUDREN_MEMPOOL_ALIGNMENT, size);
    memset(pcm, 0, size);
    for (int i = 0; i < SAMPLES; i++)
        pcm[i] = (i / 24) % 2 ? -AMPLITUDE : AMPLITUDE;
    armDCacheFlush(pcm, size);

    AudioDriver drv;
    AudioDriverWaveBuf wb = { .data_pcm16 = pcm, .size = size, .start_sample_offset = 0, .end_sample_offset = SAMPLES, .is_looping = long_mode };
    static const u8 sink_channels[] = { 0, 1 };

    STEP(0, smInitialize());
    STEP(1, audrenInitialize(&cfg));
    STEP(2, audrvCreate(&drv, &cfg, 2));
    if (!fails) {
        int pool = audrvMemPoolAdd(&drv, pcm, size);
        CHECK(3, pool >= 0 && audrvMemPoolAttach(&drv, pool));
        CHECK(3, audrvDeviceSinkAdd(&drv, AUDREN_DEFAULT_DEVICE_NAME, 2, sink_channels) >= 0);
    }
    STEP(4, audrvUpdate(&drv));
    STEP(4, audrenStartAudioRenderer());
    if (!fails) {
        CHECK(5, audrvVoiceInit(&drv, 0, 1, PcmFormat_Int16, 48000));
        audrvVoiceSetDestinationMix(&drv, 0, AUDREN_FINAL_MIX_ID);
        audrvVoiceSetMixFactor(&drv, 0, 1.0f, 0, 0);
        audrvVoiceSetMixFactor(&drv, 0, 1.0f, 0, 1);
        if (long_mode)
            audrvVoiceSetVolume(&drv, 0, 0.2f); // the listening mode plays at 20% (a full-scale square wave is loud)
        CHECK(5, audrvVoiceAddWaveBuf(&drv, 0, &wb));
        audrvVoiceStart(&drv, 0);
    }
    if (long_mode) {
        for (int i = 0; !fails && i < 400; i++) {
            STEP(6, audrvUpdate(&drv));
            audrenWaitFrame();
        }
        played = audrvVoiceGetPlayedSampleCount(&drv, 0); // stopping a voice clears its counters
        audrvVoiceStop(&drv, 0);
        STEP(6, audrvUpdate(&drv));
    }
    for (int i = 0; !long_mode && !fails && i < 40 && wb.state != AudioDriverWaveBufState_Done; i++) {
        STEP(6, audrvUpdate(&drv));
        audrenWaitFrame();
    }
    if (!fails) {
        if (!long_mode)
            played = audrvVoiceGetPlayedSampleCount(&drv, 0);
        CHECK(7, long_mode || wb.state == AudioDriverWaveBufState_Done);
        CHECK(7, played >= (long_mode ? 399 * 240 : SAMPLES));
        audrvClose(&drv);
        audrenExit();
    }
    if (!(fails & 1))
        smExit();

    report("played", played);
    if (first_rc)
        report("rc", first_rc);
    report("result", fails ? 0xBAD0000 | fails : 0xEEE);
    return 0;
}
