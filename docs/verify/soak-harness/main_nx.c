#include <stdio.h>
#include <stdlib.h>
#include <switch.h>
int soak_run(FILE*o,int iters);
int main(int argc,char**argv){consoleInit(NULL);int n=100;FILE*i=fopen("sdmc:/iters.txt","r");if(i){if(fscanf(i,"%d",&n)!=1)n=100;fclose(i);}FILE*f=fopen("sdmc:/results.txt","w");soak_run(f?f:stdout,n);if(f)fclose(f);consoleUpdate(NULL);return 0;}
