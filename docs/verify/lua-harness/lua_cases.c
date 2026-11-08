#include <stdio.h>
#include <string.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
static FILE*out;
static int l_print(lua_State*L){int n=lua_gettop(L);for(int i=1;i<=n;i++){size_t l;const char*s=luaL_tolstring(L,i,&l);fwrite(s,1,l,out);fputc(i<n?'\t':'\n',out);lua_pop(L,1);}return 0;}
static const char*SCRIPT =
"local function fib(n) if n<2 then return n end return fib(n-1)+fib(n-2) end\n"
"print('fib', fib(24))\n"
"print('int', 9223372036854775807+1, math.mininteger, 7//2, -7//2, 7%-3, 7.5//2, 1<<62, 1<<63, 1<<64, ~0, 5&3, 5|3, 5~3)\n"
"print('float', 1/0, -1/0, 0.1+0.2, 3.0, 2^53, 10//0.0, math.huge, string.format('%.17g', 0.1))\n"
"print('conv', 10==10.0, math.type(1), math.type(1.0), tostring(1e15), tostring(1e100), 0x7fffffffffffffff, tonumber('0x10'), tonumber('  12  '), tonumber('1e2'), tonumber('z',36))\n"
"print('math', string.format('%.12f %.12f %.12f %.12f %.12f', math.sin(1), math.cos(1), math.exp(1), math.log(10), math.sqrt(2)), math.floor(-3.5), math.ceil(-3.5), math.fmod(-7,3), math.tointeger(3.0), math.abs(math.mininteger))\n"
"math.randomseed(42); local r={} for i=1,5 do r[i]=math.random(1,1000) end print('rand', table.concat(r,','))\n"
"local t={} for i=1,200 do t[i]=(i*7919)%1009 end table.sort(t) print('sort', t[1],t[100],t[200], #t)\n"
"table.sort(t,function(a,b) return a>b end) print('sortdesc', t[1],t[200])\n"
"print('str', ('hello world'):upper(), ('abc'):rep(3,'-'), ('hello'):sub(2,-2), ('x'):byte(), string.char(72,105), #('\\0abc'))\n"
"print('fmt', string.format('%5d|%-5d|%05.1f|%x|%X|%o|%e|%g|%q|%s', 42, 42, 3.14159, 255, 255, 8, 12345.678, 0.0001, 'a\"b', {} ~= nil))\n"
"print('pat', ('key=value; k2=v2'):gsub('(%w+)=(%w+)', '%2=%1'), ('2024-11-08'):match('(%d+)-(%d+)-(%d+)'), ('  trim  '):match('^%s*(.-)%s*$'), ('a,b,,c'):find(',,',1,true))\n"
"local words={} for w in ('the quick brown fox'):gmatch('%a+') do words[#words+1]=w:upper() end print('gmatch', table.concat(words,' '))\n"
"print('pack', string.pack('<i4>i2z', 1, 2, 'hi'):byte(1,-1))\n"
"print('unpack', string.unpack('<I2 >I2 d', string.pack('<I2 >I2 d', 513, 513, 1.5)))\n"
"print('utf8', utf8.char(72, 228, 8364, 128512), utf8.len('häl€'), #utf8.char(128512))\n"
"local co=coroutine.wrap(function(a) local b=coroutine.yield(a+1) local c=coroutine.yield(b*2) return c..'!' end)\n"
"print('co', co(1), co(10), co('end'))\n"
"print('pcall', pcall(error, 'boom'), pcall(error, {code=7}), select(2, pcall(function() local x=nil; return x.y end)))\n"
"print('err', select(2,pcall(function() return 1+{} end)), select(2,pcall(string.rep,'x',-1)), (pcall(string.rep,'x',1<<40)))\n"
"local mt={__add=function(a,b) return a.v+b.v end,__index=function(t,k) return k..'?' end,__tostring=function() return 'OBJ' end,__call=function(self,x) return x*2 end,__len=function() return 99 end}\n"
"local o=setmetatable({v=5},mt) print('meta', o+o, o.zzz, tostring(o), o(21), #o)\n"
"local cl={} for i=1,3 do cl[i]=function() i=i+1 return i end end print('closure', cl[1](), cl[1](), cl[2](), cl[3]())\n"
"do local i=0 ::top:: i=i+1 if i<5 then goto top end print('goto', i) end\n"
"local s=0 for i=1,100000 do s=s+i%7*(i&3) end print('loop', s)\n"
"local a={} for i=1,1000 do a[#a+1]=tostring(i) end print('concat', #table.concat(a), select('#', table.unpack(a, 1, 200)))\n"
"print('tbl', table.concat({table.unpack({1,2,3,4,5}, 2, 4)}, ','), next({}), rawlen({1,2}), rawequal('a','a'))\n"
"collectgarbage() print('gc', collectgarbage('count') > 0)\n"
"print('load', load('return 1+1')(), load('syntax error here'))\n"
"print('done')\n";
int lua_run_cases(FILE*o){
  out=o; int fails=0;
  lua_State*L=luaL_newstate(); if(!L){fprintf(o,"newstate failed\n");return 1;}
  luaL_requiref(L,"_G",luaopen_base,1);lua_pop(L,1);
  luaL_requiref(L,"string",luaopen_string,1);lua_pop(L,1);
  luaL_requiref(L,"table",luaopen_table,1);lua_pop(L,1);
  luaL_requiref(L,"math",luaopen_math,1);lua_pop(L,1);
  luaL_requiref(L,"utf8",luaopen_utf8,1);lua_pop(L,1);
  luaL_requiref(L,"coroutine",luaopen_coroutine,1);lua_pop(L,1);
  lua_pushcfunction(L,l_print);lua_setglobal(L,"print");
  fprintf(o,"%s\n",LUA_RELEASE);
  if(luaL_dostring(L,SCRIPT)){fprintf(o,"script error: %s\n",lua_tostring(L,-1));fails++;}
  lua_close(L);
  fprintf(o,"fails=%d\n",fails); return fails;
}
