/* wasm3 differential cases. The WebAssembly modules are hand-assembled by gen_wasm.py (no toolchain). */
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include "wasm3.h"
#include "wasm_modules.h"

/* original 61-byte module: recursive fib(i32) -> i32 */
static const uint8_t WASM_FIB[]={0,0x61,0x73,0x6d,1,0,0,0, 1,6,1,0x60,1,0x7f,1,0x7f, 3,2,1,0, 7,7,1,3,'f','i','b',0,0,
 0x0a,0x1e,1,0x1c,0,0x20,0,0x41,2,0x48,0x04,0x7f,0x20,0,5,0x20,0,0x41,1,0x6b,0x10,0,0x20,0,0x41,2,0x6b,0x10,0,0x6a,0x0b,0x0b};

static FILE *O;
static IM3Runtime RT;

static IM3Runtime load(const uint8_t *w, size_t n, const char *tag){
  IM3Environment env=m3_NewEnvironment();
  IM3Runtime rt=m3_NewRuntime(env,64*1024,NULL);
  IM3Module mod; M3Result r=m3_ParseModule(env,&mod,w,(uint32_t)n);
  if(r){fprintf(O,"%s parse %s\n",tag,r);return NULL;}
  r=m3_LoadModule(rt,mod); if(r){fprintf(O,"%s load %s\n",tag,r);return NULL;}
  return rt;}

static void pv(int t,uint64_t v){
  switch(t){case c_m3Type_i32: case c_m3Type_f32: fprintf(O," %08x",(uint32_t)v);break;
            default: fprintf(O," %016llx",(unsigned long long)v);}}

/* call export `nm` with raw argument bits; print args and results (or the trap text) */
static void call(const char *nm,const uint64_t *a){
  IM3Function f; M3Result r=m3_FindFunction(&f,RT,nm);
  if(r){fprintf(O,"%s find %s\n",nm,r);return;}
  uint32_t ac=m3_GetArgCount(f),rc=m3_GetRetCount(f);
  const void *p[8]; for(uint32_t i=0;i<ac;i++)p[i]=&a[i];
  fprintf(O,"%s",nm); for(uint32_t i=0;i<ac;i++)pv(m3_GetArgType(f,i),a[i]);
  r=m3_Call(f,ac,p);
  if(r){fprintf(O," -> trap: %s\n",r);return;}
  uint64_t res[4]={0}; const void *q[4]; for(uint32_t i=0;i<rc;i++)q[i]=&res[i];
  r=m3_GetResults(f,rc,q); if(r){fprintf(O," -> results %s\n",r);return;}
  fprintf(O," ->"); for(uint32_t i=0;i<rc;i++)pv(m3_GetRetType(f,i),res[i]); fprintf(O,"\n");}

#define CALL(nm,...) do{uint64_t a_[]={0,__VA_ARGS__};call(nm,a_+1);}while(0)

static const uint32_t E32[]={0,1,0xffffffffu,2,31,32,0x7fffffffu,0x80000000u,0x12345678u,0xdeadbeefu};
static const uint64_t E64[]={0,1,~0ull,2,63,64,0x7fffffffffffffffull,0x8000000000000000ull,0x123456789abcdef0ull,
  0xdeadbeefcafef00dull,0xffffffffull,0x100000000ull};
static const uint32_t EF[]={0x00000000,0x80000000,0x3f800000,0xbf800000,0x3f000000,0x40200000,0xc0200000,0x7f800000,
  0xff800000,0x7fc00000,0x7fa00000,0x4f000000,0xcf000000,0x4f800000,0x00000001,0x7f7fffff,0x5f000000};
static const uint64_t ED[]={0,0x8000000000000000ull,0x3ff0000000000000ull,0xbff0000000000000ull,0x3fe0000000000000ull,
  0x4004000000000000ull,0xc004000000000000ull,0x7ff0000000000000ull,0xfff0000000000000ull,0x7ff8000000000000ull,
  0x7ff4000000000000ull,0x41e0000000000000ull,0xc1e0000000000000ull,0x41f0000000000000ull,0x0000000000000001ull,
  0x7fefffffffffffffull,0x43e0000000000000ull};
#define N(x) (sizeof(x)/sizeof((x)[0]))

static uint64_t val(int t,size_t k){
  switch(t){case c_m3Type_i32: return E32[k%N(E32)];
            case c_m3Type_i64: return E64[k%N(E64)];
            case c_m3Type_f32: return EF[k%N(EF)];
            default: return ED[k%N(ED)];}}
static size_t cnt(int t){return t==c_m3Type_i32?N(E32):t==c_m3Type_i64?N(E64):t==c_m3Type_f32?N(EF):N(ED);}

