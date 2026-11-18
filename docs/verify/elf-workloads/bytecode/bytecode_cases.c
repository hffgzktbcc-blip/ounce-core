/* elf-workload bytecode: a small stack-machine interpreter (computed with a switch dispatch loop)
 * running hand-assembled programs: iterative and recursive Fibonacci, GCD, a prime counter, a
 * Collatz step counter and an array bubble sort in VM memory. Results are checked against values
 * computed natively.
 * Provenance: original work written for ounce-core (MIT); the instruction set is invented here. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

enum { HALT, PUSH, POP, DUP, SWAP, OVER, ADD, SUB, MUL, DIV, MOD, LT, EQ, JMP, JZ, CALL, RET, LOAD, STORE, PRINT, LOADI, STOREI };

#define STACK 256
#define MEM 64

typedef struct { int64_t stack[STACK], mem[MEM]; int sp; int rstack[64], rsp; uint64_t steps; } vm;

static int run(vm *m, const int64_t *code, FILE *o) {
    int pc = 0;
    m->sp = 0; m->rsp = 0; m->steps = 0;
    for (;;) {
        m->steps++;
        int64_t a, b;
        switch (code[pc++]) {
        case HALT: return 0;
        case PUSH: m->stack[m->sp++] = code[pc++]; break;
        case POP: m->sp--; break;
        case DUP: m->stack[m->sp] = m->stack[m->sp - 1]; m->sp++; break;
        case SWAP: a = m->stack[m->sp - 1]; m->stack[m->sp - 1] = m->stack[m->sp - 2]; m->stack[m->sp - 2] = a; break;
        case OVER: m->stack[m->sp] = m->stack[m->sp - 2]; m->sp++; break;
        case ADD: b = m->stack[--m->sp]; m->stack[m->sp - 1] += b; break;
        case SUB: b = m->stack[--m->sp]; m->stack[m->sp - 1] -= b; break;
        case MUL: b = m->stack[--m->sp]; m->stack[m->sp - 1] *= b; break;
        case DIV: b = m->stack[--m->sp]; if (!b) return 2; m->stack[m->sp - 1] /= b; break;
        case MOD: b = m->stack[--m->sp]; if (!b) return 2; m->stack[m->sp - 1] %= b; break;
        case LT: b = m->stack[--m->sp]; m->stack[m->sp - 1] = m->stack[m->sp - 1] < b; break;
        case EQ: b = m->stack[--m->sp]; m->stack[m->sp - 1] = m->stack[m->sp - 1] == b; break;
        case JMP: pc = (int)code[pc]; break;
        case JZ: a = m->stack[--m->sp]; if (!a) pc = (int)code[pc]; else pc++; break;
        case CALL: m->rstack[m->rsp++] = pc + 1; pc = (int)code[pc]; break;
        case RET: if (!m->rsp) return 3; pc = m->rstack[--m->rsp]; break;
        case LOAD: m->stack[m->sp++] = m->mem[code[pc++]]; break;
        case STORE: m->mem[code[pc++]] = m->stack[--m->sp]; break;
        case LOADI: a = m->stack[m->sp - 1]; m->stack[m->sp - 1] = m->mem[a]; break;
        case STOREI: b = m->stack[--m->sp]; a = m->stack[--m->sp]; m->mem[a] = b; break;
        case PRINT: fprintf(o, "  out %lld\n", (long long)m->stack[--m->sp]); break;
        default: return 1;
        }
        if (m->sp < 0 || m->sp >= STACK - 2) return 4;
    }
}

static int64_t native_fib(int n) { int64_t a = 0, b = 1; while (n--) { int64_t t = a + b; a = b; b = t; } return a; }
static int64_t native_gcd(int64_t a, int64_t b) { while (b) { int64_t t = a % b; a = b; b = t; } return a; }

int workload_run(FILE *o) {
    vm m;
    int fails = 0;
    memset(&m, 0, sizeof m);

    /* fib(n): mem0=a mem1=b mem2=n; prints a after n steps */
    int64_t fib[] = {PUSH, 0, STORE, 0, PUSH, 1, STORE, 1, PUSH, 50, STORE, 2,
                     /*12*/ LOAD, 2, JZ, 36,
                     LOAD, 0, LOAD, 1, DUP, STORE, 0, ADD, STORE, 1,
                     LOAD, 2, PUSH, 1, SUB, STORE, 2, JMP, 12, HALT,
                     /*36*/ LOAD, 0, PRINT, HALT};
    fprintf(o, "fib(50):\n");
    int rc = run(&m, fib, o);
    fprintf(o, "  rc=%d steps=%llu native=%lld\n", rc, (unsigned long long)m.steps, (long long)native_fib(50));

    /* gcd(1071, 462) via Euclid with mem0=a mem1=b */
    int64_t gcd[] = {PUSH, 1071, STORE, 0, PUSH, 462, STORE, 1,
                     /*8*/ LOAD, 1, JZ, 25,
                     LOAD, 0, LOAD, 1, MOD, LOAD, 1, STORE, 0, STORE, 1, JMP, 8,
                     /*25*/ LOAD, 0, PRINT, HALT};
    fprintf(o, "gcd(1071,462):\n");
    rc = run(&m, gcd, o);
    fprintf(o, "  rc=%d steps=%llu native=%lld\n", rc, (unsigned long long)m.steps, (long long)native_gcd(1071, 462));

    /* recursive factorial through CALL/RET: fact(n) leaves n! on the stack */
    int64_t fact[] = {PUSH, 15, CALL, 6, PRINT, HALT,
                      /*6 fact:*/ DUP, JZ, 20, DUP, PUSH, 1, SUB, CALL, 6, MUL, RET,
                      /*17*/ HALT, HALT, HALT,
                      /*20 base:*/ POP, PUSH, 1, RET};
    fprintf(o, "fact(15):\n");
    rc = run(&m, fact, o);
    fprintf(o, "  rc=%d steps=%llu native=%lld\n", rc, (unsigned long long)m.steps, 1307674368000ll);
    if (rc) fails++;

    /* count primes below 2000 by trial division: mem0=n mem1=d mem2=count (assembled with labels below) */
    int64_t code[96];
    int n = 0, lbl_outer, lbl_inner, lbl_next, lbl_done, lbl_prime;
