#include <stdio.h>
#include <switch.h>
int intvec_run_cases(FILE*o);
int main(int argc,char**argv){consoleInit(NULL);FILE*f=fopen("sdmc:/results.txt","w");intvec_run_cases(f?f:stdout);if(f)fclose(f);consoleUpdate(NULL);return 0;}
