/* TinyExpr differential cases. Deterministic text only; doubles printed with %.17g. */
#include <stdio.h>
#include <string.h>
#include <math.h>
#include "tinyexpr.h"

static FILE *O; /* compared byte-for-byte with the host build */
static FILE *P; /* every value at %.17g; host libm and newlib libm may differ in the last digit */
static int loose;

/* Expressions that reach libm (sin, exp, pow, ...) differ between the host libm and the guest's
   newlib in the last bit, which is not an emulator question. Those print 13 digits to O. Everything
   else is exact IEEE arithmetic and prints 17 digits. P always has 17 digits. */
static int uses_libm(const char *e) {
  static const char *k[] = {"sin", "cos", "tan", "exp", "ln", "log", "pow", "^", "**"};
  for (unsigned i = 0; i < sizeof k / sizeof *k; i++) if (strstr(e, k[i])) return 1;
  return 0;
}

static void num(const char *label, double v) {
  fprintf(P, "%s = %.17g\n", label, v);
  if (loose) fprintf(O, "%s = %.13g\n", label, v); else fprintf(O, "%s = %.17g\n", label, v);
}

static void interp(const char *e) {
  int err = -1;
  double v = te_interp(e, &err);
  char lab[256];
  snprintf(lab, sizeof lab, "%-40s", e);
  loose = uses_libm(e);
  if (err) { fprintf(O, "%s error at %d\n", lab, err); fprintf(P, "%s error at %d\n", lab, err); }
  else num(lab, v);
}

static double my_sum3(double a, double b, double c) { return a + b + c; }
static double my_scale(void *ctx, double a) { return a * *(double *)ctx; }

