#include <stdio.h>
#include <switch.h>
int tinyexpr_run_cases(FILE*o, FILE*p);
int main(int argc,char**argv){
  consoleInit(NULL);
  FILE*f=fopen("sdmc:/results.txt","w");
  FILE*p=fopen("sdmc:/precise.txt","w");
  int fails=tinyexpr_run_cases(f?f:stdout, p?p:stdout);
  if(f)fclose(f);
  if(p)fclose(p);
  printf("TinyExpr harness done fails=%d\n",fails);
  consoleUpdate(NULL);
  return fails?1:0;
}
