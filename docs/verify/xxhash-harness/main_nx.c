#include <stdio.h>
#include <switch.h>
int xxh_run_cases(FILE*o);
int main(int argc,char**argv){
  consoleInit(NULL);
  FILE*f=fopen("sdmc:/xxh-results.txt","w");
  int fails=xxh_run_cases(f?f:stdout);
  if(f)fclose(f);
  printf("xxh harness done fails=%d\n",fails);
  consoleUpdate(NULL);
  return fails?1:0;
}
