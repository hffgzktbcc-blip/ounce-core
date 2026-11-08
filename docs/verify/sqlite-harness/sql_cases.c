#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include "sqlite3.h"
static uint32_t st=2024u;static unsigned rnd(void){st=st*1664525u+1013904223u;return st>>8;}
static int fails=0;
static void q(FILE*o,sqlite3*db,const char*label,const char*sql){
  sqlite3_stmt*s; int rc=sqlite3_prepare_v2(db,sql,-1,&s,0);
  if(rc){fprintf(o,"%s: prepare rc=%d\n",label,rc);fails++;return;}
  fprintf(o,"%s:",label);
  while((rc=sqlite3_step(s))==SQLITE_ROW){
    int n=sqlite3_column_count(s);
    for(int i=0;i<n;i++){const unsigned char*t=sqlite3_column_text(s,i);fprintf(o," %s",t?(const char*)t:"NULL");}
    fprintf(o," |");
  }
  fprintf(o," rc=%d\n",rc); if(rc!=SQLITE_DONE)fails++;
  sqlite3_finalize(s);
}
static void x(FILE*o,sqlite3*db,const char*label,const char*sql){
  char*e=0; int rc=sqlite3_exec(db,sql,0,0,&e);
  fprintf(o,"%s: rc=%d%s%s\n",label,rc,e?" ":"",e?e:""); sqlite3_free(e);
}
int sql_run_cases(FILE*o){
  sqlite3*db; int rc=sqlite3_open(":memory:",&db); fprintf(o,"open rc=%d\n",rc); if(rc)return 1;
  fprintf(o,"version %s\n",sqlite3_libversion());
  x(o,db,"create","CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER, v REAL, s TEXT);CREATE INDEX tk ON t(k);");
  sqlite3_stmt*ins; sqlite3_prepare_v2(db,"INSERT INTO t(k,v,s) VALUES(?,?,?)",-1,&ins,0);
  x(o,db,"begin","BEGIN");
  for(int i=0;i<3000;i++){
    char b[32]; unsigned r=rnd(); snprintf(b,sizeof b,"row-%u",r%977);
    sqlite3_bind_int(ins,1,(int)(r%100)); sqlite3_bind_double(ins,2,(double)(r%10007)/13.0); sqlite3_bind_text(ins,3,b,-1,SQLITE_TRANSIENT);
    if(sqlite3_step(ins)!=SQLITE_DONE)fails++; sqlite3_reset(ins);
  }
  sqlite3_finalize(ins); x(o,db,"commit","COMMIT");
  q(o,db,"count","SELECT count(*),sum(k),min(v),max(v),round(avg(v),6) FROM t");
  q(o,db,"group","SELECT k,count(*),round(sum(v),4) FROM t GROUP BY k ORDER BY k LIMIT 8");
  q(o,db,"order","SELECT id,s FROM t ORDER BY v DESC,id LIMIT 5");
  q(o,db,"like","SELECT count(*) FROM t WHERE s LIKE 'row-9%'");
  q(o,db,"join","SELECT a.k,count(*) FROM t a JOIN t b ON a.k=b.k AND b.id<=50 WHERE a.id<=200 GROUP BY a.k ORDER BY 2 DESC,1 LIMIT 5");
  q(o,db,"window","SELECT id,sum(k) OVER (ORDER BY id ROWS 3 PRECEDING) FROM t WHERE id<=6");
  q(o,db,"cte","WITH RECURSIVE f(n,a,b) AS (SELECT 1,0,1 UNION ALL SELECT n+1,b,a+b FROM f WHERE n<60) SELECT a FROM f WHERE n=60");
  q(o,db,"strfn","SELECT upper('Ounce'),substr('abcdef',2,3),replace('a-b-c','-','+'),length(hex(randomblob(0))),printf('%08.3f|%x|%5s',3.14159,255,'ab')");
  q(o,db,"int64","SELECT 9223372036854775807+0,9223372036854775807+1,-9223372036854775808,abs(-5),5/2,5%3,7.0/2");
  q(o,db,"float","SELECT 1e308*10,0.1+0.2,round(2.5),round(-2.5),cast('12abc' as integer),1.0/3");
  q(o,db,"json","SELECT json_extract('{\"a\":[1,2,{\"b\":7}]}','$.a[2].b'),json_array(1,'x',null)");
  x(o,db,"constraint","INSERT INTO t(id,k,v,s) VALUES(1,1,1,'dup')");
  x(o,db,"rollback","BEGIN;DELETE FROM t;ROLLBACK");
  q(o,db,"after-rollback","SELECT count(*) FROM t");
  q(o,db,"integrity","PRAGMA integrity_check");
  x(o,db,"syntax","SELEC 1");
  sqlite3_close(db);
  fprintf(o,"fails=%d\n",fails); return fails;
}
