#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "src/webp/encode.h"
#include "src/webp/decode.h"
static uint64_t fnv(const uint8_t*p,size_t n,uint64_t h){for(size_t i=0;i<n;i++){h^=p[i];h*=1099511628211ull;}return h;}
int main(void){
  int W[]={1,7,16,33,64,97,128}, H[]={1,5,16,31,64,50,96};
  int cases=0;
  for(int s=0;s<7;s++){ int w=W[s],h=H[s]; uint8_t*rgba=malloc(w*h*4);
    uint32_t r=12345+s; for(int y=0;y<h;y++)for(int x=0;x<w;x++){ r=r*1664525+1013904223; uint8_t*p=rgba+(y*w+x)*4; p[0]=(x*255/(w>1?w-1:1)); p[1]=(y*255/(h>1?h-1:1)); p[2]=(uint8_t)((x*y)^(r>>24)>>(s&3)); p[3]=(s%3==0)?(uint8_t)(r>>16):255; if(((x/8+y/8)&1)) p[2]^=0x55; }
    for(int mode=0;mode<8;mode++){ uint8_t*out=NULL; size_t sz;
      float q=(mode==0)?5:(mode==1)?50:(mode==2)?90:100;
      switch(mode){case 0: case 1: case 2: sz=WebPEncodeRGBA(rgba,w,h,w*4,q,&out);break; case 3: sz=WebPEncodeLosslessRGBA(rgba,w,h,w*4,&out);break;
       default:{ WebPConfig c; WebPPicture pic; WebPConfigInit(&c); WebPPictureInit(&pic); pic.width=w;pic.height=h; pic.use_argb=(mode>=6); WebPPictureImportRGBA(&pic,rgba,w*4);
         c.quality=(mode==4)?75:(mode==5)?30:(mode==6)?60:100; c.method=(mode==7)?6:4; c.lossless=(mode>=6); c.sns_strength=(mode==5)?100:50; c.segments=(mode==4)?4:2; c.filter_strength=(mode==4)?60:20; c.use_sharp_yuv=(mode==5); c.preprocessing=(mode==4)?2:0; c.partitions=(mode==5)?3:0; c.alpha_quality=80;
         WebPMemoryWriter wr; WebPMemoryWriterInit(&wr); pic.writer=WebPMemoryWrite; pic.custom_ptr=&wr; int ok=WebPEncode(&c,&pic); WebPPictureFree(&pic); out=wr.mem; sz=ok?wr.size:0; }}
      uint64_t hd=fnv(out,sz,1469598103934665603ull); int dw,dh; uint8_t*dec=WebPDecodeRGBA(out,sz,&dw,&dh); uint64_t hp=dec?fnv(dec,(size_t)dw*dh*4,1469598103934665603ull):0;
      // also decode via YUV and bilinear upsample path
      printf("%d.%d %dx%d enc=%zu %016llx dec=%dx%d %016llx\n",s,mode,w,h,sz,(unsigned long long)hd,dw,dh,(unsigned long long)hp); cases++;
      WebPFree(dec); WebPFree(out); }
    free(rgba); }
  printf("cases=%d\n",cases); return 0; }
