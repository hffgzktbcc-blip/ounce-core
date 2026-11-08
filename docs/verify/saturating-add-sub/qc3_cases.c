#include <stdio.h>
#include <stdint.h>
#include <arm_neon.h>
static uint32_t st=777u;static uint32_t rnd(void){st=st*1664525u+1013904223u;return st;}
static const uint32_t EDGE[]={0,1,0xFFFFFFFFu,0x80000000u,0x7FFFFFFFu,0x00008000u,0x00007FFFu,0xFFFF8000u,0x000000FFu,0x00000080u,0xFFFFFF80u,0x7F7F7F7Fu,0x80808080u};
static uint32_t pick(void){uint32_t r=rnd();return (r&3)==0?EDGE[(r>>8)%(sizeof EDGE/sizeof*EDGE)]:rnd();}
static void clr(void){__asm__ volatile("msr fpsr, xzr":::"memory");}
static unsigned qc(void){uint64_t v;__asm__ volatile("mrs %0, fpsr":"=r"(v)::"memory");return (v>>27)&1;}
#define V(nm,T,ld,op,w) do{clr();T r=op(ld(a),ld(b));unsigned q=qc();uint64_t o[2]={0,0};__builtin_memcpy(o,&r,w);fprintf(F,nm " %d %016llx%016llx qc=%u\n",i,(unsigned long long)o[1],(unsigned long long)o[0],q);}while(0)
#define SC(nm,ins,mod,x,y) do{uint64x2_t va=vdupq_n_u64((uint64_t)(x)),vb=vdupq_n_u64((uint64_t)(y)),vr;clr();__asm__ volatile(ins " %" mod "0,%" mod "1,%" mod "2":"=w"(vr):"w"(va),"w"(vb):"memory");unsigned q=qc();uint64_t r=vgetq_lane_u64(vr,0);fprintf(F,nm " %d %llx qc=%u\n",i,(unsigned long long)r,q);}while(0)
int qc3_run_cases(FILE*F){
  for(int i=0;i<300;i++){
    uint32_t a_[4]={pick(),pick(),pick(),pick()},b_[4]={pick(),pick(),pick(),pick()};
    uint8_t*a=(uint8_t*)a_,*b=(uint8_t*)b_;
    #define LD8(p) vld1_s8((const int8_t*)(p))
    #define LD16(p) vld1_u16((const uint16_t*)(p))
    #define L8(p) vld1q_s8((const int8_t*)(p))
    #define LU8(p) vld1q_u8((const uint8_t*)(p))
    #define L16(p) vld1q_s16((const int16_t*)(p))
    #define LU16(p) vld1q_u16((const uint16_t*)(p))
    #define L32(p) vld1q_s32((const int32_t*)(p))
    #define LU32(p) vld1q_u32((const uint32_t*)(p))
    #define L64(p) vld1q_s64((const int64_t*)(p))
    #define LU64(p) vld1q_u64((const uint64_t*)(p))
    V("sqadd8",int8x16_t,L8,vqaddq_s8,16);V("uqadd8",uint8x16_t,LU8,vqaddq_u8,16);V("sqsub8",int8x16_t,L8,vqsubq_s8,16);V("uqsub8",uint8x16_t,LU8,vqsubq_u8,16);
    V("sqadd16",int16x8_t,L16,vqaddq_s16,16);V("uqadd16",uint16x8_t,LU16,vqaddq_u16,16);V("sqsub16",int16x8_t,L16,vqsubq_s16,16);V("uqsub16",uint16x8_t,LU16,vqsubq_u16,16);
    V("sqadd32",int32x4_t,L32,vqaddq_s32,16);V("uqadd32",uint32x4_t,LU32,vqaddq_u32,16);V("sqsub32",int32x4_t,L32,vqsubq_s32,16);V("uqsub32",uint32x4_t,LU32,vqsubq_u32,16);
    V("sqadd64",int64x2_t,L64,vqaddq_s64,16);V("uqadd64",uint64x2_t,LU64,vqaddq_u64,16);V("sqsub64",int64x2_t,L64,vqsubq_s64,16);V("uqsub64",uint64x2_t,LU64,vqsubq_u64,16);
    V("sqadd8d",int8x8_t,LD8,vqadd_s8,8);V("uqsub16d",uint16x4_t,LD16,vqsub_u16,8);
    SC("sqaddb","sqadd","b",a_[0],b_[0]);SC("uqsubb","uqsub","b",a_[0],b_[0]);SC("sqsubh","sqsub","h",a_[1],b_[1]);SC("uqaddh","uqadd","h",a_[1],b_[1]);
    SC("sqadds","sqadd","s",a_[2],b_[2]);SC("uqsubs","uqsub","s",a_[2],b_[2]);SC("sqaddd","sqadd","d",((uint64_t)a_[3]<<32)|a_[0],((uint64_t)b_[3]<<32)|b_[0]);SC("uqsubd","uqsub","d",((uint64_t)a_[3]<<32)|a_[0],((uint64_t)b_[3]<<32)|b_[0]);
  }
  return 0;}
