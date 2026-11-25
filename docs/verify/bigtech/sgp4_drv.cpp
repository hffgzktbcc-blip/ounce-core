#include <iostream>
#include <string>
#include <vector>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include "SGP4.h"
#include "Tle.h"
#include "Eci.h"
int main(){
  std::string l; std::vector<std::string> v; int n=0,ok=0;
  while(std::getline(std::cin,l)){ if(l.size()>60 && (l[0]=='1'||l[0]=='2')) v.push_back(l.substr(0,69)); }
  for(size_t i=0;i+1<v.size();i+=2){
    try{ libsgp4::Tle t("sat",v[i],v[i+1]); libsgp4::SGP4 s(t);
      for(int m=0;m<=4320;m+=120){ n++;
        try{ libsgp4::Eci e=s.FindPosition((double)m); auto p=e.Position(); auto vv=e.Velocity();
          uint64_t b[6]; double d[6]={p.x,p.y,p.z,vv.x,vv.y,vv.z}; memcpy(b,d,48);
          printf("%zu %d %016llx %016llx %016llx %016llx %016llx %016llx\n",i/2,m,(unsigned long long)b[0],(unsigned long long)b[1],(unsigned long long)b[2],(unsigned long long)b[3],(unsigned long long)b[4],(unsigned long long)b[5]); ok++; }
        catch(std::exception&ex){ printf("%zu %d EXC %s\n",i/2,m,ex.what()); }
      }
    }catch(std::exception&ex){ printf("%zu TLE-EXC %s\n",i/2,ex.what()); }
  }
  printf("points=%d ok=%d\n",n,ok); return 0; }
