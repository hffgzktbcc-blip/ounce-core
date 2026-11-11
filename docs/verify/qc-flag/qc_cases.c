/* FPSR.QC differential: each case clears FPSR, runs one saturating op, reads FPSR back.
   Host (Apple Silicon) and Ounce-Core must print identical lines. */
#include <stdio.h>
#include <stdint.h>
#include <arm_neon.h>
static uint32_t s=0x12345678u;
static uint32_t pick(void){s^=s<<13;s^=s>>17;s^=s<<5;static const uint32_t e[]={0,1,0xffffffffu,0x80008000u,0x7fff7fffu,0x80000000u,0x7fffffffu,0x00010001u,0xffff0000u};
  uint32_t w=s; if((s&7)==0) w=e[(s>>8)%9]; return w;}
static void v128(uint8_t*b){for(int i=0;i<4;i++){uint32_t w=pick();b[4*i]=w;b[4*i+1]=w>>8;b[4*i+2]=w>>16;b[4*i+3]=w>>24;}}
static void put(FILE*o,const char*n,int i,uint8x16_t r,uint64_t q){uint8_t b[16];vst1q_u8(b,r);fprintf(o,"%s %d qc=%d ",n,i,(int)((q>>27)&1));for(int k=15;k>=0;k--)fprintf(o,"%02x",b[k]);fputc('\n',o);}
#define OP(NAME,INS,TS,TN) \
  static uint64_t NAME(uint8x16_t*d,uint8x16_t n,uint8x16_t m){uint64_t q; \
    __asm__ volatile("msr fpsr, xzr\n" INS " %0." TS ", %2." TN ", %3." TN "\n mrs %1, fpsr":"+w"(*d),"=r"(q):"w"(n),"w"(m)); return q;}
OP(sqdmull_4h,"sqdmull","4s","4h") OP(sqdmull_2s,"sqdmull","2d","2s")
OP(sqdmull2_8h,"sqdmull2","4s","8h") OP(sqdmull2_4s,"sqdmull2","2d","4s")
OP(sqdmlal_4h,"sqdmlal","4s","4h") OP(sqdmlsl_4h,"sqdmlsl","4s","4h")
OP(sqdmlal_2s,"sqdmlal","2d","2s") OP(sqdmlsl_2s,"sqdmlsl","2d","2s")
/* sticky: a saturating op, then a non-saturating op (zeros), then read; then clear and read again */
static void sticky(FILE*o){uint8x16_t d=vdupq_n_u8(0),z=vdupq_n_u8(0),big=vreinterpretq_u8_u16(vdupq_n_u16(0x8000));uint64_t a,c;
  __asm__ volatile("msr fpsr, xzr\n sqdmull %0.4s, %3.4h, %3.4h\n sqdmull %0.4s, %4.4h, %4.4h\n mrs %1, fpsr\n msr fpsr, xzr\n mrs %2, fpsr"
    :"+w"(d),"=r"(a),"=r"(c):"w"(big),"w"(z));
  fprintf(o,"sticky after_sat_then_nonsat=%d after_clear=%d\n",(int)((a>>27)&1),(int)((c>>27)&1));}
int qc_run_cases(FILE*o){
  for(int i=0;i<300;i++){
    uint8_t a[16],b[16],c[16];v128(a);v128(b);v128(c);
    uint8x16_t n=vld1q_u8(a),m=vld1q_u8(b),acc=vld1q_u8(c),d;uint64_t q;
    d=vdupq_n_u8(0);q=sqdmull_4h(&d,n,m);put(o,"sqdmull.4h",i,d,q);
    d=vdupq_n_u8(0);q=sqdmull_2s(&d,n,m);put(o,"sqdmull.2s",i,d,q);
    d=vdupq_n_u8(0);q=sqdmull2_8h(&d,n,m);put(o,"sqdmull2.8h",i,d,q);
    d=vdupq_n_u8(0);q=sqdmull2_4s(&d,n,m);put(o,"sqdmull2.4s",i,d,q);
    d=acc;q=sqdmlal_4h(&d,n,m);put(o,"sqdmlal.4h",i,d,q);
    d=acc;q=sqdmlsl_4h(&d,n,m);put(o,"sqdmlsl.4h",i,d,q);
    d=acc;q=sqdmlal_2s(&d,n,m);put(o,"sqdmlal.2s",i,d,q);
    d=acc;q=sqdmlsl_2s(&d,n,m);put(o,"sqdmlsl.2s",i,d,q);
  }
  sticky(o);
  /* FPSR[31:28] (N,Z,C,V) exist only when FEAT_AA32 is implemented (AArch32 FPSCR flags); otherwise they are RES0.
     Neoverse implements AArch32 and keeps them, Apple Silicon does not and reads 0, so both are architectural (issue #257).
     Mask them: this checks QC and the cumulative bits, which every AArch64 CPU defines. */
  { uint64_t v; __asm__ volatile("msr fpsr, %1\n mrs %0, fpsr":"=r"(v):"r"((uint64_t)0xffffffffu)); fprintf(o,"fpsr write ffffffff reads %08x\n",(unsigned)(v&0x0fffffffu));
    __asm__ volatile("msr fpsr, xzr"); }
  { uint64_t v; __asm__ volatile("mrs %0, fpcr":"=r"(v)); uint64_t w=v|0x01000000u; __asm__ volatile("msr fpcr, %0"::"r"(w)); uint64_t r; __asm__ volatile("mrs %0, fpcr":"=r"(r));
    fprintf(o,"fpcr roundtrip bit24=%d\n",(int)((r>>24)&1)); __asm__ volatile("msr fpcr, %0"::"r"(v)); }
  return 0;}
