#include <stdio.h>
#include <stdint.h>
static const uint32_t S[]={0,0x80000000u,0x3F800000u,0xBF800000u,0x7F800000u,0xFF800000u,0x7FC00000u,0x7F800001u,0xFFC00000u,0xFF800001u,0x7FC00123u,0x7F8ABCDEu,1,0x80000001u,0x7F7FFFFFu};
static const uint64_t D[]={0,0x8000000000000000ull,0x3FF0000000000000ull,0xBFF0000000000000ull,0x7FF0000000000000ull,0xFFF0000000000000ull,0x7FF8000000000000ull,0x7FF0000000000001ull,0xFFF8000000000000ull,0xFFF0000000000001ull,0x7FF8000000000123ull,0x7FF0ABCDEF012345ull,1,0x8000000000000001ull};
#define NS (int)(sizeof S/sizeof*S)
#define ND (int)(sizeof D/sizeof*D)
#define OP(nm,fmt,T,R,asmstr,a,b) do{T x=a,y=b;T z;__asm__ volatile(asmstr:"=w"(z):"w"(x),"w"(y));fprintf(o,"%s %llx %llx %llx\n",nm,(unsigned long long)a,(unsigned long long)b,(unsigned long long)(R)z);}while(0)
int fm_run_cases(FILE*o){
  for(int i=0;i<NS;i++)for(int j=0;j<NS;j++){
    uint32_t a=S[i],b=S[j];
    uint32_t r;float x,y,z;
    #define F32(nm,ins) do{__builtin_memcpy(&x,&a,4);__builtin_memcpy(&y,&b,4);__asm__ volatile(ins " %s0,%s1,%s2":"=w"(z):"w"(x),"w"(y));__builtin_memcpy(&r,&z,4);fprintf(o,nm " %x %x %x\n",a,b,r);}while(0)
    F32("fmax","fmax");F32("fmin","fmin");F32("fmaxnm","fmaxnm");F32("fminnm","fminnm");F32("fadd","fadd");
  }
  for(int i=0;i<ND;i++)for(int j=0;j<ND;j++){
    uint64_t a=D[i],b=D[j],r;double x,y,z;
    #define F64(nm,ins) do{__builtin_memcpy(&x,&a,8);__builtin_memcpy(&y,&b,8);__asm__ volatile(ins " %d0,%d1,%d2":"=w"(z):"w"(x),"w"(y));__builtin_memcpy(&r,&z,8);fprintf(o,nm " %llx %llx %llx\n",(unsigned long long)a,(unsigned long long)b,(unsigned long long)r);}while(0)
    F64("dmax","fmax");F64("dmin","fmin");F64("dmaxnm","fmaxnm");F64("dminnm","fminnm");F64("dadd","fadd");
  }
  return 0;}
