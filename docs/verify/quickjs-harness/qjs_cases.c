/* QuickJS differential cases: run fixed scripts through JS_Eval and print the string form of each result
   (or the exception text). No module loader, no std/os libraries, no clock or random sources are used by
   the scripts, so output depends only on the engine and libm. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "quickjs.h"

static const char *scripts[] = {
  /* arithmetic and number formatting */
  "1+2*3-4/5",
  "[0.1+0.2, 1/3, 2**53, -(2**53), 1e21, 1e-7, 123456789.125, 5e-324, 1.7976931348623157e308].join(' ')",
  "[(255).toString(16), (255).toString(2), (-255.5).toString(8), (0.1).toFixed(20), (1234.5678).toExponential(3), (1234.5678).toPrecision(6)].join(' ')",
  "[7%3, -7%3, 7%-3, 5.5%2, 2**-1, 2**0.5, 10**15, 0xff|0x100, 1<<31, 1<<32, -1>>>0, -1>>>28, ~5].join(' ')",
  "[parseInt('0x1f'), parseInt('  -42abc'), parseFloat('3.14e2xyz'), Number(''), Number(' 12 '), Number('1_0'), +[], +{}, +[5]].join(' ')",
  "[Math.floor(-1.5), Math.ceil(-1.5), Math.round(-1.5), Math.round(2.5), Math.trunc(-0.9), Math.sign(-3), Math.abs(-0)].join(' ')",
  "[Math.sin(1), Math.cos(1), Math.tan(1), Math.atan2(1,2), Math.exp(1), Math.log(10), Math.sqrt(2), Math.cbrt(27), Math.hypot(3,4)].map(x=>x.toPrecision(10)).join(' ')",
  "[Math.pow(2,10), Math.pow(2,0.5).toPrecision(12), Math.max(), Math.min(), Math.max(1,NaN), Math.fround(5.5), Math.clz32(1), Math.imul(65536,65536)].join(' ')",
  "[0/0, 1/0, -1/0, 0*-1, Object.is(0*-1,-0), NaN===NaN, isNaN('x'), Number.MAX_SAFE_INTEGER+2].join(' ')",
  /* BigInt */
  "(2n**200n).toString() + ' ' + (123456789012345678901234567890n*987654321098765432109876543210n) + ' ' + (-17n/5n) + ' ' + (-17n%5n) + ' ' + BigInt.asUintN(64,-1n)",
  "(function(){let a=1n;for(let i=1n;i<=60n;i++)a*=i;return a})().toString(36)",
  /* strings and unicode */
  "['abc'.toUpperCase(), 'ǅ'.toLowerCase(), 'ß'.toUpperCase(), 'İ'.toLowerCase().length, 'é'.normalize('NFD').length, '\\u{1F600}'.length, [...'a\\u{1F600}b'].length].join(' ')",
  "'the quick brown fox'.split(' ').map(w=>w[0].toUpperCase()+w.slice(1)).join('-') + ' ' + 'abcabc'.replace(/b/g,'XX') + ' ' + 'x'.padStart(5,'ab') + ' ' + 'a,b,,c'.split(',').length",
  "['\\u00e9'.codePointAt(0), String.fromCodePoint(0x1F600,0x41).length, 'abc'.localeCompare('abd'), 'a'<'B', encodeURIComponent('a b&c/é'), decodeURIComponent('%E2%82%AC')].join(' ')",
  /* regular expressions */
  "'2024-11-09T10:20:30'.match(/(\\d+)-(\\d+)-(\\d+)T(\\d+):(\\d+):(\\d+)/).slice(1).join('/')",
  "'aaa bbb  ccc'.replace(/(?<first>\\w)(\\w*)/g, (m,a,b,off,s,g)=>g.first.toUpperCase()+b)",
  "[/(?<=\\$)\\d+(\\.\\d+)?/.exec('cost $42.50')[0], /^(?:a|ab)+c$/.test('aabac'), 'x1y22z333'.match(/\\d+/g).join(','), /\\p{Lu}/u.test('É'), /[\\u{1F600}-\\u{1F64F}]/u.test('\\u{1F603}')].join(' ')",
  "(/(a+)+b/.test('a'.repeat(22)+'c')) + ' ' + 'aBc'.replace(/b/i,'-') + ' ' + /(\\d{2,4})\\1/.test('12341234') + ' ' + 'abc'.search(/c/)",
  /* objects, arrays, JSON */
  "JSON.stringify({a:[1,2,{b:null}],c:'x\\ny',d:undefined,e:()=>1,f:new Date(0),g:1e21,h:-0,i:NaN},null,1)",
  "JSON.stringify(JSON.parse('{\"x\":[1,2,3.5e2,true,null,{\"y\":\"\\\\u00e9\\\\ud83d\\\\ude00\"}]}'))",
  "(()=>{try{JSON.parse('{\"a\":1,}')}catch(e){return e.name+': '+e.message}})()",
  "[3,1,2,10,20,100].sort().join(',') + ' ' + [3,1,2,10,20,100].sort((a,b)=>a-b).join(',') + ' ' + [5,1,4].toSorted().join(',') + ' ' + [1,2,3,4,5].findLast(x=>x%2==0) + ' ' + [[1,[2]],[3]].flat(Infinity).join(',')",
  "(()=>{const a=[];for(let i=0;i<2000;i++)a.push((i*7919)%2003);a.sort((x,y)=>x-y);return a.slice(0,5).join(',')+' '+a[1999]+' '+a.reduce((s,x)=>s+x,0)})()",
  "(()=>{const o={};for(let i=0;i<500;i++)o['k'+(i*37%500)]=i;return Object.keys(o).slice(0,6).join(',')+' '+Object.keys(o).length})()",
  "(()=>{const m=new Map();for(let i=0;i<1000;i++)m.set(i*31%1009,i);const s=new Set([...m.keys()].map(x=>x%10));return m.size+' '+[...s].join('')+' '+[...m.entries()][5].join(':')})()",
  /* functions, classes, closures, generators, async */
  "(function f(n){return n<2?n:f(n-1)+f(n-2)})(22)",
  "(()=>{class A{#x=1;static s=5;get x(){return this.#x}inc(){this.#x++;return this}}class B extends A{inc(){super.inc();return super.inc()}}return new B().inc().x+' '+A.s})()",
  "(()=>{function*g(){let a=0,b=1;for(;;){yield a;[a,b]=[b,a+b]}}const r=[];for(const v of g()){if(v>1000)break;r.push(v)}return r.join(',')})()",
  "(()=>{const f=(...a)=>a.reduce((x,y)=>x+y,0);const {p,...rest}={p:1,q:2,r:3};return f(...[1,2,3])+' '+JSON.stringify(rest)+' '+((a,b=a*2)=>a+b)(3)})()",
  "(()=>{let out=[];Promise.resolve(1).then(v=>out.push('p'+v));(async()=>{out.push('a1');await null;out.push('a2')})();out.push('sync');return out.join(' ')})()",
  "(()=>{const s=Symbol('t');const o={[s]:1,a:2};return Object.getOwnPropertySymbols(o).length+' '+s.toString()+' '+typeof s+' '+Object.entries(o).length})()",
  "(()=>{function depth(n){return n==0?0:1+depth(n-1)}return depth(400)})()",
  "(()=>{try{(function r(){r()})()}catch(e){return e.name}})()",
  "(()=>{const p=new Proxy({}, {get:(t,k)=>'p:'+String(k)});return p.foo+' '+Reflect.ownKeys({a:1,[Symbol.iterator]:2}).length+' '+Object.getOwnPropertyNames('ab').join()})()",
  /* typed arrays, dates, misc */
  "(()=>{const b=new ArrayBuffer(16),v=new DataView(b);v.setFloat64(0,Math.PI);v.setInt32(8,-2,true);const u=new Uint8Array(b);return Array.from(u).map(x=>x.toString(16).padStart(2,'0')).join('')+' '+new Float32Array([1.1,2.5])[0]+' '+new Uint8ClampedArray([300,-5,1.5])+' '+new BigInt64Array(1).length})()",
  "(()=>{const d=new Date(Date.UTC(2024,1,29,12,34,56,789));return d.toISOString()+' '+d.getTime()+' '+new Date('2000-01-01T00:00:00Z').getUTCDay()+' '+new Date(NaN).getTime()+' '+Date.UTC(1970,0,1)})()",
  "(()=>{const a=new Uint32Array(1000);for(let i=0;i<1000;i++)a[i]=(i*2654435761)>>>0;let h=0;for(const x of a)h=(Math.imul(h,31)+x)|0;return h})()",
  "(()=>{let s=0;for(let i=0;i<200000;i++){s=(s+i*i%7)|0}return s})()",
  "(()=>{let s='';for(let i=0;i<300;i++)s+=String.fromCharCode(65+i%26);return s.length+' '+s.slice(-5)+' '+[...s].filter(c=>c=='A').length})()",
  "(()=>{const w=new WeakMap();const k={};w.set(k,1);return w.get(k)+' '+(typeof globalThis)+' '+(typeof queueMicrotask)+' '+Array.from({length:3},(_, i)=>i*i)})()",
  "(()=>{try{null.x}catch(e){return e.constructor.name+': '+e.message}})()",
  "(()=>{try{undefinedFunction()}catch(e){return e.name+': '+e.message}})()",
  "(()=>{try{new Array(-1)}catch(e){return e.name+': '+e.message}})()",
  "(()=>{let x=eval('1+1');return x+' '+new Function('a','b','return a*b')(6,7)})()",
  "var syntax error here",
  "throw new RangeError('boom')",
};

