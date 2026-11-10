#include <stdio.h>
#include <stdlib.h>
int soak_run(FILE*o,int iters);
int main(int c,char**v){return soak_run(stdout,c>1?atoi(v[1]):100);}
