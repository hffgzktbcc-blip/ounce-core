#include <stdio.h>
#include <switch.h>
int lua_run_cases(FILE*o);
int main(int argc,char**argv){
  consoleInit(NULL);
  FILE*f=fopen("sdmc:/lua-results.txt","w");
  int fails=lua_run_cases(f?f:stdout);
  if(f)fclose(f);
  printf("lua harness done fails=%d\n",fails);
  consoleUpdate(NULL);
  return fails?1:0;
}
