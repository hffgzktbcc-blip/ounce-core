# Builds deko3d.nro with devkitA64 + libnx + deko3d (switch-dev). Shaders are compiled from GLSL with
# uam (deko3d's shader compiler) and embedded with xxd -i.
DEVKITPRO ?= /opt/devkitpro
export DEVKITPRO
PREFIX    := $(DEVKITPRO)/devkitA64/bin/aarch64-none-elf-
ARCH      := -march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE
CFLAGS    := -O2 -Wall -Werror -ffunction-sections $(ARCH) -D__SWITCH__ -I$(DEVKITPRO)/libnx/include -I.
LDFLAGS   := -specs=$(DEVKITPRO)/libnx/switch.specs $(ARCH) -L$(DEVKITPRO)/libnx/lib

deko3d.nro: deko3d.elf
	$(DEVKITPRO)/tools/bin/elf2nro $< $@

deko3d.elf: main.o
	$(PREFIX)gcc $(LDFLAGS) $^ -ldeko3d -lnx -o $@

main.o: main.c tri_vsh.h tri_fsh.h
	$(PREFIX)gcc $(CFLAGS) -c $< -o $@

%_vsh.dksh: %_vsh.glsl
	$(DEVKITPRO)/tools/bin/uam -s vert -o $@ $<

%_fsh.dksh: %_fsh.glsl
	$(DEVKITPRO)/tools/bin/uam -s frag -o $@ $<

%.h: %.dksh
	xxd -i $< > $@

all: deko3d.nro

clean:
	rm -f main.o deko3d.elf deko3d.nro *.dksh tri_vsh.h tri_fsh.h

.PHONY: all clean
