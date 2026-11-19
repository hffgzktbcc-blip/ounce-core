/* elf-workload linalg: modular matrix multiplication and exponentiation over GF(p),
 * Gaussian elimination and matrix inversion over GF(p), and fraction-free Bareiss integer
 * determinant (cross-checked against Vandermonde product and Laplace cofactor expansion).
 * Provenance: original work written for ounce-core (MIT); textbook linear algebra. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

typedef __int128 i128;

#define MOD1 1000000007ULL
#define MOD2 998244353ULL

static uint64_t mod_pow(uint64_t b, uint64_t e, uint64_t p) {
    uint64_t r = 1 % p;
    b %= p;
    while (e) {
        if (e & 1u) r = (r * b) % p;
        b = (b * b) % p;
        e >>= 1;
    }
    return r;
}

static void mat_mul_mod(int n, const uint64_t a[8][8], const uint64_t b[8][8], uint64_t out[8][8], uint64_t p) {
    uint64_t tmp[8][8] = {{0}};
    for (int i = 0; i < n; i++) {
        for (int k = 0; k < n; k++) {
            uint64_t aik = a[i][k];
            if (!aik) continue;
            for (int j = 0; j < n; j++) {
                tmp[i][j] = (tmp[i][j] + aik * b[k][j]) % p;
            }
        }
    }
    memcpy(out, tmp, sizeof tmp);
}

static int test_mod_matmul(FILE *o) {
    int fails = 0;
    enum { N = 6 };
    uint64_t A[8][8] = {{0}}, B[8][8] = {{0}}, C[8][8] = {{0}};
    uint64_t AB[8][8], ABC1[8][8], BC[8][8], ABC2[8][8];
    uint32_t rng = 0x71727374u;
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            rng = rng * 1664525u + 1013904223u;
            A[i][j] = rng % MOD1;
            rng = rng * 1664525u + 1013904223u;
            B[i][j] = rng % MOD1;
            rng = rng * 1664525u + 1013904223u;
            C[i][j] = rng % MOD1;
        }
    }
    mat_mul_mod(N, A, B, AB, MOD1);
    mat_mul_mod(N, AB, C, ABC1, MOD1);
    mat_mul_mod(N, B, C, BC, MOD1);
    mat_mul_mod(N, A, BC, ABC2, MOD1);
    int assoc = (memcmp(ABC1, ABC2, sizeof ABC1) == 0);

    /* Matrix exponentiation for Fibonacci: [[1, 1], [1, 0]]^50 -> F_50 = 12586269025 mod MOD1 = 586269025 */
    uint64_t fib_base[8][8] = {{0}}, fib_pow[8][8] = {{0}};
    fib_base[0][0] = 1; fib_base[0][1] = 1; fib_base[1][0] = 1;
    fib_pow[0][0] = 1; fib_pow[1][1] = 1;
    uint64_t e = 50;
    while (e) {
        if (e & 1u) mat_mul_mod(2, fib_pow, fib_base, fib_pow, MOD1);
        mat_mul_mod(2, fib_base, fib_base, fib_base, MOD1);
        e >>= 1;
    }
    uint64_t f50 = fib_pow[0][1];
    if (!assoc || f50 != (12586269025ULL % MOD1)) fails++;
    fprintf(o, "mod_matmul: assoc=%d fib50_mod=%llu\n", assoc, (unsigned long long)f50);
    return fails;
}

static int test_gauss_gf_p(FILE *o) {
    int fails = 0;
    enum { N = 6 };
    uint64_t orig[8][8] = {{0}}, inv[8][8] = {{0}}, aug[6][12];
    uint64_t b[6], x[6];
    uint32_t rng = 0x11223344u;
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            rng = rng * 1664525u + 1013904223u;
            orig[i][j] = 1 + (rng % (MOD2 - 1));
            aug[i][j] = orig[i][j];
            aug[i][N + j] = (i == j) ? 1 : 0;
        }
        rng = rng * 1664525u + 1013904223u;
        b[i] = rng % MOD2;
    }

    /* Gauss-Jordan inversion over GF(MOD2) */
    int rank = 0;
    for (int col = 0; col < N; col++) {
        int piv = -1;
        for (int r = rank; r < N; r++) {
            if (aug[r][col] != 0) { piv = r; break; }
        }
        if (piv < 0) continue;
        for (int j = 0; j < 2 * N; j++) {
            uint64_t t = aug[rank][j]; aug[rank][j] = aug[piv][j]; aug[piv][j] = t;
        }
        uint64_t inv_p = mod_pow(aug[rank][col], MOD2 - 2, MOD2);
        for (int j = 0; j < 2 * N; j++) aug[rank][j] = (aug[rank][j] * inv_p) % MOD2;
        for (int r = 0; r < N; r++) {
            if (r == rank || aug[r][col] == 0) continue;
            uint64_t factor = aug[r][col];
            for (int j = 0; j < 2 * N; j++) {
                uint64_t sub = (factor * aug[rank][j]) % MOD2;
                aug[r][j] = (aug[r][j] + MOD2 - sub) % MOD2;
            }
        }
        rank++;
    }
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) inv[i][j] = aug[i][N + j];
    }

    uint64_t prod[8][8] = {{0}};
    mat_mul_mod(N, orig, inv, prod, MOD2);
    int is_id = (rank == N);
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            if (prod[i][j] != (uint64_t)(i == j)) is_id = 0;
        }
    }

    /* Solve A*x = b as x = A^-1 * b and verify A*x == b */
    for (int i = 0; i < N; i++) {
        uint64_t s = 0;
        for (int j = 0; j < N; j++) s = (s + inv[i][j] * b[j]) % MOD2;
        x[i] = s;
    }
    int ax_ok = 1;
    for (int i = 0; i < N; i++) {
        uint64_t s = 0;
        for (int j = 0; j < N; j++) s = (s + orig[i][j] * x[j]) % MOD2;
        if (s != b[i]) ax_ok = 0;
    }
    if (!is_id || !ax_ok) fails++;
    fprintf(o, "gauss_gf_p: rank=%d is_id=%d ax_ok=%d\n", rank, is_id, ax_ok);
    return fails;
}

