/* Monocypher 4.0.3 differential cases. Published vectors (RFC 7693, 7748, 8032,
   8439, 9106) are checked against fixed expected hex; every result is also
   printed so the output can be compared byte-for-byte with the host build. */
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include "monocypher.h"
#include "monocypher-ed25519.h"

static int fails;
static void hexp(FILE*o,const char*l,const uint8_t*p,size_t n){
  fprintf(o,"%s=",l); for(size_t i=0;i<n;i++) fprintf(o,"%02x",p[i]); fputc('\n',o);
}
static int unhex(uint8_t*out,const char*h){
  size_t n=strlen(h)/2; for(size_t i=0;i<n;i++){unsigned v;sscanf(h+2*i,"%2x",&v);out[i]=(uint8_t)v;} return (int)n;
}
static void verdict(FILE*o,const char*name,int ok){ fprintf(o,"%s: %s\n",name,ok?"PASS":"FAIL"); if(!ok)fails++; }
/* print and compare against published hex */
static int vec(FILE*o,const char*l,const uint8_t*got,size_t n,const char*exp){
  uint8_t e[256]; hexp(o,l,got,n); unhex(e,exp); return memcmp(got,e,n)==0;
}
static void fill(uint8_t*p,size_t n,uint32_t s){ for(size_t i=0;i<n;i++){ s=s*1664525u+1013904223u; p[i]=(uint8_t)(s>>24);} }

static void t_blake2b(FILE*o){
  int ok=1; uint8_t h[64];
  crypto_blake2b(h,64,(const uint8_t*)"abc",3);
  ok&=vec(o,"blake2b-abc",h,64,"ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923");
  static const size_t hl[]={20,32,48,64}, ml[]={0,3,128,129,255};
  crypto_blake2b_ctx g; crypto_blake2b_init(&g,32);
  uint8_t in[256],key[64];
  for(int a=0;a<4;a++) for(int b=0;b<5;b++){
    fill(in,256,a*10+b); uint8_t out[64];
    crypto_blake2b(out,hl[a],in,ml[b]); crypto_blake2b_update(&g,out,hl[a]);
    fill(key,64,99+a); crypto_blake2b_keyed(out,hl[a],key,hl[a],in,ml[b]); crypto_blake2b_update(&g,out,hl[a]);
  }
  uint8_t gh[32]; crypto_blake2b_final(&g,gh); hexp(o,"blake2b-grand",gh,32);
  uint8_t s1[64],s2[64]; fill(in,256,5); crypto_blake2b(s1,64,in,256);
  crypto_blake2b_ctx c; crypto_blake2b_init(&c,64);
  crypto_blake2b_update(&c,in,1); crypto_blake2b_update(&c,in+1,127); crypto_blake2b_update(&c,in+128,128); crypto_blake2b_final(&c,s2);
  ok&=memcmp(s1,s2,64)==0;
  verdict(o,"BLAKE2b",ok);
}

static void t_argon2(FILE*o){
  int ok=1;
  /* RFC 9106 5.3 Argon2id: m=32 KiB t=3 p=4, with secret and associated data */
  uint8_t pw[32],salt[16],sec[8],ad[12],tag[32];
  memset(pw,1,32); memset(salt,2,16); memset(sec,3,8); memset(ad,4,12);
  uint8_t *work=malloc(64*1024);
  crypto_argon2_config cfg={CRYPTO_ARGON2_ID,32,3,4};
  crypto_argon2_inputs in={pw,salt,32,16};
  crypto_argon2_extras ex={sec,ad,8,12};
  crypto_argon2(tag,32,work,cfg,in,ex);
  ok&=vec(o,"argon2id-rfc9106",tag,32,"0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659");
  static const int al[]={CRYPTO_ARGON2_D,CRYPTO_ARGON2_I,CRYPTO_ARGON2_ID};
  for(int a=0;a<3;a++){
    crypto_argon2_config c2={al[a],64,2,1};
    crypto_argon2_inputs i2={(const uint8_t*)"password",(const uint8_t*)"somesalt",8,8};
    uint8_t t2[32]; char l[24]; snprintf(l,sizeof l,"argon2-var%d",a);
    crypto_argon2(t2,32,work,c2,i2,crypto_argon2_no_extras); hexp(o,l,t2,32);
  }
  /* wrong password gives a different tag */
  crypto_argon2_inputs iw={(const uint8_t*)"passwore",salt,8,8};
  crypto_argon2_config c3={CRYPTO_ARGON2_ID,64,2,1};
  uint8_t ta[32],tb[32];
  crypto_argon2_inputs ia={(const uint8_t*)"password",salt,8,8};
  crypto_argon2(ta,32,work,c3,ia,crypto_argon2_no_extras);
  crypto_argon2(tb,32,work,c3,iw,crypto_argon2_no_extras);
  ok&=memcmp(ta,tb,32)!=0;
  free(work);
  verdict(o,"Argon2",ok);
}

