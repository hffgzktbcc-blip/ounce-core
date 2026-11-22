# Paper checks

A paper check takes a published result, rebuilds it as a small ARM64 program, runs it on ounce-core and compares the output
with the numbers the paper or its published test data give. It shows that a paper's numbers come out of its method, to the precision each check states: exactly for known-answer vectors,
within a declared tolerance for a results table, and the same on every run.
It does **not** show that the paper's wider claims are right: a wrong method reproduces wrong numbers just as exactly.

This is one way ounce-core is used beyond homebrew. The programs are plain static Linux ARM64 ELF files, run with
`ounce-core run-elf`. Nothing here needs a console, firmware or keys.

## What is here

102 checks, one folder each. Run them all with `docs/verify/papers/run_all.sh <new out dir> target/release/ounce-core` (it took about 25 minutes on a loaded machine; the slowest single check is about 45 s).

| Check | Paper or specification | Authors | Year |
|---|---|---|---|
| [`accurate-sum/`](accurate-sum/) | Accurate Sum and Dot Product | Takeshi Ogita et al. | 2005 |
| [`adler32/`](adler32/) | ZLIB Compressed Data Format Specification version 3.3 (RFC 1950): the Adler-32 checksum | L. Peter Deutsch and Jean-Loup Gailly | 1996 |
| [`aes-ccm/`](aes-ccm/) | Counter with CBC-MAC (CCM), RFC 3610; origin: AES Encryption & Authentication Using CTR Mode & CBC-MAC (IEE... | Doug Whiting et al. | 2003 |
| [`aes-fips197/`](aes-fips197/) | AES Proposal: Rijndael (as standardised in FIPS 197, Advanced Encryption Standard) | Joan Daemen and Vincent Rijmen | 1998 |
| [`aes-siv/`](aes-siv/) | A Provable-Security Treatment of the Key-Wrap Problem (introduces SIV); AES instantiation specified in RFC... | Phillip Rogaway et al. | 2008 |
| [`aria/`](aria/) | New Block Cipher: ARIA (algorithm as specified in RFC 5794) | Daesung Kwon et al. | 2003 |
| [`ascon/`](ascon/) | Ascon v1.2: Lightweight Authenticated Encryption and Hashing | Christoph Dobraunig et al. | 2021 |
| [`atkin-bernstein-sieve/`](atkin-bernstein-sieve/) | Prime sieves using binary quadratic forms | A. O. L. Atkin and D. J. Bernstein | 2004 |
| [`base-rfc4648/`](base-rfc4648/) | RFC 4648: The Base16, Base32, and Base64 Data Encodings | Simon Josefsson | 2006 |
| [`bbp-pi-hex/`](bbp-pi-hex/) | On the rapid computation of various polylogarithmic constants (BBP formula); worked example and tables as s... | David H. Bailey et al. | 1997 |
| [`bech32/`](bech32/) | BIP-0173: Base32 address format for native v0-16 witness outputs, and BIP-0350: Bech32m format for v1+ witn... | Pieter Wuille and Greg Maxwell | 2020 |
| [`blake2/`](blake2/) | BLAKE2: simpler, smaller, fast as MD5 | Jean-Philippe Aumasson et al. | 2013 |
| [`blowfish/`](blowfish/) | Description of a New Variable-Length Key, 64-Bit Block Cipher (Blowfish) | Bruce Schneier | 1993 |
| [`brent-rho-multiplications/`](brent-rho-multiplications/) | An improved Monte Carlo factorization algorithm | Richard P. Brent | 1980 |
| [`camellia/`](camellia/) | Camellia: A 128-Bit Block Cipher Suitable for Multiple Platforms - Design and Analysis (algorithm as specif... | Kazumaro Aoki et al. | 2000 |
| [`cast128/`](cast128/) | Constructing Symmetric Ciphers Using the CAST Design Procedure (specified as CAST-128 in RFC 2144) | Carlisle M. Adams | 1997 |
| [`cbor/`](cbor/) | RFC 8949: Concise Binary Object Representation (CBOR), Appendix A (Examples of Encoded CBOR Data Items) | Carsten Bormann and Paul Hoffman | 2020 |
| [`chacha/`](chacha/) | ChaCha, a variant of Salsa20 | Daniel J. Bernstein | 2008 |
| [`chacha20-poly1305/`](chacha20-poly1305/) | ChaCha20 and Poly1305 for IETF Protocols (RFC 8439): ChaCha20 (Bernstein 2008) + Poly1305 (Bernstein 2005)... | Yoav Nir and Adam Langley | 2018 |
| [`cham/`](cham/) | CHAM: A Family of Lightweight Block Ciphers for Resource-Constrained Devices | Bonwook Koo et al. | 2017 |
| [`clefia/`](clefia/) | The 128-Bit Blockcipher CLEFIA | Taizo Shirai et al. | 2007 |
| [`cmac/`](cmac/) | The AES-CMAC Algorithm (RFC 4493; CMAC is the OMAC1 One-Key CBC-MAC of Iwata and Kurosawa, FSE 2003) | Junhyuk Song et al. | 2006 |
| [`collatz-records/`](collatz-records/) | Delay records of the 3x+1 (Collatz) problem: the published list of record holders and their step counts (ta... | Eric Roosendaal (table) | 2024 |
| [`crc-catalog/`](crc-catalog/) | A Painless Guide to CRC Error Detection Algorithms (the parametrised CRC model) and the catalogue of CRC al... | Ross N. Williams | 1993 |
| [`crc32c-iscsi/`](crc32c-iscsi/) | Internet Small Computer Systems Interface (iSCSI), RFC 3720: CRC32C digest (section 12.1) and CRC examples... | J. Satran et al. | 2004 |
| [`cuckoo-thresholds/`](cuckoo-thresholds/) | Tight Thresholds for Cuckoo Hashing via XORSAT | Martin Dietzfelbinger et al. | 2010 |
| [`deflate-huffman/`](deflate-huffman/) | RFC 1951: DEFLATE Compressed Data Format Specification version 1.3 | Peter Deutsch | 1996 |
| [`dsfmt/`](dsfmt/) | A PRNG specialized in double precision floating point numbers using an affine transition (dSFMT) | Mutsuo Saito and Makoto Matsumoto | 2009 |
| [`fast-invsqrt/`](fast-invsqrt/) | Improving the accuracy of the fast inverse square root algorithm | Cezary J. Walczyk et al. | 2018 |
| [`fnv/`](fnv/) | The FNV Non-Cryptographic Hash Algorithm (draft-eastlake-fnv-21) | Glenn Fowler et al. | 2024 |
| [`gift/`](gift/) | GIFT: A Small Present | Subhadeep Banik et al. | 2017 |
| [`goldberg-1991/`](goldberg-1991/) | What Every Computer Scientist Should Know About Floating-Point Arithmetic | David Goldberg | 1991 |
| [`granlund-montgomery-division/`](granlund-montgomery-division/) | Division by invariant integers using multiplication | Torbjorn Granlund and Peter L. Montgomery | 1994 |
| [`hight/`](hight/) | HIGHT: A New Block Cipher Suitable for Low-Resource Device | Deukjo Hong et al. | 2006 |
| [`highwayhash/`](highwayhash/) | Fast keyed hash/pseudo-random function using SIMD multiply and permute (HighwayHash) | Jyrki Alakuijala et al. | 2016 |
| [`hmac/`](hmac/) | Keying Hash Functions for Message Authentication (HMAC); test vectors from RFC 4231 | Mihir Bellare et al. | 1996 |
| [`hpack/`](hpack/) | RFC 7541: HPACK: Header Compression for HTTP/2, Appendix C (Examples) | Roberto Peon and Herve Ruellan | 2015 |
| [`internet-checksum/`](internet-checksum/) | Computing the Internet Checksum (RFC 1071), with the incremental update of RFC 1624 | Robert Braden et al. | 1988 |
| [`isaac/`](isaac/) | ISAAC and RC4 (the ISAAC pseudorandom number generator) | Robert J. Jenkins Jr. | 1996 |
| [`jaeschke-miller-rabin/`](jaeschke-miller-rabin/) | On strong pseudoprimes to several bases | Gerhard Jaeschke | 1993 |
| [`jump-hash/`](jump-hash/) | A Fast, Minimal Memory, Consistent Hash Algorithm | John Lamping and Eric Veach | 2014 |
| [`kahan-division-series/`](kahan-division-series/) | How Futile are Mindless Assessments of Roundoff in Floating-Point Computation? | William Kahan | 2006 |
| [`kahan-java-hurts/`](kahan-java-hurts/) | How Java's Floating-Point Hurts Everyone Everywhere | Joseph D. Darcy and William Kahan | 2004 |
| [`kahan-marketing-math/`](kahan-marketing-math/) | Marketing versus Mathematics | William Kahan | 2000 |
| [`kahan-needle-triangle/`](kahan-needle-triangle/) | Miscalculating Area and Angles of a Needle-like Triangle | William Kahan | 2014 |
| [`kahan-quadratic/`](kahan-quadratic/) | On the Cost of Floating-Point Computation Without Extra-Precise Arithmetic | William Kahan | 2004 |
| [`kahan-sin-monotone/`](kahan-sin-monotone/) | Mathematics Written in Sand | William Kahan | 1983 |
| [`kasumi/`](kasumi/) | 3GPP TS 35.202, Specification of the 3GPP Confidentiality and Integrity Algorithms; Document 2: KASUMI Spec... | 3GPP / ETSI SAGE (algorithm designed on the basis of MISTY1) | 2020 |
| [`keccak-f1600/`](keccak-f1600/) | The Keccak reference (version 3.0) / Keccak sponge function family main document | Guido Bertoni et al. | 2011 |
| [`knuth-ran-array/`](knuth-ran-array/) | The Art of Computer Programming, Volume 2: Seminumerical Algorithms, 3rd edition, Section 3.6 (ran_array, 2... | Donald E. Knuth | 2002 |
| [`lea/`](lea/) | LEA: A 128-Bit Block Cipher for Fast Encryption on Common Processors | Deukjo Hong et al. | 2013 |
| [`lookup3/`](lookup3/) | lookup3.c: hashlittle() and hashlittle2(), 32-bit hashes for hash table lookup | Bob Jenkins | 2006 |
| [`lucas-lehmer/`](lucas-lehmer/) | An extended theory of Lucas' functions (the Lucas-Lehmer test for Mersenne numbers) | D. H. Lehmer | 1930 |
| [`machin-pi/`](machin-pi/) | Machin's arctangent formula for pi (1706) and Stormer's four-term formula (1896), as used for hand and mach... | John Machin and Carl Stormer | 1706 |
| [`md4/`](md4/) | The MD4 Message-Digest Algorithm | Ronald L. Rivest | 1992 |
| [`md5/`](md5/) | The MD5 Message-Digest Algorithm | Ronald L. Rivest | 1992 |
| [`minstd/`](minstd/) | Random number generators: good ones are hard to find (the minimal standard generator) | Stephen K. Park and Keith W. Miller | 1988 |
| [`mrg32k3a/`](mrg32k3a/) | Good Parameters and Implementations for Combined Multiple Recursive Random Number Generators (MRG32k3a with... | Pierre L'Ecuyer | 1999 |
| [`mt19937-64/`](mt19937-64/) | Tables of 64-bit Mersenne Twisters | Takuji Nishimura | 2000 |
| [`mt19937/`](mt19937/) | Mersenne Twister: a 623-dimensionally equidistributed uniform pseudo-random number generator | Makoto Matsumoto and Takuji Nishimura | 1998 |
| [`muller-recurrence/`](muller-recurrence/) | How Futile are Mindless Assessments of Roundoff in Floating-Point Computation? | William Kahan | 2006 |
| [`murmur2/`](murmur2/) | MurmurHash2, MurmurHash2A, MurmurHash64A and MurmurHash64B and their SMHasher verification values | Austin Appleby | 2008 |
| [`murmur3/`](murmur3/) | MurmurHash3 (x86_32, x86_128, x64_128) and its SMHasher verification values | Austin Appleby | 2011 |
| [`nist-lls-longley-wampler/`](nist-lls-longley-wampler/) | Linear least squares test problems: J. W. Longley, 'An Appraisal of Least Squares Programs for the Electron... | James W. Longley et al. | 1967 |
| [`ocb/`](ocb/) | The OCB Authenticated-Encryption Algorithm (RFC 7253); OCB3 from Krovetz and Rogaway, 'The Software Perform... | Ted Krovetz and Phillip Rogaway | 2014 |
| [`pcg/`](pcg/) | PCG: A Family of Simple Fast Space-Efficient Statistically Good Algorithms for Random Number Generation | Melissa E. O'Neill | 2014 |
| [`philox/`](philox/) | Parallel random numbers: as easy as 1, 2, 3 | John K. Salmon et al. | 2011 |
| [`poly1305-aes/`](poly1305-aes/) | The Poly1305-AES message-authentication code | Daniel J. Bernstein | 2005 |
| [`psw-pseudoprimes/`](psw-pseudoprimes/) | The pseudoprimes to 25*10^9 | Carl Pomerance et al. | 1980 |
| [`punycode/`](punycode/) | RFC 3492: Punycode: A Bootstring encoding of Unicode for Internationalized Domain Names in Applications (IDNA) | Adam M. Costello | 2003 |
| [`rabbit/`](rabbit/) | Rabbit: A New High-Performance Stream Cipher | Martin Boesgaard et al. | 2003 |
| [`rabinowitz-wagon-spigot/`](rabinowitz-wagon-spigot/) | A spigot algorithm for the digits of pi | Stanley Rabinowitz and Stan Wagon | 1995 |
| [`ranlux/`](ranlux/) | A portable high-quality random number generator for lattice field theory simulations (RANLUX) | Martin Luscher | 1994 |
| [`rc5/`](rc5/) | The RC5 Encryption Algorithm | Ronald L. Rivest | 1994 |
| [`rc6/`](rc6/) | The RC6 Block Cipher | Ronald L. Rivest et al. | 1998 |
| [`rmd160/`](rmd160/) | RIPEMD-160: a strengthened version of RIPEMD | Hans Dobbertin et al. | 1996 |
| [`ryu/`](ryu/) | Ryu: fast float-to-string conversion | Ulf Adams | 2018 |
| [`sale-e-spigot/`](sale-e-spigot/) | The calculation of e to many significant digits | A. H. J. Sale | 1968 |
| [`salsa20/`](salsa20/) | Salsa20 specification | Daniel J. Bernstein | 2005 |
| [`seed/`](seed/) | The SEED Encryption Algorithm (RFC 4269; the cipher is the KISA/TTA standard TTAS.KO-12.0004, 1998) | Hyangjin Lee et al. | 2005 |
| [`serpent/`](serpent/) | Serpent: A Proposal for the Advanced Encryption Standard | Ross Anderson et al. | 1998 |
| [`sfmt19937/`](sfmt19937/) | SIMD-oriented Fast Mersenne Twister: a 128-bit Pseudorandom Number Generator | Mutsuo Saito and Makoto Matsumoto | 2008 |
| [`sha1/`](sha1/) | US Secure Hash Algorithm 1 (SHA1) | D. Eastlake 3rd and P. Jones | 2001 |
| [`sha2/`](sha2/) | Secure Hash Standard (SHS), SHA-224/256/384/512 | National Institute of Standards and Technology | 2011 |
| [`sha3/`](sha3/) | SHA-3 Standard: Permutation-Based Hash and Extendable-Output Functions | National Institute of Standards and Technology (Keccak designers: Bertoni, Daemen, Peeters, Van Assche) | 2015 |
| [`simeck/`](simeck/) | The Simeck Family of Lightweight Block Ciphers | Gangqiang Yang et al. | 2015 |
| [`simon/`](simon/) | The SIMON and SPECK Families of Lightweight Block Ciphers | Ray Beaulieu et al. | 2013 |
| [`siphash/`](siphash/) | SipHash: a fast short-input PRF | Jean-Philippe Aumasson and Daniel J. Bernstein | 2012 |
| [`skein/`](skein/) | The Skein Hash Function Family | Niels Ferguson et al. | 2010 |
| [`skinny/`](skinny/) | The SKINNY Family of Block Ciphers and its Low-Latency Variant MANTIS | Christof Beierle et al. | 2016 |
| [`skipjack/`](skipjack/) | SKIPJACK and KEA Algorithm Specifications (version 2.0), with the SP 800-17 known-answer tables | NIST (specification, 29 May 1998) | 1998 |
| [`sm3/`](sm3/) | The SM3 Cryptographic Hash Function (GB/T 32905-2016) | S. Shen et al. | 2018 |
| [`sm4/`](sm4/) | The SM4 Blockcipher Algorithm And Its Modes Of Operations (draft-ribose-cfrg-sm4), specifying GB/T 32907-2016 | Ronald Tse et al. | 2018 |
| [`speck/`](speck/) | The SIMON and SPECK Families of Lightweight Block Ciphers | Ray Beaulieu et al. | 2013 |
| [`streebog/`](streebog/) | GOST R 34.11-2012: Hash Function | V. Dolmatov (ed.) and A. Degtyarev | 2013 |
| [`threefry/`](threefry/) | Parallel random numbers: as easy as 1, 2, 3 (Threefry generators) | John K. Salmon et al. | 2011 |
| [`tinymt/`](tinymt/) | Tiny Mersenne Twister (TinyMT) 1.1.2: reference implementation and documentation (no peer-reviewed paper fe... | Mutsuo Saito and Makoto Matsumoto | 2011 |
| [`twofish/`](twofish/) | Twofish: A 128-Bit Block Cipher | Bruce Schneier et al. | 1998 |
| [`unicode-utf/`](unicode-utf/) | RFC 3629: UTF-8, a transformation format of ISO 10646 (section 7 examples); RFC 2781: UTF-16, an encoding o... | Francois Yergeau et al. | 2003 |
| [`xor-filter/`](xor-filter/) | Xor Filters: Faster and Smaller Than Bloom and Cuckoo Filters | Thomas Mueller Graf and Daniel Lemire | 2020 |
| [`xxhash32/`](xxhash32/) | xxHash algorithm specification, XXH32 (xxhash_spec.md version 0.2.0) | Yann Collet | 2023 |
| [`xxhash64/`](xxhash64/) | xxHash algorithm specification, XXH64 (xxhash_spec.md version 0.2.0) | Yann Collet | 2023 |

### How much each check proves

A check compares a program's output with published numbers. How strong that is differs, so read each folder's `paper.json`:

- **Where the numbers come from.** Most known-answer values are in the paper itself, an RFC, or the authors' own test files.
  Some are not. Several came from a secondary copy (for example Crypto++ or the Linux kernel's test tables), from OEIS, or
  from the C++ standard, because the paper's own site or PDF could not be fetched. `paper.json` field
  `expected_values_source` says which, and many `paper.json` files add a note.
- **Some are specifications, not papers.** RFCs, FIPS documents and format specs with published test vectors are included
  and are labelled as such. One folder (`tinymt`) has no paper at all, only the authors' reference output.
- **Some are thin.** A few checks compare very few published values (for example `knuth-ran-array` has one, `minstd` two).
  `paper.json` says how many.
- **Some tolerances were chosen after a first run.** Where that happened (for example `fast-invsqrt` and parts of
  `muller-recurrence`) the folder says so and prints both sets of numbers. It is a weaker kind of check than a tolerance
  fixed beforehand.
- **Where the paper disagrees with itself or with arithmetic**, the check asserts the discrepancy and records it. Examples:
  a misprinted root in a table of Kahan's, and a mismatch between Goldberg's text and its own footnote.
- **The instruction budget limits ranges.** `run-elf` stops a program after 2 billion instructions (about 36 million per
  second here), so some ranges are smaller than the paper's, for example Lucas-Lehmer up to exponent 1279 and the Atkin
  sieve up to 10^7.
- **The native comparison is x86-64 only.** Each check also builds the same source natively and requires identical output.
  No check has been run on Tier-4 or on native ARM64 hardware yet, and none is in CI.

Each folder has a `paper.json` (what is claimed, where the expected numbers come from, what is **not** checked), the program,
a `check.sh` that builds and runs it, and the expected output. Run one from the repository root:

```sh
cargo build --release --locked
docs/verify/papers/philox/check.sh /tmp/philox-check target/release/ounce-core
```

The output folder (`/tmp/philox-check` here) must not exist yet: `check.sh` refuses any path that does.

`check.sh` needs `clang` and `ld.lld` (or a Rust toolchain, which has `rust-lld`). If a host C compiler is present it also
tries to build the same source natively and requires identical output, so ounce-core's arithmetic is compared with the machine
it runs on. For Philox, only the statistical smoke program (`smoke.c`) is compared this way; the known-answer program
(`philox4x32.c`) is checked against the published vectors only. If that native build fails, the check prints "not compared" and can still pass, so read the last line: only
"identical to a native ... build" means the comparison happened. Both checks were run on x86-64 and Tier-3 only; Tier-4 and native ARM64 runs are not done yet.

## How to add one

1. **Pick a paper with numbers you can compare**: a table, known-answer vectors, or a bound a program can test. Read the paper
   text, not a summary, and take algorithms and numbers from it.
2. **Write the program yourself from the algorithm.** Do not copy code. Published numbers are data and may be copied as
   numbers; say where they came from. Do not vendor GPL code (for example TestU01).
3. **Fix the tolerance before the first run** and write it in `paper.json`. If the result is outside it, report that. Do not
   move the tolerance to fit.
4. **Add a negative control**: break one constant and show the check fails. A check that cannot fail proves nothing.
5. **List what is not checked** in `paper.json`: speed, wider claims, other variants. Emulator run time says nothing about the
   authors' hardware.
6. **Keep it deterministic**: fixed random seeds, no clock, no host files. The output must be the same on every run.
7. Record the output as `expected.txt` and require it byte for byte in `check.sh`.

The easy way: copy [`siphash/`](siphash/). The program includes `common/ounce_paper.h` (output helpers and the aarch64 entry
point, or stdio with `-DHOST`), defines `int paper_main(void)` and returns 0 only if every published value matched.
`check.sh` is three lines that call `paper_check` from `common/lib.sh`, which builds, runs on Tier-3, diffs `expected.txt` and
compares with a native build.

## What a pass means

The program, run on ounce-core, produces the paper's numbers. Evidence for the inputs tested, not proof for every input and
not an independent reproduction by someone else.
