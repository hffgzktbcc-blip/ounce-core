#include <stdio.h>
#include <switch.h>
int fp_run_cases(FILE*o);
int main(int argc,char**argv){
  consoleInit(NULL);
  FILE*f=fopen("sdmc:/fp-results.txt","w");
  fp_run_cases(f?f:stdout);
  if(f)fclose(f);
  printf("fp cases done\n"); consoleUpdate(NULL); return 0;
}
