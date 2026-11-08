#include <stdio.h>
#include <switch.h>
int lz4_run_cases(FILE*o);
int main(int argc,char**argv){
  consoleInit(NULL);
  FILE*f=fopen("sdmc:/lz4-results.txt","w");
  int fails=lz4_run_cases(f?f:stdout);
  if(f)fclose(f);
  lz4_run_cases(stdout);
  printf("LZ4 harness done fails=%d\n",fails);
  consoleUpdate(NULL);
  return fails?1:0;
}
