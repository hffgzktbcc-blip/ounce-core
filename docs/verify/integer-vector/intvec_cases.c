#include <stdio.h>
#include <stdint.h>
#include <arm_neon.h>
/* Integer-vector differential: saturating narrows, rounding/accumulating/inserting/saturating shifts,
   across-lanes reductions. Same C builds for Apple Silicon (reference) and devkitA64 (guest). */
static volatile uint32_t st=991u;
static uint32_t rnd(void){uint32_t s=st*1664525u+1013904223u;st=s;return s;}
static const uint32_t EDGE[]={0,1,0xFFFFFFFFu,0x80000000u,0x7FFFFFFFu,0x00008000u,0x00007FFFu,0xFFFF8000u,
  0x80008000u,0x7FFF7FFFu,0x00FF00FFu,0x01010101u,0x80808080u,0x7F7F7F7Fu,0xFF00FF00u,0x00010000u,0xFFFEFFFEu};
static uint32_t pick(void){uint32_t r=rnd();return (r&3)==0?EDGE[(r>>8)%(sizeof EDGE/sizeof*EDGE)]:rnd();}
static void v128(uint8_t*b){for(int i=0;i<4;i++){uint32_t w=pick();b[4*i]=w;b[4*i+1]=w>>8;b[4*i+2]=w>>16;b[4*i+3]=w>>24;}}
static void pq(FILE*o,const char*n,int i,uint8x16_t r){uint8_t b[16];vst1q_u8(b,r);fprintf(o,"%s %d ",n,i);for(int k=15;k>=0;k--)fprintf(o,"%02x",b[k]);fputc('\n',o);}
static void ps(FILE*o,const char*n,int i,uint64_t v){fprintf(o,"%s %d %016llx\n",n,i,(unsigned long long)v);}
/* any vector type to u8x16 (64-bit vectors are zero-extended) */
#define CV(S,T) static inline uint8x16_t cv_##S(T v){return vreinterpretq_u8_##S(v);}
static inline uint8x16_t cv_u8(uint8x16_t v){return v;}
CV(s8,int8x16_t) CV(u16,uint16x8_t) CV(s16,int16x8_t) CV(u32,uint32x4_t) CV(s32,int32x4_t) CV(u64,uint64x2_t) CV(s64,int64x2_t)
static inline uint8x16_t cd_u8(uint8x8_t v){return vcombine_u8(v,vdup_n_u8(0));}
static inline uint8x16_t cd_s8(int8x8_t v){return vcombine_u8(vreinterpret_u8_s8(v),vdup_n_u8(0));}
static inline uint8x16_t cd_u16(uint16x4_t v){return vreinterpretq_u8_u16(vcombine_u16(v,vdup_n_u16(0)));}
static inline uint8x16_t cd_s16(int16x4_t v){return vreinterpretq_u8_u16(vcombine_u16(vreinterpret_u16_s16(v),vdup_n_u16(0)));}
static inline uint8x16_t cd_u32(uint32x2_t v){return vreinterpretq_u8_u32(vcombine_u32(v,vdup_n_u32(0)));}
static inline uint8x16_t cd_s32(int32x2_t v){return vreinterpretq_u8_u32(vcombine_u32(vreinterpret_u32_s32(v),vdup_n_u32(0)));}
#define CVT(v) _Generic((v), uint8x16_t:cv_u8,int8x16_t:cv_s8,uint16x8_t:cv_u16,int16x8_t:cv_s16,uint32x4_t:cv_u32,int32x4_t:cv_s32,\
  uint64x2_t:cv_u64,int64x2_t:cv_s64,uint8x8_t:cd_u8,int8x8_t:cd_s8,uint16x4_t:cd_u16,int16x4_t:cd_s16,uint32x2_t:cd_u32,int32x2_t:cd_s32)(v)
