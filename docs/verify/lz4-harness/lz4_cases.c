#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include "lz4.h"
static uint32_t fnv(const unsigned char*p,int n){uint32_t h=2166136261u;while(n-->0){h^=*p++;h*=16777619u;}return h;}
static uint32_t st=12345;static unsigned rnd(void){st=st*1664525u+1013904223u;return st>>8;}
/* prints deterministic result lines; returns number of failed self-checks */
int lz4_run_cases(FILE*o){
  int fails=0; static unsigned char in[65536],cmp[80000],out[65536];
  for(int k=0;k<6;k++){
    int n=(k==0)?0:(k==1)?1:(k==2)?1000:(k==3)?4096:(k==4)?30000:65000;
    for(int i=0;i<n;i++){
      switch(k){case 2:in[i]="the quick brown fox "[i%20];break;
        case 3:in[i]=0;break;
        case 4:in[i]=(unsigned char)(rnd()&0x0f);break;
        case 5:in[i]=(unsigned char)(i%7==0?rnd():i&0xff);break;
        default:in[i]=(unsigned char)rnd();}
    }
    int c=LZ4_compress_default((const char*)in,(char*)cmp,n,sizeof cmp);
    int d=LZ4_decompress_safe((const char*)cmp,(char*)out,c,sizeof out);
    int ok=(c>0||n==0)&&d==n&&memcmp(in,out,n)==0;
    if(!ok)fails++;
    fprintf(o,"case%d n=%d c=%d hash=%08x d=%d ok=%d\n",k,n,c,fnv(cmp,c>0?c:0),d,ok);
  }
  /* insufficient output buffer on compress */
  int c=LZ4_compress_default((const char*)in,(char*)cmp,4096,8);
  fprintf(o,"smallbuf_compress=%d\n",c); if(c!=0)fails++;
  /* insufficient output buffer on decompress */
  memset(in,'a',4096); c=LZ4_compress_default((const char*)in,(char*)cmp,4096,sizeof cmp);
  int d=LZ4_decompress_safe((const char*)cmp,(char*)out,c,100);
  fprintf(o,"smallbuf_decompress=%d\n",d); if(d>=0)fails++;
  /* corrupt inputs: truncation and bit flips must not crash and must be rejected or bounded */
  for(int t=0;t<8;t++){
    unsigned char bad[512]; int bn=c<512?c:512; memcpy(bad,cmp,bn);
    if(t<4) bn=bn>(t+1)?bn-(t+1):0; else bad[rnd()%bn]^=(unsigned char)(1u<<(t&7));
    d=LZ4_decompress_safe((const char*)bad,(char*)out,bn,sizeof out);
    fprintf(o,"corrupt%d=%d\n",t,d);
  }
  fprintf(o,"fails=%d\n",fails); return fails;
}