int tinyexpr_run_cases(FILE *o, FILE *p) {
  O = o; P = p;
  /* precedence, associativity, unary */
  const char *basic[] = {
    "1", "1+1", "1+2*3", "(1+2)*3", "2*3+4", "10-4-3", "100/10/5", "2^3^2", "2**3", "-2^2", "(-2)^2",
    "--3", "-(-3)", "+5", "2*-3", "2^-1", "1/3", "2/3", "10%3", "10%4", "-10%3", "7.5%2", "5%0", "1/0", "-1/0", "0/0",
    "0.1+0.2", "1e3", "1e-3", "1.5e+2", ".5", "5.", "1e308*10", "1e-308/1e10", "123456789*987654321",
    "9007199254740992+1", "((((((((1))))))))", "1+(2+(3+(4+(5+(6+(7+8))))))", "2 * ( 3 + 4 ) - 5 / ( 6 - 7 )",
    "1,2", "pi", "e", "pi*2", "e^2", "pi/e", "  3  +  4  ", "3+4*2/(1-5)^2^3"
  };
  for (unsigned i = 0; i < sizeof basic / sizeof *basic; i++) interp(basic[i]);

  /* functions */
  const char *fn[] = {
    "abs(-3.5)", "abs(0)", "ceil(1.2)", "ceil(-1.2)", "floor(1.8)", "floor(-1.8)",
    "sqrt(2)", "sqrt(16)", "sqrt(0)", "sqrt(-1)", "sqrt(1e300)",
    "exp(0)", "exp(1)", "exp(-1)", "exp(10)", "exp(100)", "exp(710)",
    "ln(1)", "ln(e)", "ln(10)", "ln(0)", "ln(-1)", "log(100)", "log10(1000)", "log10(2)",
    "fac(0)", "fac(1)", "fac(5)", "fac(10)", "fac(20)", "fac(25)", "fac(170)", "fac(171)", "fac(-1)",
    "ncr(5,2)", "ncr(10,5)", "ncr(52,5)", "ncr(5,6)", "npr(5,2)", "npr(10,3)",
    "pow(2,10)", "pow(2,0.5)", "pow(-8,1/3)", "pow(0,0)", "pow(10,-3)", "pow(2,1000)", "pow(2,2000)",
    "sin(0)", "sin(1)", "sin(pi/2)", "sin(pi)", "sin(100)", "sin(1e6)", "sin(-0.5)",
    "cos(0)", "cos(1)", "cos(pi)", "cos(pi/2)", "cos(100)", "cos(1e6)",
    "tan(0)", "tan(1)", "tan(pi/4)", "tan(pi/2)", "tan(100)",
    "asin(0)", "asin(0.5)", "asin(1)", "asin(2)", "acos(0)", "acos(0.5)", "acos(-1)", "acos(2)",
    "atan(0)", "atan(1)", "atan(1e10)", "atan2(1,1)", "atan2(-1,-1)", "atan2(0,-1)", "atan2(1,0)",
    "sinh(0)", "sinh(1)", "sinh(10)", "cosh(0)", "cosh(1)", "cosh(10)", "tanh(0)", "tanh(1)", "tanh(20)",
    "sin(cos(tan(1)))", "exp(ln(7))", "sqrt(pow(3,2)+pow(4,2))", "sin(pi/6)^2+cos(pi/6)^2",
    "atan(1)*4", "e^pi", "pi^e", "ln(2)/ln(10)", "sinh(1)^2-cosh(1)^2",
    "floor(ceil(2.5)/2)", "abs(sin(pi))<1e-15", "1<2", "2<1", "1<=1", "1>=2", "1==1", "1!=1"
  };
  for (unsigned i = 0; i < sizeof fn / sizeof *fn; i++) interp(fn[i]);

  /* errors and error positions */
  const char *bad[] = {
    "", " ", "(", ")", "1+", "+", "*2", "2*", "1 2", "(1", "1)", "((1)+2", "1+*2", "1++", "foo", "foo(1)", "sin", "sin()",
    "sin(1,2)", "pow(1)", "pow(1,2,3)", "pow(,)", "atan2(1)", "1 + $", "1..2", "1e", "3 4 5", "2 +  + ", "pi(2)", "abs(", "ncr(1,)", "@", "1+2)"
  };
  for (unsigned i = 0; i < sizeof bad / sizeof *bad; i++) interp(bad[i]);

  /* variables, compile once and evaluate many times */
  {
    double x = 0, y = 0, z = 3;
    te_variable vars[] = {{"x", &x, TE_VARIABLE, 0}, {"y", &y, TE_VARIABLE, 0}, {"z", &z, TE_VARIABLE, 0}};
    const char *ex[] = {"x*x+y*y", "sqrt(x^2+y^2)", "sin(x)*cos(y)+z", "(x+1)*(y-1)/(z+x)", "pow(x,y)", "atan2(y,x)", "x+yy", "x y"};
    for (unsigned k = 0; k < sizeof ex / sizeof *ex; k++) {
      int err = -1;
      te_expr *n = te_compile(ex[k], vars, 3, &err);
      if (!n) { fprintf(O, "compile %-24s error at %d\n", ex[k], err); continue; }
      fprintf(O, "compile %-24s ok\n", ex[k]);
      for (int i = 0; i < 6; i++) {
        x = i * 0.5 - 1.0; y = i * i * 0.25 + 0.125; z = 3 - i;
        char lab[160];
        snprintf(lab, sizeof lab, "  x=%.17g y=%.17g z=%.17g ->", x, y, z);
        loose = uses_libm(ex[k]);
        num(lab, te_eval(n));
      }
      te_free(n);
    }
    /* loop accumulation */
    te_expr *n = te_compile("sin(x)*sin(x)+cos(x)*cos(x)", vars, 1, NULL);
    double acc = 0;
    for (int i = 0; i < 1000; i++) { x = i * 0.01; acc += te_eval(n); }
    loose = 1; num("loop acc", acc);
    te_free(n);
    n = te_compile("exp(x)-pow(e,x)", vars, 1, NULL);
    acc = 0;
    for (int i = 0; i < 200; i++) { x = i * 0.1 - 10; acc += te_eval(n); }
    loose = 1; num("loop exp-pow", acc);
    te_free(n);
  }

  /* user-defined functions and closures */
  {
    double a = 1, b = 2, k = 2.5;
    te_variable vars[] = {{"a", &a, TE_VARIABLE, 0}, {"b", &b, TE_VARIABLE, 0},
                          {"sum3", my_sum3, TE_FUNCTION3 | TE_FLAG_PURE, 0},
                          {"scale", my_scale, TE_CLOSURE1, &k},
                          {"mysqrt", sqrt, TE_FUNCTION1 | TE_FLAG_PURE, 0}};
    const char *ex[] = {"sum3(a,b,1)", "scale(a+b)", "scale(scale(2))", "mysqrt(a*8)+sum3(1,2,3)", "sum3(a,b)", "scale()"};
    for (unsigned i = 0; i < sizeof ex / sizeof *ex; i++) {
      int err = -1;
      te_expr *n = te_compile(ex[i], vars, 5, &err);
      if (!n) fprintf(O, "user %-28s error at %d\n", ex[i], err);
      else { char lab[96]; snprintf(lab, sizeof lab, "user %-28s", ex[i]); loose = 0; num(lab, te_eval(n)); te_free(n); }
    }
  }

  /* long expression */
  {
    char buf[4096]; int n = 0;
    n += snprintf(buf + n, sizeof buf - n, "0");
    for (int i = 1; i <= 120; i++) n += snprintf(buf + n, sizeof buf - n, "+%d*%d/%d", i, i + 1, i + 2);
    interp(buf);
    n = snprintf(buf, sizeof buf, "1");
    for (int i = 0; i < 200; i++) n += snprintf(buf + n, sizeof buf - n, "*(1.01)");
    interp(buf);
    n = 0;
    for (int i = 0; i < 100; i++) buf[n++] = '(';
    buf[n++] = '7';
    for (int i = 0; i < 100; i++) buf[n++] = ')';
    buf[n] = 0;
    interp(buf);
  }
  fprintf(O, "done\n");
  return 0;
}
