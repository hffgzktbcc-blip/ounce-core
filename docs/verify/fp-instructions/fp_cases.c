#include <stdio.h>
#include <stdint.h>
#include <string.h>
static uint64_t bits(double d){uint64_t u;memcpy(&u,&d,8);return u;}
static double dbl(uint64_t u){double d;memcpy(&d,&u,8);return d;}
static uint32_t bitsf(float d){uint32_t u;memcpy(&u,&d,4);return u;}
static const double V[]={0.0,-0.0,1.0,-1.0,0.5,2.5,-2.5,3.5,1.5,1e300,-1e300,1e-310,3.141592653589793,0.1,-7.25,9007199254740993.0,
  4503599627370497.5,1.0/0.0,-1.0/0.0,0.0/0.0,123456789.987654321,2.0,-3.999999999999999,1e19,-1e19,2147483648.5,-2147483649.5};
#define NV (int)(sizeof V/sizeof*V)
#define D3(name,asm_) do{for(int i=0;i<NV;i+=3)for(int j=0;j<NV;j+=4)for(int k=0;k<NV;k+=5){double a=V[i],b=V[j],c=V[k],r;\
  __asm__ volatile(asm_:"=w"(r):"w"(a),"w"(b),"w"(c));fprintf(o,"%s %d %d %d %016llx\n",name,i,j,k,(unsigned long long)bits(r));}}while(0)
#define D1(name,asm_) do{for(int i=0;i<NV;i++){double a=V[i],r;__asm__ volatile(asm_:"=w"(r):"w"(a));fprintf(o,"%s %d %016llx\n",name,i,(unsigned long long)bits(r));}}while(0)
#define D2(name,asm_) do{for(int i=0;i<NV;i++)for(int j=0;j<NV;j+=2){double a=V[i],b=V[j],r;__asm__ volatile(asm_:"=w"(r):"w"(a),"w"(b));fprintf(o,"%s %d %d %016llx\n",name,i,j,(unsigned long long)bits(r));}}while(0)
#define DI(name,asm_) do{for(int i=0;i<NV;i++){double a=V[i];long long r;__asm__ volatile(asm_:"=r"(r):"w"(a));fprintf(o,"%s %d %016llx\n",name,i,(unsigned long long)r);}}while(0)
int fp_run_cases(FILE*o){
  D3("fmadd","fmadd %d0, %d1, %d2, %d3");
  D3("fmsub","fmsub %d0, %d1, %d2, %d3");
  D3("fnmadd","fnmadd %d0, %d1, %d2, %d3");
  D3("fnmsub","fnmsub %d0, %d1, %d2, %d3");
  D2("fmul","fmul %d0, %d1, %d2"); D2("fdiv","fdiv %d0, %d1, %d2"); D2("fadd","fadd %d0, %d1, %d2"); D2("fsub","fsub %d0, %d1, %d2");
  D2("fmax","fmax %d0, %d1, %d2"); D2("fmin","fmin %d0, %d1, %d2"); D2("fmaxnm","fmaxnm %d0, %d1, %d2"); D2("fminnm","fminnm %d0, %d1, %d2");
  D2("fnmul","fnmul %d0, %d1, %d2");
  D1("fabs","fabs %d0, %d1"); D1("fneg","fneg %d0, %d1"); D1("fsqrt","fsqrt %d0, %d1");
  D1("frintn","frintn %d0, %d1"); D1("frintm","frintm %d0, %d1"); D1("frintp","frintp %d0, %d1"); D1("frintz","frintz %d0, %d1");
  D1("frinta","frinta %d0, %d1"); D1("frintx","frintx %d0, %d1"); D1("frinti","frinti %d0, %d1");
  DI("fcvtzs","fcvtzs %x0, %d1"); DI("fcvtzu","fcvtzu %x0, %d1"); DI("fcvtns","fcvtns %x0, %d1"); DI("fcvtms","fcvtms %x0, %d1");
  DI("fcvtps","fcvtps %x0, %d1"); DI("fcvtas","fcvtas %x0, %d1"); DI("fcvtzs32","fcvtzs %w0, %d1"); DI("fcvtzu32","fcvtzu %w0, %d1");
  for(int i=0;i<NV;i++){double a=V[i];float f;__asm__ volatile("fcvt %s0, %d1":"=w"(f):"w"(a));fprintf(o,"fcvt_sd %d %08x\n",i,bitsf(f));
    double b;__asm__ volatile("fcvt %d0, %s1":"=w"(b):"w"(f));fprintf(o,"fcvt_ds %d %016llx\n",i,(unsigned long long)bits(b));}
  static const long long IV[]={0,1,-1,2,255,-255,4294967295LL,-2147483648LL,9007199254740993LL,-9223372036854775807LL-1,9223372036854775807LL,123456789012345LL};
  for(unsigned i=0;i<sizeof IV/sizeof*IV;i++){long long a=IV[i];double r;
    __asm__ volatile("scvtf %d0, %x1":"=w"(r):"r"(a));fprintf(o,"scvtf %u %016llx\n",i,(unsigned long long)bits(r));
    __asm__ volatile("ucvtf %d0, %x1":"=w"(r):"r"(a));fprintf(o,"ucvtf %u %016llx\n",i,(unsigned long long)bits(r));}
  for(int i=0;i<NV;i++)for(int j=0;j<NV;j+=3){double a=V[i],b=V[j];unsigned long long fl;
    __asm__ volatile("fcmp %d1, %d2\n mrs %0, nzcv":"=r"(fl):"w"(a),"w"(b):"cc");fprintf(o,"fcmp %d %d %llx\n",i,j,fl>>28);
    __asm__ volatile("fcmpe %d1, %d2\n mrs %0, nzcv":"=r"(fl):"w"(a),"w"(b):"cc");fprintf(o,"fcmpe %d %d %llx\n",i,j,fl>>28);}
  return 0;
}