static void t_x25519(FILE*o){
  int ok=1; uint8_t a[32],u[32],r[32];
  unhex(a,"a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4");
  unhex(u,"e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c");
  crypto_x25519(r,a,u);
  ok&=vec(o,"x25519-rfc7748-5.2",r,32,"c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");
  uint8_t ska[32],skb[32],pka[32],pkb[32],s1[32],s2[32];
  unhex(ska,"77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
  unhex(skb,"5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
  crypto_x25519_public_key(pka,ska); crypto_x25519_public_key(pkb,skb);
  ok&=vec(o,"x25519-pub-alice",pka,32,"8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
  ok&=vec(o,"x25519-pub-bob",pkb,32,"de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
  crypto_x25519(s1,ska,pkb); crypto_x25519(s2,skb,pka);
  ok&=vec(o,"x25519-shared",s1,32,"4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
  ok&=memcmp(s1,s2,32)==0;
  uint8_t skc[32],s3[32]; fill(skc,32,77); crypto_x25519(s3,skc,pkb);
  hexp(o,"x25519-wrongkey",s3,32); ok&=memcmp(s1,s3,32)!=0;
  /* RFC 7748 5.2 iterated, 1000 rounds */
  uint8_t k[32]={9},uu[32]={9},t[32];
  for(int i=0;i<1000;i++){ crypto_x25519(t,k,uu); memcpy(uu,k,32); memcpy(k,t,32); }
  ok&=vec(o,"x25519-iter1000",k,32,"684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51");
  verdict(o,"X25519",ok);
}

static void t_eddsa(FILE*o){
  int ok=1; uint8_t seed[32],sk[64],pk[32],sig[64];
  unhex(seed,"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
  crypto_ed25519_key_pair(sk,pk,seed);
  ok&=vec(o,"ed25519-pk1",pk,32,"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
  crypto_ed25519_sign(sig,sk,(const uint8_t*)"",0);
  ok&=vec(o,"ed25519-sig1",sig,64,"e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
  ok&=crypto_ed25519_check(sig,pk,(const uint8_t*)"",0)==0;
  unhex(seed,"4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb");
  crypto_ed25519_key_pair(sk,pk,seed);
  ok&=vec(o,"ed25519-pk2",pk,32,"3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c");
  uint8_t m=0x72; crypto_ed25519_sign(sig,sk,&m,1);
  ok&=vec(o,"ed25519-sig2",sig,64,"92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00");
  ok&=crypto_ed25519_check(sig,pk,&m,1)==0;
  uint8_t bad[64]; memcpy(bad,sig,64); bad[10]^=1;
  int r1=crypto_ed25519_check(bad,pk,&m,1); fprintf(o,"eddsa-tampered-sig-rejected=%d\n",r1!=0); ok&=r1!=0;
  uint8_t m2=0x73; int r2=crypto_ed25519_check(sig,pk,&m2,1); fprintf(o,"eddsa-wrong-msg-rejected=%d\n",r2!=0); ok&=r2!=0;
  uint8_t sk2[64],pk2[32],s2[32]; fill(s2,32,3); crypto_ed25519_key_pair(sk2,pk2,s2);
  int r3=crypto_ed25519_check(sig,pk2,&m,1); fprintf(o,"eddsa-wrong-key-rejected=%d\n",r3!=0); ok&=r3!=0;
  uint8_t msg[300]; crypto_blake2b_ctx g; crypto_blake2b_init(&g,32);
  static const size_t ml[]={0,1,63,64,65,127,128,129,300};
  for(int i=0;i<9;i++){
    fill(msg,300,i); crypto_ed25519_sign(sig,sk2,msg,ml[i]);
    crypto_blake2b_update(&g,sig,64); ok&=crypto_ed25519_check(sig,pk2,msg,ml[i])==0;
    uint8_t esk[64],epk[32],esig[64]; crypto_eddsa_key_pair(esk,epk,s2); crypto_eddsa_sign(esig,esk,msg,ml[i]);
    crypto_blake2b_update(&g,esig,64); ok&=crypto_eddsa_check(esig,epk,msg,ml[i])==0;
    esig[5]^=1; ok&=crypto_eddsa_check(esig,epk,msg,ml[i])!=0;
  }
  uint8_t gh[32]; crypto_blake2b_final(&g,gh); hexp(o,"ed25519-grand",gh,32);
  verdict(o,"EdDSA",ok);
}

static void t_aead(FILE*o){
  int ok=1; uint8_t key[32],nonce[12],out[128],tag[16];
  for(int i=0;i<32;i++)key[i]=i;
  /* RFC 8439 2.4.2 ChaCha20 */
  const char*pt="Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
  size_t n=strlen(pt); unhex(nonce,"000000000000004a00000000");
  crypto_chacha20_ietf(out,(const uint8_t*)pt,n,key,nonce,1);
  ok&=vec(o,"chacha20-rfc8439-head",out,16,"6e2e359a2568f98041ba0728dd0d6981");
  hexp(o,"chacha20-rfc8439-all",out,n);
  /* RFC 8439 2.5.2 Poly1305 */
  uint8_t pkey[32]; unhex(pkey,"85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b");
  crypto_poly1305(tag,(const uint8_t*)"Cryptographic Forum Research Group",34,pkey);
  ok&=vec(o,"poly1305-rfc8439",tag,16,"a8061dc1305136c6c22b8baf0c0127a9");
  /* XChaCha20-Poly1305 AEAD roundtrip and failure cases */
  uint8_t n24[24],ad[20],p[100],c[100],back[100],mac[16];
  fill(n24,24,1); fill(ad,20,2); fill(p,100,3);
  crypto_aead_lock(c,mac,key,n24,ad,20,p,100);
  hexp(o,"aead-ct",c,100); hexp(o,"aead-mac",mac,16);
  int r=crypto_aead_unlock(back,mac,key,n24,ad,20,c,100);
  ok&=r==0&&memcmp(back,p,100)==0;
  uint8_t k2[32]; memcpy(k2,key,32); k2[0]^=1;
  r=crypto_aead_unlock(back,mac,k2,n24,ad,20,c,100); fprintf(o,"aead-wrong-key-rejected=%d\n",r!=0); ok&=r!=0;
  uint8_t ct2[100]; memcpy(ct2,c,100); ct2[50]^=0x80;
  r=crypto_aead_unlock(back,mac,key,n24,ad,20,ct2,100); fprintf(o,"aead-tampered-ct-rejected=%d\n",r!=0); ok&=r!=0;
  uint8_t mac2[16]; memcpy(mac2,mac,16); mac2[15]^=1;
  r=crypto_aead_unlock(back,mac2,key,n24,ad,20,c,100); fprintf(o,"aead-tampered-mac-rejected=%d\n",r!=0); ok&=r!=0;
  uint8_t ad2[20]; memcpy(ad2,ad,20); ad2[0]^=1;
  r=crypto_aead_unlock(back,mac,key,n24,ad2,20,c,100); fprintf(o,"aead-tampered-ad-rejected=%d\n",r!=0); ok&=r!=0;
  verdict(o,"ChaCha20-Poly1305",ok);
}

static void t_verify(FILE*o){
  int ok=1; uint8_t a[64],b[64];
  fill(a,64,1); memcpy(b,a,64);
  ok&=crypto_verify16(a,b)==0&&crypto_verify32(a,b)==0&&crypto_verify64(a,b)==0;
  for(int i=0;i<64;i++){ b[i]^=0x01;
    int r64=crypto_verify64(a,b), r32=crypto_verify32(a,b), r16=crypto_verify16(a,b);
    ok&=r64!=0&&(i<32?r32!=0:r32==0)&&(i<16?r16!=0:r16==0);
    b[i]^=0x01; }
  fprintf(o,"verify-single-bit-all=%d\n",ok);
  uint8_t w[16]; fill(w,16,9); crypto_wipe(w,16); int z=1; for(int i=0;i<16;i++) z&=w[i]==0;
  ok&=z; fprintf(o,"wipe=%d\n",z);
  verdict(o,"constant-time-compare",ok);
}

int mc_run_cases(FILE*o){
  fails=0; fprintf(o,"monocypher 4.0.3 harness\n");
  t_blake2b(o); t_argon2(o); t_x25519(o); t_eddsa(o); t_aead(o); t_verify(o);
  fprintf(o,"TOTAL FAILS=%d\n",fails); return fails;
}
