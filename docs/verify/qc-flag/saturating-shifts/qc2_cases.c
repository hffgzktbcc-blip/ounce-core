#include <stdio.h>
#include <stdint.h>
#include <arm_neon.h>
static uint32_t st=4242u;static uint32_t rnd(void){st=st*1664525u+1013904223u;return st;}
static const uint32_t EDGE[]={0,1,0xFFFFFFFFu,0x80000000u,0x7FFFFFFFu,0x00008000u,0x00007FFFu,0xFFFF8000u,0x00010000u,0xFFFF0000u,0x000000FFu,0x00000100u,0x0000FF80u,0xFFFFFF80u};
static uint32_t pick(void){uint32_t r=rnd();return (r&3)==0?EDGE[(r>>8)%(sizeof EDGE/sizeof*EDGE)]:rnd();}
static void clr(void){__asm__ volatile("msr fpsr, xzr":::"memory");}
static unsigned qc(void){uint64_t v;__asm__ volatile("mrs %0, fpsr":"=r"(v)::"memory");return (v>>27)&1;}
#define VEC(name,expr) do{clr();uint64_t _r=(expr);unsigned _q=qc();fprintf(o,"%s %d %016llx qc=%u\n",name,i,(unsigned long long)_r,_q);}while(0)
int qc2_run_cases(FILE*o){
  for(int i=0;i<300;i++){
    uint32_t w[4]={pick(),pick(),pick(),pick()};
    int16x8_t s16=vreinterpretq_s16_u32(vld1q_u32(w));uint16x8_t u16=vreinterpretq_u16_u32(vld1q_u32(w));
    int32x4_t s32=vreinterpretq_s32_u32(vld1q_u32(w));uint32x4_t u32=vld1q_u32(w);
    int64x2_t s64=vreinterpretq_s64_u32(vld1q_u32(w));uint64x2_t u64=vreinterpretq_u64_u32(vld1q_u32(w));
    VEC("sqxtun16",vgetq_lane_u64(vreinterpretq_u64_u8(vcombine_u8(vqmovun_s16(s16),vdup_n_u8(0))),0));
    VEC("sqxtn16",vgetq_lane_u64(vreinterpretq_u64_s8(vcombine_s8(vqmovn_s16(s16),vdup_n_s8(0))),0));
    VEC("uqxtn16",vgetq_lane_u64(vreinterpretq_u64_u8(vcombine_u8(vqmovn_u16(u16),vdup_n_u8(0))),0));
    VEC("sqxtn32",vgetq_lane_u64(vreinterpretq_u64_s16(vcombine_s16(vqmovn_s32(s32),vdup_n_s16(0))),0));
    VEC("uqxtn64",vgetq_lane_u64(vreinterpretq_u64_u32(vcombine_u32(vqmovn_u64(u64),vdup_n_u32(0))),0));
    VEC("sqxtun64",vgetq_lane_u64(vreinterpretq_u64_u32(vcombine_u32(vqmovun_s64(s64),vdup_n_u32(0))),0));
    VEC("sqshrn32_3",vgetq_lane_u64(vreinterpretq_u64_s16(vcombine_s16(vqshrn_n_s32(s32,3),vdup_n_s16(0))),0));
    VEC("uqshrn32_5",vgetq_lane_u64(vreinterpretq_u64_u16(vcombine_u16(vqshrn_n_u32(u32,5),vdup_n_u16(0))),0));
    VEC("sqrshrn32_4",vgetq_lane_u64(vreinterpretq_u64_s16(vcombine_s16(vqrshrn_n_s32(s32,4),vdup_n_s16(0))),0));
    VEC("sqshrun32_2",vgetq_lane_u64(vreinterpretq_u64_u16(vcombine_u16(vqshrun_n_s32(s32,2),vdup_n_u16(0))),0));
    VEC("sqrshrun16_3",vgetq_lane_u64(vreinterpretq_u64_u8(vcombine_u8(vqrshrun_n_s16(s16,3),vdup_n_u8(0))),0));
    VEC("sqshl32_4",vgetq_lane_u64(vreinterpretq_u64_s32(vqshlq_n_s32(s32,4)),0)^vgetq_lane_u64(vreinterpretq_u64_s32(vqshlq_n_s32(s32,4)),1));
    VEC("uqshl32_6",vgetq_lane_u64(vreinterpretq_u64_u32(vqshlq_n_u32(u32,6)),0)^vgetq_lane_u64(vreinterpretq_u64_u32(vqshlq_n_u32(u32,6)),1));
    VEC("sqshlu16_2",vgetq_lane_u64(vreinterpretq_u64_u16(vqshluq_n_s16(s16,2)),0)^vgetq_lane_u64(vreinterpretq_u64_u16(vqshluq_n_s16(s16,2)),1));
    VEC("sqshl64_3",(uint64_t)vgetq_lane_s64(vqshlq_n_s64(s64,3),0)^(uint64_t)vgetq_lane_s64(vqshlq_n_s64(s64,3),1));
    VEC("uqxtn_s",(uint64_t)vqmovns_u32(w[0]));       /* scalar */
    VEC("sqxtun_s",(uint64_t)vqmovuns_s32((int32_t)w[1]));
    VEC("sqshl_d",(uint64_t)vqshld_n_s64((int64_t)(((uint64_t)w[2]<<32)|w[3]),5));
    VEC("uqshl_s",(uint64_t)vqshls_n_u32(w[0],7));
  }
  return 0;
}
