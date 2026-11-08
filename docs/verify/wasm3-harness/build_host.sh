#!/bin/bash
W=src/source
cc -O2 -I$W -o /tmp/w3_ref host/host_main.c wasm3_cases.c $(ls $W/m3_*.c | grep -vE 'api_|m3_deterministic')
