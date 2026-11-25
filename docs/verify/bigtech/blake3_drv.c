#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include "blake3.h"
int main(void){
  static uint8_t buf[1<<17];
  for(size_t i=0;i<sizeof buf;i++) buf[i]=(uint8_t)(i%251);
  uint8_t key[32]; for(int i=0;i<32;i++) key[i]=(uint8_t)(i*7+1);
  size_t lens[400]; int n=0;
  for(size_t l=0;l<=3100;l+= (l<200?1:(l<2200?37:101))) lens[n++]=l;
  size_t big[]={4096,4097,8192,8193,16384,16385,31744,65536,100000,131072};
  for(int i=0;i<10;i++) lens[n++]=big[i];
  for(int i=0;i<n;i++){ for(int mode=0;mode<3;mode++){
    blake3_hasher h; uint8_t out[131];
    if(mode==0) blake3_hasher_init(&h); else if(mode==1) blake3_hasher_init_keyed(&h,key); else blake3_hasher_init_derive_key(&h,"ounce ctx");
    // feed in odd chunk sizes to exercise state
    size_t off=0, step=(lens[i]%97)+1; while(off<lens[i]){ size_t c=lens[i]-off<step?lens[i]-off:step; blake3_hasher_update(&h,buf+off,c); off+=c; step=step*3%1021+1; }
    blake3_hasher_finalize(&h,out,sizeof out);
    printf("%zu %d ",lens[i],mode); for(int k=0;k<131;k++) printf("%02x",out[k]); printf("\n"); } }
  return 0; }
