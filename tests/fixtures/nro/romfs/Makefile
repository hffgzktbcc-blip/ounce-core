# Builds romfs.nro with devkitA64 + libnx. Needs the switch-dev package (see ../hello/README.md).
DEVKITPRO ?= /opt/devkitpro
export DEVKITPRO
PREFIX    := $(DEVKITPRO)/devkitA64/bin/aarch64-none-elf-
ARCH      := -march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE
CFLAGS    := -O2 -Wall -Werror -ffunction-sections $(ARCH) -D__SWITCH__ -I$(DEVKITPRO)/libnx/include
LDFLAGS   := -specs=$(DEVKITPRO)/libnx/switch.specs $(ARCH) -L$(DEVKITPRO)/libnx/lib

romfs.nro: romfs.elf romfs/greeting.txt add_romfs.py
	$(DEVKITPRO)/tools/bin/elf2nro $< $@
	$(DEVKITPRO)/tools/bin/build_romfs romfs romfs.bin
	python3 add_romfs.py $@ romfs.bin

romfs.elf: main.o
	$(PREFIX)gcc $(LDFLAGS) $^ -lnx -o $@

main.o: main.c
	$(PREFIX)gcc $(CFLAGS) -c $< -o $@

all: romfs.nro

clean:
	rm -f main.o romfs.elf romfs.nro romfs.bin

.PHONY: all clean
