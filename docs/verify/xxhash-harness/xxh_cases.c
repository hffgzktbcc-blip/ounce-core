#include <stdio.h>
#include <stdint.h>
#include "xxhash.h"
static uint32_t st=987654321u;static unsigned rnd(void){st=st*1664525u+1013904223u;return st>>8;}
int xxh_run_cases(FILE*o){
  static unsigned char b[5000]; for(int i=0;i<5000;i++)b[i]=(unsigned char)rnd();
  static const int lens[]={0,1,3,4,8,15,16,17,31,32,33,63,64,127,128,129,240,241,255,256,257,1023,1024,4096,5000};
  int fails=0;
  for(unsigned k=0;k<sizeof lens/sizeof*lens;k++){
    int n=lens[k];
    uint32_t h32=XXH32(b,n,0x12345678u);
    uint64_t h64=XXH64(b,n,0x9e3779b97f4a7c15ull);
    uint64_t x3=XXH3_64bits(b,n);
    uint64_t x3s=XXH3_64bits_withSeed(b,n,42);
    XXH128_hash_t h=XXH3_128bits(b,n);
    /* streaming in odd chunks must equal one-shot */
    XXH3_state_t*s=XXH3_createState();XXH3_64bits_reset(s);
    for(int p=0;p<n;){int c=1+(int)(rnd()%97);if(c>n-p)c=n-p;XXH3_64bits_update(s,b+p,c);p+=c;}
    uint64_t sx=XXH3_64bits_digest(s);XXH3_freeState(s);
    int ok=(sx==x3); if(!ok)fails++;
    fprintf(o,"n=%d xxh32=%08x xxh64=%016llx xxh3=%016llx xxh3s=%016llx xxh128=%016llx%016llx stream=%d\n",
      n,h32,(unsigned long long)h64,(unsigned long long)x3,(unsigned long long)x3s,
      (unsigned long long)h.high64,(unsigned long long)h.low64,ok);
  }
  fprintf(o,"fails=%d\n",fails);return fails;
}