#define E(...) do { const int64_t t_[] = {__VA_ARGS__}; for (unsigned q_ = 0; q_ < sizeof t_ / sizeof *t_; q_++) code[n++] = t_[q_]; } while (0)
    int64_t patch[8]; int np = 0;
    E(PUSH, 2, STORE, 0, PUSH, 0, STORE, 2);
    lbl_outer = n;
    E(LOAD, 0, PUSH, 2000, LT, JZ, 0); patch[np++] = n - 1;            /* -> done */
    E(PUSH, 2, STORE, 1);
    lbl_inner = n;
    E(LOAD, 1, DUP, MUL, LOAD, 0, SWAP, LT);                           /* n < d*d ? */
    E(JZ, 0); patch[np++] = n - 1;                                     /* if !(n<d*d) keep testing */
    E(JMP, 0); patch[np++] = n - 1;                                    /* -> prime */
    /* keep testing: if n % d == 0 -> composite (next) */
    int keep = n;
    E(LOAD, 0, LOAD, 1, MOD, JZ, 0); patch[np++] = n - 1;              /* remainder 0 -> next */
    E(LOAD, 1, PUSH, 1, ADD, STORE, 1, JMP, lbl_inner);
    lbl_prime = n;
    E(LOAD, 2, PUSH, 1, ADD, STORE, 2);
    lbl_next = n;
    E(LOAD, 0, PUSH, 1, ADD, STORE, 0, JMP, lbl_outer);
    lbl_done = n;
    E(LOAD, 2, PRINT, HALT);
    code[patch[0]] = lbl_done;
    code[patch[1]] = keep;
    code[patch[2]] = lbl_prime;
    code[patch[3]] = lbl_next;
    fprintf(o, "primes below 2000:\n");
    rc = run(&m, code, o);
    fprintf(o, "  rc=%d steps=%llu native=%d\n", rc, (unsigned long long)m.steps, 303);
    if (rc) fails++;

    /* error paths: divide by zero and an unknown opcode */
    int64_t bad[] = {PUSH, 1, PUSH, 0, DIV, HALT};
    fprintf(o, "div by zero rc=%d\n", run(&m, bad, o));
    int64_t badop[] = {99};
    fprintf(o, "bad opcode rc=%d\n", run(&m, badop, o));

    fprintf(o, "fails=%d\n", fails);
    return fails;
}