#define P(n,e) pq(o,n,i,CVT(e))
#define S(n,e) ps(o,n,i,(uint64_t)(e))
int intvec_run_cases(FILE*o){
  for(int i=0;i<400;i++){
    uint8_t a[16],b[16],c[16];v128(a);v128(b);v128(c);
    uint8x16_t u8a=vld1q_u8(a),u8c=vld1q_u8(c);
    int8x16_t s8a=vreinterpretq_s8_u8(u8a);
    uint16x8_t u16a=vreinterpretq_u16_u8(u8a),u16c=vreinterpretq_u16_u8(u8c);
    int16x8_t s16a=vreinterpretq_s16_u8(u8a),s16c=vreinterpretq_s16_u8(u8c);
    uint32x4_t u32a=vreinterpretq_u32_u8(u8a),u32c=vreinterpretq_u32_u8(u8c);
    int32x4_t s32a=vreinterpretq_s32_u8(u8a),s32c=vreinterpretq_s32_u8(u8c);
    uint64x2_t u64a=vreinterpretq_u64_u8(u8a),u64c=vreinterpretq_u64_u8(u8c);
    int64x2_t s64a=vreinterpretq_s64_u8(u8a),s64c=vreinterpretq_s64_u8(u8c);
    uint8x8_t lo8=vget_low_u8(u8c);uint16x4_t lo16=vget_low_u16(u16c);uint32x2_t lo32=vget_low_u32(u32c);
    int16x4_t slo16=vget_low_s16(s16c);int8x8_t slo8=vget_low_s8(vreinterpretq_s8_u8(u8c));
    uint64_t x=vgetq_lane_u64(u64a,0),y=vgetq_lane_u64(u64c,0);

    /* saturating narrow: SQXTUN SQXTN UQXTN (+2) */
    P("sqxtun16",vqmovun_s16(s16a)); P("sqxtun32",vqmovun_s32(s32a)); P("sqxtun64",vqmovun_s64(s64a));
    P("sqxtun16_2",vqmovun_high_s16(lo8,s16a)); P("sqxtun32_2",vqmovun_high_s32(lo16,s32a));
    P("sqxtn16",vqmovn_s16(s16a)); P("sqxtn32",vqmovn_s32(s32a)); P("sqxtn64",vqmovn_s64(s64a));
    P("sqxtn16_2",vqmovn_high_s16(slo8,s16a)); P("sqxtn64_2",vqmovn_high_s64(vreinterpret_s32_u32(lo32),s64a));
    P("uqxtn16",vqmovn_u16(u16a)); P("uqxtn32",vqmovn_u32(u32a)); P("uqxtn64",vqmovn_u64(u64a));
    P("uqxtn16_2",vqmovn_high_u16(lo8,u16a)); P("uqxtn32_2",vqmovn_high_u32(lo16,u32a));
    S("sqxtn_h",(uint8_t)vqmovnh_s16(vgetq_lane_s16(s16a,1))); S("sqxtn_s",(uint16_t)vqmovns_s32(vgetq_lane_s32(s32a,2)));
    S("sqxtn_d",(uint32_t)vqmovnd_s64(vgetq_lane_s64(s64a,1)));
    S("uqxtn_h",(uint8_t)vqmovnh_u16(vgetq_lane_u16(u16a,3))); S("uqxtn_d",(uint32_t)vqmovnd_u64(vgetq_lane_u64(u64a,0)));
    S("sqxtun_s",(uint16_t)vqmovuns_s32(vgetq_lane_s32(s32a,0))); S("sqxtun_h",(uint8_t)vqmovunh_s16(vgetq_lane_s16(s16a,5)));

    /* narrowing shifts */
    P("rshrn16",vrshrn_n_u16(u16a,4)); P("rshrn16b",vrshrn_n_u16(u16a,8)); P("rshrn32",vrshrn_n_u32(u32a,1));
    P("rshrn32b",vrshrn_n_s32(s32a,16)); P("rshrn64",vrshrn_n_u64(u64a,13)); P("rshrn64b",vrshrn_n_u64(u64a,32));
    P("rshrn16_2",vrshrn_high_n_u16(lo8,u16a,3)); P("rshrn32_2",vrshrn_high_n_s32(slo16,s32a,7));
    P("sqshrn16",vqshrn_n_s16(s16a,3)); P("sqshrn32",vqshrn_n_s32(s32a,5)); P("sqshrn32b",vqshrn_n_s32(s32a,16));
    P("sqshrn64",vqshrn_n_s64(s64a,9)); P("uqshrn16",vqshrn_n_u16(u16a,2)); P("uqshrn32",vqshrn_n_u32(u32a,11));
    P("uqshrn64",vqshrn_n_u64(u64a,20)); P("sqshrn32_2",vqshrn_high_n_s32(slo16,s32a,4)); P("uqshrn16_2",vqshrn_high_n_u16(lo8,u16a,1));
    P("sqrshrn16",vqrshrn_n_s16(s16a,1)); P("sqrshrn32",vqrshrn_n_s32(s32a,6)); P("sqrshrn64",vqrshrn_n_s64(s64a,31));
    P("uqrshrn16",vqrshrn_n_u16(u16a,3)); P("uqrshrn32",vqrshrn_n_u32(u32a,9)); P("uqrshrn64",vqrshrn_n_u64(u64a,32));
    P("sqrshrn32_2",vqrshrn_high_n_s32(slo16,s32a,12));
    P("sqshrun16",vqshrun_n_s16(s16a,2)); P("sqshrun32",vqshrun_n_s32(s32a,7)); P("sqshrun64",vqshrun_n_s64(s64a,17));
    P("sqrshrun16",vqrshrun_n_s16(s16a,5)); P("sqrshrun32",vqrshrun_n_s32(s32a,15)); P("sqrshrun64",vqrshrun_n_s64(s64a,3));
    P("sqshrun64_2",vqshrun_high_n_s64(lo32,s64a,9)); P("sqrshrun16_2",vqrshrun_high_n_s16(lo8,s16a,6));
    S("sqshrn_s",(uint16_t)vqshrns_n_s32(vgetq_lane_s32(s32a,1),4)); S("uqrshrn_d",(uint32_t)vqrshrnd_n_u64(x,10));
    S("sqshrun_h",(uint8_t)vqshrunh_n_s16(vgetq_lane_s16(s16a,2),3));

    /* accumulate / rounding shifts */
    P("ssra8",vsraq_n_s8(s8a,vreinterpretq_s8_u8(u8c),3)); P("usra8",vsraq_n_u8(u8a,u8c,1)); P("usra8b",vsraq_n_u8(u8a,u8c,8));
    P("ssra16",vsraq_n_s16(s16a,s16c,5)); P("usra16",vsraq_n_u16(u16a,u16c,16)); P("usra32",vsraq_n_u32(u32a,u32c,3));
    P("ssra32",vsraq_n_s32(s32a,s32c,31)); P("ssra64",vsraq_n_s64(s64a,s64c,7)); P("usra64",vsraq_n_u64(u64a,u64c,40));
    P("usra32d",vsra_n_u32(vget_low_u32(u32a),vget_low_u32(u32c),9)); P("ssra16d",vsra_n_s16(vget_low_s16(s16a),vget_low_s16(s16c),2));
    S("usra_d",vsrad_n_u64(x,y,7)); S("ssra_d",(uint64_t)vsrad_n_s64((int64_t)x,(int64_t)y,63));
    P("srshr8",vrshrq_n_s8(s8a,2)); P("urshr8",vrshrq_n_u8(u8a,8)); P("srshr16",vrshrq_n_s16(s16a,1)); P("urshr16",vrshrq_n_u16(u16a,15));
    P("srshr32",vrshrq_n_s32(s32a,4)); P("urshr32",vrshrq_n_u32(u32a,1)); P("urshr32b",vrshrq_n_u32(u32a,32));
    P("srshr64",vrshrq_n_s64(s64a,17)); P("urshr64",vrshrq_n_u64(u64a,64)); P("urshr32d",vrshr_n_u32(vget_low_u32(u32a),5));
    S("srshr_d",(uint64_t)vrshrd_n_s64((int64_t)x,11)); S("urshr_d",vrshrd_n_u64(x,1));
    P("srsra8",vrsraq_n_s8(s8a,vreinterpretq_s8_u8(u8c),4)); P("ursra8",vrsraq_n_u8(u8a,u8c,2));
    P("srsra16",vrsraq_n_s16(s16a,s16c,3)); P("ursra16",vrsraq_n_u16(u16a,u16c,9)); P("srsra32",vrsraq_n_s32(s32a,s32c,10));
    P("ursra32",vrsraq_n_u32(u32a,u32c,32)); P("srsra64",vrsraq_n_s64(s64a,s64c,33)); P("ursra64",vrsraq_n_u64(u64a,u64c,5));
    S("ursra_d",vrsrad_n_u64(x,y,12)); S("srsra_d",(uint64_t)vrsrad_n_s64((int64_t)x,(int64_t)y,1));

    /* insert */
    P("sri8",vsriq_n_u8(u8a,u8c,3)); P("sri16",vsriq_n_u16(u16a,u16c,5)); P("sri16b",vsriq_n_u16(u16a,u16c,16));
    P("sri32",vsriq_n_u32(u32a,u32c,8)); P("sri64",vsriq_n_u64(u64a,u64c,20)); S("sri_d",vsrid_n_u64(x,y,9));
    P("sli8",vsliq_n_u8(u8a,u8c,3)); P("sli16",vsliq_n_u16(u16a,u16c,5)); P("sli16b",vsliq_n_u16(u16a,u16c,15));
    P("sli32",vsliq_n_u32(u32a,u32c,8)); P("sli64",vsliq_n_u64(u64a,u64c,20)); S("sli_d",vslid_n_u64(x,y,9));
    P("sli32d",vsli_n_u32(vget_low_u32(u32a),vget_low_u32(u32c),31));

    /* saturating shift left by immediate */
    P("sqshl8",vqshlq_n_s8(s8a,2)); P("uqshl8",vqshlq_n_u8(u8a,3)); P("sqshl8b",vqshlq_n_s8(s8a,7));
    P("sqshl16",vqshlq_n_s16(s16a,4)); P("uqshl16",vqshlq_n_u16(u16a,9)); P("sqshl32",vqshlq_n_s32(s32a,3));
    P("uqshl32",vqshlq_n_u32(u32a,16)); P("sqshl32b",vqshlq_n_s32(s32a,31)); P("sqshl64",vqshlq_n_s64(s64a,20));
    P("uqshl64",vqshlq_n_u64(u64a,63)); P("sqshl32d",vqshl_n_s32(vget_low_s32(s32a),1));
    P("sqshlu8",vqshluq_n_s8(s8a,1)); P("sqshlu16",vqshluq_n_s16(s16a,4)); P("sqshlu32",vqshluq_n_s32(s32a,12));
    P("sqshlu64",vqshluq_n_s64(s64a,33)); P("sqshlu32b",vqshluq_n_s32(s32a,0));
    S("sqshl_b",(uint8_t)vqshlb_n_s8(vgetq_lane_s8(s8a,3),2)); S("uqshl_h",(uint16_t)vqshlh_n_u16(vgetq_lane_u16(u16a,2),5));
    S("sqshl_s",(uint32_t)vqshls_n_s32(vgetq_lane_s32(s32a,1),9)); S("uqshl_d",vqshld_n_u64(x,12));
    S("sqshlu_s",(uint32_t)vqshlus_n_s32(vgetq_lane_s32(s32a,3),6)); S("sqshlu_d",vqshlud_n_s64((int64_t)y,4));

    /* pairwise long */
    P("uaddlp8",vpaddlq_u8(u8a)); P("saddlp8",vpaddlq_s8(s8a)); P("uaddlp16",vpaddlq_u16(u16a)); P("saddlp16",vpaddlq_s16(s16a));
    P("uaddlp32",vpaddlq_u32(u32a)); S("saddlp32d",(uint64_t)vget_lane_s64(vpaddl_s32(vget_low_s32(s32a)),0)); P("uaddlp8d",vpaddl_u8(vget_low_u8(u8a)));
    P("uadalp8",vpadalq_u8(u16c,u8a)); P("sadalp16",vpadalq_s16(s32c,s16a)); P("uadalp32",vpadalq_u32(u64c,u32a)); P("sadalp8d",vpadal_s8(vget_low_s16(s16c),vget_low_s8(s8a)));
    /* across lanes */
    S("addv8q",vaddvq_u8(u8a)); S("addv16q",vaddvq_u16(u16a)); S("addv32q",vaddvq_u32(u32a)); S("addv64q",vaddvq_u64(u64a));
    S("addv8",vaddv_u8(vget_low_u8(u8a))); S("addv16",vaddv_u16(vget_low_u16(u16a))); S("addv32",vaddv_u32(vget_low_u32(u32a)));
    S("addv_s8q",(uint8_t)vaddvq_s8(s8a)); S("addv_s32q",(uint32_t)vaddvq_s32(s32a));
    S("uaddlv8q",vaddlvq_u8(u8a)); S("uaddlv16q",vaddlvq_u16(u16a)); S("uaddlv32q",vaddlvq_u32(u32a));
    S("uaddlv8",vaddlv_u8(vget_low_u8(u8a))); S("uaddlv16",vaddlv_u16(vget_low_u16(u16a))); S("uaddlv32",vaddlv_u32(vget_low_u32(u32a)));
    S("saddlv8q",(uint16_t)vaddlvq_s8(s8a)); S("saddlv16q",(uint32_t)vaddlvq_s16(s16a)); S("saddlv32q",(uint64_t)vaddlvq_s32(s32a));
    S("saddlv8",(uint16_t)vaddlv_s8(vget_low_s8(s8a))); S("saddlv16",(uint32_t)vaddlv_s16(vget_low_s16(s16a)));
    S("umaxv8q",vmaxvq_u8(u8a)); S("umaxv16q",vmaxvq_u16(u16a)); S("umaxv32q",vmaxvq_u32(u32a)); S("umaxv8",vmaxv_u8(vget_low_u8(u8a)));
    S("smaxv8q",(uint8_t)vmaxvq_s8(s8a)); S("smaxv16q",(uint16_t)vmaxvq_s16(s16a)); S("smaxv32q",(uint32_t)vmaxvq_s32(s32a));
    S("uminv8q",vminvq_u8(u8a)); S("uminv16q",vminvq_u16(u16a)); S("uminv32q",vminvq_u32(u32a)); S("uminv16",vminv_u16(vget_low_u16(u16a)));
    S("sminv8q",(uint8_t)vminvq_s8(s8a)); S("sminv16q",(uint16_t)vminvq_s16(s16a)); S("sminv32q",(uint32_t)vminvq_s32(s32a));
  }
  return 0;
}
