# Builds services.nro with devkitA64 + libnx. Needs the switch-dev package (see ../hello/README.md).
DEVKITPRO ?= /opt/devkitpro
export DEVKITPRO
PREFIX    := $(DEVKITPRO)/devkitA64/bin/aarch64-none-elf-
ARCH      := -march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE
CFLAGS    := -O2 -Wall -Werror -ffunction-sections $(ARCH) -D__SWITCH__ -I$(DEVKITPRO)/libnx/include
LDFLAGS   := -specs=$(DEVKITPRO)/libnx/switch.specs $(ARCH) -L$(DEVKITPRO)/libnx/lib

services.nro: services.elf
	$(DEVKITPRO)/tools/bin/elf2nro $< $@

services.elf: main.o
	$(PREFIX)gcc $(LDFLAGS) $^ -lnx -o $@

main.o: main.c
	$(PREFIX)gcc $(CFLAGS) -c $< -o $@

all: services.nro

clean:
	rm -f main.o services.elf services.nro

.PHONY: all clean
