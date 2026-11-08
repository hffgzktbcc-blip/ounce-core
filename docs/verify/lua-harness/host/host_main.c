#include <stdio.h>
int lua_run_cases(FILE*o);
int main(void){return lua_run_cases(stdout)?1:0;}
