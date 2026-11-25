#include "simdjson.h"
#include <iostream>
#include <sstream>
#include <cstdio>
#include <string>
#include <cstdint>
using namespace simdjson;
static uint64_t fnv(const std::string&s,uint64_t h=1469598103934665603ull){for(unsigned char c:s){h^=c;h*=1099511628211ull;}return h;}
static uint64_t cnt[8];
static void walk(dom::element e,uint64_t&h){
  switch(e.type()){
   case dom::element_type::ARRAY: cnt[0]++; for(auto x:dom::array(e)) walk(x,h); break;
   case dom::element_type::OBJECT: cnt[1]++; for(auto kv:dom::object(e)){ h=fnv(std::string(kv.key),h); walk(kv.value,h);} break;
   case dom::element_type::INT64: cnt[2]++; h=fnv(std::to_string(int64_t(e)),h); break;
   case dom::element_type::UINT64: cnt[3]++; h=fnv(std::to_string(uint64_t(e)),h); break;
   case dom::element_type::DOUBLE: {cnt[4]++; double d=double(e); uint64_t b; memcpy(&b,&d,8); h=fnv(std::to_string(b),h);} break;
   case dom::element_type::STRING: cnt[5]++; h=fnv(std::string(std::string_view(e)),h); break;
   case dom::element_type::BOOL: cnt[6]++; h=fnv(bool(e)?"t":"f",h); break;
   case dom::element_type::NULL_VALUE: cnt[7]++; h=fnv("n",h); break; }
}
int main(){
  std::stringstream ss; ss<<std::cin.rdbuf(); std::string in=ss.str();
  printf("impl=%s\n", get_active_implementation()->name().c_str());
  printf("valid_utf8=%d\n", (int)validate_utf8(in.data(),in.size()));
  padded_string ps(in);
  // minify
  auto buf=std::make_unique<char[]>(in.size()+64); size_t ml=0;
  auto er=minify(in.data(),in.size(),buf.get(),ml);
  printf("minify err=%d len=%zu hash=%016llx\n",(int)er,ml,(unsigned long long)fnv(std::string(buf.get(),ml)));
  dom::parser p; dom::element doc; 
  if(p.parse(ps).get(doc)){ // maybe ndjson
    dom::document_stream ds; size_t n=0; uint64_t h=0;
    if(!p.parse_many(ps).get(ds)){ for(auto d:ds){ if(d.error()) {printf("many err\n");break;} dom::element e=d.value(); walk(e,h); n++; } }
    printf("docs=%zu hash=%016llx\n",n,(unsigned long long)h);
  } else { uint64_t h=0; walk(doc,h); printf("dom hash=%016llx\n",(unsigned long long)h);
    printf("raw=%016llx\n",(unsigned long long)fnv(simdjson::minify(doc))); }
  for(int i=0;i<8;i++) printf("c%d=%llu\n",i,(unsigned long long)cnt[i]);
  // ondemand
  ondemand::parser op; auto od=op.iterate(ps); size_t objs=0; 
  if(!od.error()){ auto t=od.type(); printf("od type=%d\n",t.error()?-1:(int)t.value()); }
  return 0; }
