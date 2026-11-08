#include <stdio.h>
#include <switch.h>
int zlib_run_cases(FILE*o);
int main(int argc,char**argv){
  consoleInit(NULL);
  FILE*f=fopen("sdmc:/results.txt","w");
  int fails=zlib_run_cases(f?f:stdout);
  if(f)fclose(f);
  printf("zlib harness done fails=%d\n",fails);
  consoleUpdate(NULL);
  return fails?1:0;
}
