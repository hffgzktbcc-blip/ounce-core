#include <stdio.h>
#include <stdint.h>
#include "wasm3.h"
static const uint8_t WASM[]={0,0x61,0x73,0x6d,1,0,0,0, 1,6,1,0x60,1,0x7f,1,0x7f, 3,2,1,0, 7,7,1,3,'f','i','b',0,0,
 0x0a,0x1e,1,0x1c,0,0x20,0,0x41,2,0x48,0x04,0x7f,0x20,0,5,0x20,0,0x41,1,0x6b,0x10,0,0x20,0,0x41,2,0x6b,0x10,0,0x6a,0x0b,0x0b};
int wasm3_run_cases(FILE*o){
  IM3Environment env=m3_NewEnvironment();if(!env){fprintf(o,"env fail\n");return 1;}
  IM3Runtime rt=m3_NewRuntime(env,64*1024,NULL);if(!rt){fprintf(o,"rt fail\n");return 1;}
  IM3Module mod;M3Result r=m3_ParseModule(env,&mod,WASM,sizeof WASM);if(r){fprintf(o,"parse %s\n",r);return 1;}
  r=m3_LoadModule(rt,mod);if(r){fprintf(o,"load %s\n",r);return 1;}
  IM3Function f;r=m3_FindFunction(&f,rt,"fib");if(r){fprintf(o,"find %s\n",r);return 1;}
  for(int i=0;i<=22;i++){r=m3_CallV(f,(int32_t)i);if(r){fprintf(o,"call %d %s\n",i,r);return 1;}
    int32_t v=0;r=m3_GetResultsV(f,&v);if(r){fprintf(o,"res %s\n",r);return 1;}fprintf(o,"fib %d %d\n",i,v);}
  return 0;}
