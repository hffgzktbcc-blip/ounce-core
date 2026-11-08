#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include "zlib.h"
static uint32_t fnv(const unsigned char*p,unsigned long n){uint32_t h=2166136261u;while(n-->0){h^=*p++;h*=16777619u;}return h;}
static uint32_t st=12345;static unsigned rnd(void){st=st*1664525u+1013904223u;return st>>8;}
#define IN 70000
static unsigned char in[IN],cmp[IN+4096],out[IN],small[64];

static void fill(int k,int n){
  for(int i=0;i<n;i++)switch(k){
    case 2:in[i]="the quick brown fox jumps over the lazy dog. "[i%45];break;
    case 3:in[i]=0;break;
    case 4:in[i]=(unsigned char)(rnd()&0x0f);break;
    case 5:in[i]=(unsigned char)(i%7==0?rnd():i&0xff);break;
    default:in[i]=(unsigned char)rnd();}
}
/* one-shot deflate with explicit parameters; wbits selects zlib/raw/gzip */
static int do_deflate(const unsigned char*src,unsigned n,unsigned char*dst,unsigned cap,
                      int level,int wbits,int mem,int strat,unsigned long*clen){
  z_stream s;memset(&s,0,sizeof s);
  int r=deflateInit2(&s,level,Z_DEFLATED,wbits,mem,strat);if(r!=Z_OK)return r;
  s.next_in=(Bytef*)src;s.avail_in=n;s.next_out=dst;s.avail_out=cap;
  r=deflate(&s,Z_FINISH);*clen=s.total_out;deflateEnd(&s);return r;
}
static int do_inflate(const unsigned char*src,unsigned n,unsigned char*dst,unsigned cap,
                      int wbits,unsigned long*olen){
  z_stream s;memset(&s,0,sizeof s);
  int r=inflateInit2(&s,wbits);if(r!=Z_OK)return r;
  s.next_in=(Bytef*)src;s.avail_in=n;s.next_out=dst;s.avail_out=cap;
  r=inflate(&s,Z_FINISH);*olen=s.total_out;inflateEnd(&s);return r;
}
int zlib_run_cases(FILE*o){
  int fails=0;unsigned long cl,ol;
  fprintf(o,"zlib %s\n",zlibVersion());
  static const char*sn[]={"default","filtered","huffman","rle","fixed"};
  static const int strat[]={Z_DEFAULT_STRATEGY,Z_FILTERED,Z_HUFFMAN_ONLY,Z_RLE,Z_FIXED};
  static const int lv[]={0,1,6,9};
  for(int k=0;k<6;k++){
    int n=(k==0)?0:(k==1)?1:(k==2)?5000:(k==3)?20000:(k==4)?30000:IN;
    st=12345+k;fill(k,n);
    fprintf(o,"input%d n=%d crc32=%08lx adler32=%08lx\n",k,n,
      (unsigned long)crc32(crc32(0L,Z_NULL,0),in,n),(unsigned long)adler32(adler32(0L,Z_NULL,0),in,n));
    for(int li=0;li<4;li++)for(int si=0;si<5;si++){
      if(si>0&&li!=2&&li!=1)continue; /* all strategies at levels 1 and 6 */
      int r=do_deflate(in,n,cmp,sizeof cmp,lv[li],15,8,strat[si],&cl);
      int r2=do_inflate(cmp,cl,out,sizeof out,15,&ol);
      int ok=r==Z_STREAM_END&&r2==Z_STREAM_END&&ol==(unsigned long)n&&memcmp(in,out,n)==0;
      if(!ok)fails++;
      fprintf(o,"  k%d L%d %s c=%lu hash=%08x r=%d r2=%d ok=%d\n",k,lv[li],sn[si],cl,fnv(cmp,cl),r,r2,ok);
    }
    /* raw, gzip and tiny-window/memLevel variants */
    int r=do_deflate(in,n,cmp,sizeof cmp,6,-15,9,Z_DEFAULT_STRATEGY,&cl);
    int r2=do_inflate(cmp,cl,out,sizeof out,-15,&ol);
    int ok=r==Z_STREAM_END&&r2==Z_STREAM_END&&ol==(unsigned long)n&&memcmp(in,out,n)==0;if(!ok)fails++;
    fprintf(o,"  raw c=%lu hash=%08x ok=%d\n",cl,fnv(cmp,cl),ok);
    r=do_deflate(in,n,cmp,sizeof cmp,6,15+16,8,Z_DEFAULT_STRATEGY,&cl);
    if(cl>9)cmp[9]=3; /* gzip header OS byte is host dependent (macOS writes 19); pin it */
    r2=do_inflate(cmp,cl,out,sizeof out,15+32,&ol); /* auto-detect */
    ok=r==Z_STREAM_END&&r2==Z_STREAM_END&&ol==(unsigned long)n&&memcmp(in,out,n)==0;if(!ok)fails++;
    fprintf(o,"  gzip c=%lu hash=%08x ok=%d\n",cl,fnv(cmp,cl),ok);
    r=do_deflate(in,n,cmp,sizeof cmp,9,9,1,Z_DEFAULT_STRATEGY,&cl);
    r2=do_inflate(cmp,cl,out,sizeof out,9,&ol);
    ok=r==Z_STREAM_END&&r2==Z_STREAM_END&&ol==(unsigned long)n&&memcmp(in,out,n)==0;if(!ok)fails++;
    fprintf(o,"  w9m1 c=%lu hash=%08x ok=%d\n",cl,fnv(cmp,cl),ok);
    /* compress2 / uncompress convenience API, and compressBound */
    uLongf dl=sizeof cmp;int cr=compress2(cmp,&dl,in,n,5);uLongf ul=sizeof out;int ur=uncompress(out,&ul,cmp,dl);
    ok=cr==Z_OK&&ur==Z_OK&&ul==(uLongf)n&&memcmp(in,out,n)==0;if(!ok)fails++;
    fprintf(o,"  compress2 c=%lu hash=%08x bound=%lu ok=%d\n",(unsigned long)dl,fnv(cmp,dl),(unsigned long)compressBound(n),ok);
  }
  /* crc32 / adler32 incremental and combine */
  st=777;fill(1,50000);
  {
    uint32_t a=crc32(0,Z_NULL,0),b=crc32(0,Z_NULL,0),c=crc32(0,Z_NULL,0);
    a=crc32(a,in,20000);b=crc32(b,in+20000,30000);c=crc32(c,in,50000);
    uint32_t comb=crc32_combine(a,b,30000);
    uint32_t aa=adler32(1,in,20000),ab=adler32(1,in+20000,30000),ac=adler32(1,in,50000);
    uint32_t acomb=adler32_combine(aa,ab,30000);
    int ok=comb==c&&acomb==ac;if(!ok)fails++;
    fprintf(o,"combine crc=%08x/%08x adler=%08x/%08x ok=%d\n",comb,c,acomb,ac,ok);
    uint32_t d=crc32(0,Z_NULL,0);for(int i=0;i<1000;i++)d=crc32(d,in+i,1);
    uint32_t e=crc32(0,in,1000);
    fprintf(o,"crc_bytewise=%08x bulk=%08x\n",d,e);if(d!=e)fails++;
    const z_crc_t*t=get_crc_table();
    fprintf(o,"crc_table=%08lx %08lx %08lx %08lx\n",(unsigned long)t[0],(unsigned long)t[1],(unsigned long)t[128],(unsigned long)t[255]);
  }
  /* error paths on a text stream */
  st=1;fill(2,5000);
  do_deflate(in,5000,cmp,sizeof cmp,6,15,8,Z_DEFAULT_STRATEGY,&cl);
  {
    unsigned long good=cl;static unsigned char keep[4096];memcpy(keep,cmp,good);
    int r=do_inflate(cmp,good/2,out,sizeof out,15,&ol);
    fprintf(o,"truncated r=%d out=%lu\n",r,ol);if(r==Z_STREAM_END)fails++;
    for(int t=0;t<4;t++){
      memcpy(cmp,keep,good);unsigned pos=(good/5)*(t+1);cmp[pos]^=0x5a;
      r=do_inflate(cmp,good,out,sizeof out,15,&ol);
      fprintf(o,"corrupt@%u r=%d out=%lu\n",pos,r,ol);
    }
    memcpy(cmp,keep,good);
    r=do_inflate(cmp,good,small,sizeof small,15,&ol);
    fprintf(o,"smallout_inflate r=%d out=%lu\n",r,ol);if(r!=Z_BUF_ERROR)fails++;
    r=do_deflate(in,5000,small,sizeof small,6,15,8,Z_DEFAULT_STRATEGY,&ol);
    fprintf(o,"smallout_deflate r=%d out=%lu\n",r,ol);if(r!=Z_OK)fails++;
    uLongf dl=8;r=compress(small,&dl,in,5000);
    fprintf(o,"compress_small r=%d\n",r);if(r!=Z_BUF_ERROR)fails++;
    memcpy(cmp,keep,good);cmp[0]=0x00;cmp[1]=0x00;
    r=do_inflate(cmp,good,out,sizeof out,15,&ol);
    fprintf(o,"badheader r=%d\n",r);if(r!=Z_DATA_ERROR)fails++;
    memcpy(cmp,keep,good);cmp[good-1]^=1; /* adler trailer */
    r=do_inflate(cmp,good,out,sizeof out,15,&ol);
    fprintf(o,"badtrailer r=%d\n",r);if(r!=Z_DATA_ERROR)fails++;
    /* dictionary */
    static const char dict[]="the quick brown fox jumps over the lazy dog. ";
    z_stream s;memset(&s,0,sizeof s);deflateInit(&s,6);
    deflateSetDictionary(&s,(const Bytef*)dict,sizeof dict-1);
    s.next_in=in;s.avail_in=5000;s.next_out=cmp;s.avail_out=sizeof cmp;
    r=deflate(&s,Z_FINISH);cl=s.total_out;deflateEnd(&s);
    fprintf(o,"dict_deflate r=%d c=%lu hash=%08x\n",r,cl,fnv(cmp,cl));
    memset(&s,0,sizeof s);inflateInit(&s);
    s.next_in=cmp;s.avail_in=cl;s.next_out=out;s.avail_out=sizeof out;
    r=inflate(&s,Z_FINISH);int r1=r;
    int rs=Z_OK;
    if(r==Z_NEED_DICT){
      rs=inflateSetDictionary(&s,(const Bytef*)"wrong dictionary text",21);
      fprintf(o,"wrongdict set=%d\n",rs);if(rs!=Z_DATA_ERROR)fails++;
      rs=inflateSetDictionary(&s,(const Bytef*)dict,sizeof dict-1);
      r=inflate(&s,Z_FINISH);
    }
    int ok=r1==Z_NEED_DICT&&rs==Z_OK&&r==Z_STREAM_END&&s.total_out==5000&&memcmp(in,out,5000)==0;
    fprintf(o,"dict_inflate first=%d set=%d r=%d adler=%08lx ok=%d\n",r1,rs,r,(unsigned long)s.adler,ok);
    inflateEnd(&s);if(!ok)fails++;
    r=do_inflate(cmp,cl,out,sizeof out,15,&ol);
    fprintf(o,"nodict r=%d\n",r);if(r!=Z_NEED_DICT)fails++;
    /* streaming in small chunks with sync flushes */
    memset(&s,0,sizeof s);deflateInit(&s,6);
    unsigned pos=0;
    s.next_out=cmp;s.avail_out=sizeof cmp;
    while(pos<5000){unsigned ch=(pos%97)+1;if(pos+ch>5000)ch=5000-pos;
      s.next_in=in+pos;s.avail_in=ch;pos+=ch;deflate(&s,pos>=5000?Z_FINISH:Z_SYNC_FLUSH);}
    cl=s.total_out;deflateEnd(&s);
    r=do_inflate(cmp,cl,out,sizeof out,15,&ol);
    ok=r==Z_STREAM_END&&ol==5000&&memcmp(in,out,5000)==0;if(!ok)fails++;
    fprintf(o,"stream_sync c=%lu hash=%08x ok=%d\n",cl,fnv(cmp,cl),ok);
  }
  fprintf(o,"fails=%d\n",fails);
  return fails;
}