static void auto_all(void){
  for(int n=0;n<N_AUTO;n++){
    IM3Function f; if(m3_FindFunction(&f,RT,AUTO_FUNCS[n])){fprintf(O,"%s missing\n",AUTO_FUNCS[n]);continue;}
    uint32_t ac=m3_GetArgCount(f); int t0=m3_GetArgType(f,0);
    if(ac==1){for(size_t i=0;i<cnt(t0);i++){uint64_t a[1]={val(t0,i)};call(AUTO_FUNCS[n],a);}}
    else{int t1=m3_GetArgType(f,1);
      for(size_t i=0;i<cnt(t0);i++)for(size_t j=0;j<cnt(t1);j++){uint64_t a[2]={val(t0,i),val(t1,j)};call(AUTO_FUNCS[n],a);}}}}

int wasm3_run_cases(FILE*o){
  O=o;
  fprintf(o,"== fib (61-byte module)\n");
  RT=load(WASM_FIB,sizeof WASM_FIB,"fib"); if(!RT)return 1;
  for(int i=0;i<=22;i++)CALL("fib",(uint64_t)i);

  fprintf(o,"== numeric ops\n");
  RT=load(WASM_OPS,sizeof WASM_OPS,"ops"); if(!RT)return 1;
  auto_all();
  CALL("x.select",1,2,0); CALL("x.select",1,2,5); CALL("x.unreachable");

  fprintf(o,"== memory\n");
  static const char*L[]={"i32.load","i64.load","f32.load","f64.load","i32.load8_s","i32.load8_u","i32.load16_s","i32.load16_u",
    "i64.load8_s","i64.load8_u","i64.load16_s","i64.load16_u","i64.load32_s","i64.load32_u"};
  char nm[64];
  for(size_t k=0;k<N(L);k++){for(uint64_t ad=16;ad<=22;ad+=3){snprintf(nm,sizeof nm,"mem.%s",L[k]);CALL(nm,ad);}
    uint64_t bad[]={65535,65532,65528,0xffffffffu,65536};
    for(size_t b=0;b<N(bad);b++){snprintf(nm,sizeof nm,"mem.%s",L[k]);CALL(nm,bad[b]);}
    uint64_t of[]={0,2,6,7,0xffffffffu};
    for(size_t b=0;b<N(of);b++){snprintf(nm,sizeof nm,"mem.%s_off",L[k]);CALL(nm,of[b]);}}
  static const char*S[]={"i32.store","i64.store","f32.store","f64.store","i32.store8","i32.store16","i64.store8","i64.store16","i64.store32"};
  for(size_t k=0;k<N(S);k++){snprintf(nm,sizeof nm,"mem.%s",S[k]);
    uint64_t v=(k==1||k>=6)?0x1122334455667788ull:(k==3?0x400921fb54442d18ull:(k==2?0x40490fdbu:0xa1b2c3d4u));
    CALL("mem.i64.store",200,0xffffffffffffffffull); CALL(nm,200,v); CALL("mem.i64.load",200); CALL("mem.i64.load",204);
    CALL(nm,65535,v); CALL(nm,65533,v); CALL(nm,65529,v); CALL(nm,65536,v); CALL(nm,0xffffffffu,v);}
  CALL("mem.sum",0); CALL("mem.sum",32); CALL("mem.sum",300); CALL("mem.sum",65536); CALL("mem.sum",65537);
  CALL("mem.size"); CALL("mem.grow",0); CALL("mem.grow",1); CALL("mem.size"); CALL("mem.i32.store",70000,0xcafebabeu);
  CALL("mem.i32.load",70000); CALL("mem.grow",1); CALL("mem.size"); CALL("mem.grow",1); CALL("mem.grow",0); CALL("mem.i32.load",196604);
  CALL("mem.i32.load",196606); CALL("mem.grow",0xffffffffu); CALL("mem.size");

  fprintf(o,"== programs\n");
  RT=load(WASM_PROG,sizeof WASM_PROG,"prog"); if(!RT)return 1;
  for(uint64_t i=0;i<=25;i++)CALL("fib",i);
  for(uint64_t i=0;i<=25;i++)CALL("fac",i);
  CALL("fac",0xffffffffffffffffull>>58); CALL("fac",33); CALL("fac",64);
  { uint64_t c[]={1,2,3,6,7,9,27,97,871,6171,77031,837799}; for(size_t i=0;i<N(c);i++)CALL("collatz",c[i]); }
  for(int i=-2;i<=5;i++)CALL("brtable",(uint64_t)(uint32_t)i); CALL("brtable",0x7fffffffu); CALL("brtable",0x80000000u);
  { uint64_t s[]={0,1,2,3,10,100,1000,20000,65536}; for(size_t i=0;i<N(s);i++)CALL("sieve",s[i]); }
  { uint64_t g[][2]={{12,18},{17,5},{0,9},{9,0},{0,0},{1071,462},{0x7fffffff,0x7ffffffe},{1000000,10}}; for(size_t i=0;i<N(g);i++)CALL("gcd",g[i][0],g[i][1]); }
  for(int i=0;i<5;i++)CALL("counter");
  for(uint64_t i=0;i<=5;i++){CALL("dispatch",i,7,3);CALL("dispatch",i,0xfffffffbu,2);}
  CALL("dispatch",0xffffffffu,1,1);
  return 0;}
