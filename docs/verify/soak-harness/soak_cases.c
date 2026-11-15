#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
static uint64_t s[2]={0x9E3779B97F4A7C15ull,0xD1B54A32D192ED03ull};
static uint64_t rnd(void){uint64_t a=s[0],b=s[1];s[0]=b;a^=a<<23;s[1]=a^b^(a>>17)^(b>>26);return s[1]+b;}
static uint32_t crc(const uint8_t*p,size_t n){uint32_t c=~0u;while(n--){c^=*p++;for(int k=0;k<8;k++)c=(c>>1)^(0xEDB88320u&-(c&1));}return ~c;}
static uint64_t f_sieve(void){enum{N=200000};static uint8_t c[N];memset(c,0,N);uint64_t n=0;for(int i=2;i<N;i++){if(!c[i]){n++;for(long j=(long)i*i;j<N;j+=i)c[j]=1;}}return n;}
static uint64_t f_mat(void){enum{M=48};static int32_t a[M][M],b[M][M],r[M][M];for(int i=0;i<M;i++)for(int j=0;j<M;j++){a[i][j]=(int32_t)(rnd()&0xffff)-32768;b[i][j]=(int32_t)(rnd()&0xffff)-32768;}
 for(int i=0;i<M;i++)for(int j=0;j<M;j++){int64_t t=0;for(int k=0;k<M;k++)t+=(int64_t)a[i][k]*b[k][j];r[i][j]=(int32_t)t;}uint64_t h=0;for(int i=0;i<M;i++)for(int j=0;j<M;j++)h=h*1000003u^(uint32_t)r[i][j];return h;}
static uint64_t f_crc(void){static uint8_t buf[8192];for(int i=0;i<8192;i++)buf[i]=(uint8_t)rnd();return crc(buf,sizeof buf);}
static uint64_t f_fp(void){double acc=0;for(int py=0;py<24;py++)for(int px=0;px<48;px++){double x=0,y=0,cx=px/16.0-2.0,cy=py/12.0-1.0;int it=0;while(it<200&&x*x+y*y<4.0){double t=x*x-y*y+cx;y=2*x*y+cy;x=t;it++;}acc+=it;}
 float v[256];for(int i=0;i<256;i++)v[i]=(float)(rnd()%1000)/7.0f+0.5f;float d=0;for(int i=0;i<256;i++)d+=sqrtf(v[i])*v[(i*7)&255]/(v[i]+1.0f);uint64_t h;double sum=acc+d;memcpy(&h,&sum,8);return h;}
static uint64_t f_heap(void){enum{K=300};void*p[K];size_t z[K];memset(p,0,sizeof p);uint64_t h=0;for(int r=0;r<2000;r++){int i=(int)(rnd()%K);if(p[i]){uint8_t*q=p[i];for(size_t j=0;j<z[i];j+=61)h=h*31+q[j];free(p[i]);p[i]=0;}else{z[i]=16+rnd()%4000;p[i]=malloc(z[i]);if(!p[i])return 1;memset(p[i],(int)i,z[i]);}}for(int i=0;i<K;i++)free(p[i]);return h;}
static int cmp(const void*a,const void*b){uint32_t x=*(const uint32_t*)a,y=*(const uint32_t*)b;return x<y?-1:x>y;}
static uint64_t f_sort(void){static uint32_t a[4000];for(int i=0;i<4000;i++)a[i]=(uint32_t)rnd();qsort(a,4000,4,cmp);uint64_t h=0;for(int i=0;i<4000;i++)h=h*1099511628211ull^a[i];return h;}
static uint64_t f_div(void){uint64_t h=0;for(int i=0;i<3000;i++){int64_t a=(int64_t)rnd(),b=(int64_t)(rnd()|1);if(b==-1)b=3;h^=(uint64_t)(a/b)+(uint64_t)(a%b)+(uint64_t)((uint32_t)a/(uint32_t)(b|1));}return h;}
typedef uint64_t(*fn)(void);
static fn tab[]={f_sieve,f_mat,f_crc,f_fp,f_heap,f_sort,f_div};
int soak_run(FILE*o,int iters){uint64_t tot=0;for(int it=0;it<iters;it++){uint64_t h=0;for(unsigned k=0;k<sizeof tab/sizeof*tab;k++){unsigned idx=(unsigned)((rnd()+k)%(sizeof tab/sizeof*tab));h=h*131^tab[idx]();}tot^=h;fprintf(o,"%d %016llx %016llx\n",it,(unsigned long long)h,(unsigned long long)tot);fflush(o);}return 0;}