static void print_val(FILE *o, JSContext *ctx, JSValue v) {
  const char *s = JS_ToCString(ctx, v);
  if (s) { fprintf(o, "%s", s); JS_FreeCString(ctx, s); } else fprintf(o, "<unprintable>");
}

int qjs_run_cases(FILE *o) {
  int fails = 0;
  JSRuntime *rt = JS_NewRuntime();
  if (!rt) { fprintf(o, "JS_NewRuntime failed\n"); return 1; }
  JS_SetMemoryLimit(rt, 256 * 1024 * 1024);
  JS_SetMaxStackSize(rt, 512 * 1024);
  for (unsigned i = 0; i < sizeof scripts / sizeof *scripts; i++) {
    JSContext *ctx = JS_NewContext(rt);
    if (!ctx) { fprintf(o, "case %u: JS_NewContext failed\n", i); fails++; continue; }
    JSValue r = JS_Eval(ctx, scripts[i], strlen(scripts[i]), "<case>", JS_EVAL_TYPE_GLOBAL);
    fprintf(o, "case %u: ", i);
    if (JS_IsException(r)) {
      JSValue ex = JS_GetException(ctx);
      fprintf(o, "EXC "); print_val(o, ctx, ex);
      JSValue st = JS_GetPropertyStr(ctx, ex, "name"); fprintf(o, " [name="); print_val(o, ctx, st); fprintf(o, "]");
      JS_FreeValue(ctx, st); JS_FreeValue(ctx, ex);
    } else print_val(o, ctx, r);
    fprintf(o, "\n");
    JS_FreeValue(ctx, r);
    JSContext *jc; while (JS_ExecutePendingJob(rt, &jc) > 0) {}
    JS_FreeContext(ctx);
  }
  JS_FreeRuntime(rt);
  fprintf(o, "fails=%d\n", fails);
  return fails;
}
