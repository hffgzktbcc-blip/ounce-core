#include <stdio.h>
#include <stdint.h>
#include <arm_neon.h>
static uint32_t st=77u;static uint32_t rnd(void){st=st*1664525u+1013904223u;return st;}
static const uint32_t EDGE[]={0,1,0xFFFFFFFFu,0x80000000u,0x7FFFFFFFu,0x00008000u,0x00007FFFu,0xFFFF8000u,0x80008000u,0x7FFF7FFFu,0x00FF00FFu,0x01010101u};
static uint32_t pick(void){uint32_t r=rnd();return (r&3)==0?EDGE[(r>>8)%(sizeof EDGE/sizeof*EDGE)]:rnd();}
static void v128(uint8_t*b){for(int i=0;i<4;i++){uint32_t w=pick();b[4*i]=w;b[4*i+1]=w>>8;b[4*i+2]=w>>16;b[4*i+3]=w>>24;}}
static void put(FILE*o,const char*n,int i,uint8x16_t r){uint8_t b[16];vst1q_u8(b,r);fprintf(o,"%s %d ",n,i);for(int k=15;k>=0;k--)fprintf(o,"%02x",b[k]);fputc('\n',o);}
int narrow_run_cases(FILE*o){
  for(int i=0;i<400;i++){
    uint8_t a[16],b[16],c[16];v128(a);v128(b);v128(c);
    uint16x8_t n16=vreinterpretq_u16_u8(vld1q_u8(a)),m16=vreinterpretq_u16_u8(vld1q_u8(b));
    uint32x4_t n32=vreinterpretq_u32_u8(vld1q_u8(a)),m32=vreinterpretq_u32_u8(vld1q_u8(b));
    uint64x2_t n64=vreinterpretq_u64_u8(vld1q_u8(a)),m64=vreinterpretq_u64_u8(vld1q_u8(b));
    uint8x8_t lo8=vget_low_u8(vld1q_u8(c));uint8x16_t c8=vld1q_u8(c);
    put(o,"addhn16",i,vcombine_u8(vaddhn_u16(n16,m16),vdup_n_u8(0)));
    put(o,"subhn16",i,vcombine_u8(vsubhn_u16(n16,m16),vdup_n_u8(0)));
    put(o,"raddhn16",i,vcombine_u8(vraddhn_u16(n16,m16),vdup_n_u8(0)));
    put(o,"rsubhn16",i,vcombine_u8(vrsubhn_u16(n16,m16),vdup_n_u8(0)));
    put(o,"addhn16_2",i,vaddhn_high_u16(lo8,n16,m16));
    put(o,"rsubhn16_2",i,vrsubhn_high_u16(lo8,n16,m16));
    put(o,"addhn32",i,vreinterpretq_u8_u16(vcombine_u16(vaddhn_u32(n32,m32),vdup_n_u16(0))));
    put(o,"raddhn32",i,vreinterpretq_u8_u16(vcombine_u16(vraddhn_u32(n32,m32),vdup_n_u16(0))));
    put(o,"subhn64",i,vreinterpretq_u8_u32(vcombine_u32(vsubhn_u64(n64,m64),vdup_n_u32(0))));
    put(o,"rsubhn64",i,vreinterpretq_u8_u32(vcombine_u32(vrsubhn_u64(n64,m64),vdup_n_u32(0))));
    int16x8_t s16=vreinterpretq_s16_u8(vld1q_u8(a)),t16=vreinterpretq_s16_u8(vld1q_u8(b));
    int32x4_t s32=vreinterpretq_s32_u8(vld1q_u8(a)),t32=vreinterpretq_s32_u8(vld1q_u8(b));
    int32x4_t acc32=vreinterpretq_s32_u8(c8);
    int64x2_t acc64=vreinterpretq_s64_u8(c8);
    put(o,"sqdmull16",i,vreinterpretq_u8_s32(vqdmull_s16(vget_low_s16(s16),vget_low_s16(t16))));
    put(o,"sqdmull16_2",i,vreinterpretq_u8_s32(vqdmull_high_s16(s16,t16)));
    put(o,"sqdmlal16",i,vreinterpretq_u8_s32(vqdmlal_s16(acc32,vget_low_s16(s16),vget_low_s16(t16))));
    put(o,"sqdmlsl16",i,vreinterpretq_u8_s32(vqdmlsl_s16(acc32,vget_low_s16(s16),vget_low_s16(t16))));
    put(o,"sqdmlal16_2",i,vreinterpretq_u8_s32(vqdmlal_high_s16(acc32,s16,t16)));
    put(o,"sqdmull32",i,vreinterpretq_u8_s64(vqdmull_s32(vget_low_s32(s32),vget_low_s32(t32))));
    put(o,"sqdmull32_2",i,vreinterpretq_u8_s64(vqdmull_high_s32(s32,t32)));
    put(o,"sqdmlal32",i,vreinterpretq_u8_s64(vqdmlal_s32(acc64,vget_low_s32(s32),vget_low_s32(t32))));
    put(o,"sqdmlsl32_2",i,vreinterpretq_u8_s64(vqdmlsl_high_s32(acc64,s32,t32)));
  }
  return 0;
}
