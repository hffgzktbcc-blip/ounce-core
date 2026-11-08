#include <stdio.h>
int tinyexpr_run_cases(FILE*o, FILE*p);
int main(void){
  FILE*p=fopen("precise-host.txt","w");
  int r=tinyexpr_run_cases(stdout,p?p:stderr);
  if(p)fclose(p);
  return r?1:0;
}