/* ---- Bareiss exact integer determinant ---- */
static i128 bareiss_det(int n, const int64_t src[6][6]) {
    i128 a[6][6];
    for (int i = 0; i < n; i++) for (int j = 0; j < n; j++) a[i][j] = src[i][j];
    int sign = 1;
    i128 prev = 1;
    for (int k = 0; k < n - 1; k++) {
        if (a[k][k] == 0) {
            int swap_r = -1;
            for (int r = k + 1; r < n; r++) {
                if (a[r][k] != 0) { swap_r = r; break; }
            }
            if (swap_r < 0) return 0;
            for (int j = k; j < n; j++) {
                i128 t = a[k][j]; a[k][j] = a[swap_r][j]; a[swap_r][j] = t;
            }
            sign = -sign;
        }
        for (int i = k + 1; i < n; i++) {
            for (int j = k + 1; j < n; j++) {
                a[i][j] = (a[i][j] * a[k][k] - a[i][k] * a[k][j]) / prev;
            }
        }
        prev = a[k][k];
    }
    return sign * a[n - 1][n - 1];
}

static i128 laplace_det(int n, const int64_t m[6][6]) {
    if (n == 1) return m[0][0];
    if (n == 2) return (i128)m[0][0] * m[1][1] - (i128)m[0][1] * m[1][0];
    i128 det = 0;
    for (int col = 0; col < n; col++) {
        int64_t sub[6][6] = {{0}};
        for (int i = 1; i < n; i++) {
            int sc = 0;
            for (int j = 0; j < n; j++) {
                if (j != col) sub[i - 1][sc++] = m[i][j];
            }
        }
        i128 term = (i128)m[0][col] * laplace_det(n - 1, sub);
        det += (col & 1) ? -term : term;
    }
    return det;
}

static int test_bareiss(FILE *o) {
    int fails = 0;
    /* Vandermonde matrix V(1, 2, 3, 4, 5): det = prod_{i<j}(x_j - x_i) = 1*2*3*4 * 1*2*3 * 1*2 * 1 = 288 */
    int64_t V[6][6] = {{0}};
    for (int i = 0; i < 5; i++) {
        int64_t p = 1;
        for (int j = 0; j < 5; j++) { V[i][j] = p; p *= (i + 1); }
    }
    i128 dv = bareiss_det(5, V);

    /* Zero pivot swap + dense integer 5x5 compared with Laplace cofactor expansion */
    const int64_t M[6][6] = {
        {0, 3, -2, 5, 1, 0},
        {4, 0, 7, -1, 2, 0},
        {-3, 5, 2, 0, -4, 0},
        {2, -1, 4, 3, 6, 0},
        {1, 2, -5, 4, 3, 0},
        {0, 0, 0, 0, 0, 0}
    };
    i128 dm_b = bareiss_det(5, M);
    i128 dm_l = laplace_det(5, M);

    /* Singular 5x5 where row 4 = row 0 + 2 * row 1 */
    int64_t S[6][6];
    memcpy(S, M, sizeof S);
    for (int j = 0; j < 5; j++) S[4][j] = S[0][j] + 2 * S[1][j];
    i128 ds = bareiss_det(5, S);

    if (dv != 288 || dm_b != dm_l || ds != 0) fails++;
    fprintf(o, "bareiss: vandermonde=%lld dense=%lld laplace=%lld singular=%lld\n",
            (long long)dv, (long long)dm_b, (long long)dm_l, (long long)ds);
    return fails;
}

int workload_run(FILE *o) {
    int fails = 0;
    fails += test_mod_matmul(o);
    fails += test_gauss_gf_p(o);
    fails += test_bareiss(o);
    fprintf(o, "fails=%d\n", fails);
    return fails;
}
