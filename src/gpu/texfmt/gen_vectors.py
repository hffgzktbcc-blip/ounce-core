# Regenerates vectors.rs: python3 gen_vectors.py vectors.rs
# Needs `pip install texture2ddecoder astc-encoder-py` (test oracles only, never linked into ounce-core).
import texture2ddecoder as t, random, sys
import astc_encoder as arm

def arm_astc(d, W, H, bw, bh):
    # ARM astcenc (the ASTC reference codec), one block at a time, laid out like texture2ddecoder.
    n = W // bw
    out = bytearray(W * H * 4)
    for k in range(n):
        ctx = arm.ASTCContext(arm.ASTCConfig(arm.ASTCProfile.LDR, bw, bh, 1, 60, 0))
        img = ctx.decompress(d[16 * k:16 * k + 16], arm.ASTCImage(arm.ASTCType.U8, bw, bh, 1), arm.ASTCSwizzle.from_str('RGBA'))
        px = bytes(img.data)
        for y in range(bh):
            for x in range(bw):
                p = (y * W + k * bw + x) * 4
                r, g, b, a = px[(y * bw + x) * 4:(y * bw + x) * 4 + 4]
                out[p:p + 4] = bytes([b, g, r, a])
    return bytes(out)
random.seed(239)
N = 40
def rb(n): return bytes(random.getrandbits(8) for _ in range(n))
def bgra2rgba(o): return b''.join(bytes([o[i+2],o[i+1],o[i],o[i+3]]) for i in range(0,len(o),4))
def blocks(dec, data, bsz, bw, bh, count):
    # decode a row of `count` blocks; return per-block texel lists (row-major inside block)
    W = bw*count
    out = bgra2rgba(dec(data, W, bh))
    res=[]
    for k in range(count):
        px=b''
        for y in range(bh):
            for x in range(bw):
                p=(y*W + k*bw + x)*4
                px+=out[p:p+4]
        res.append(px)
    return res
sets = []
def add(name, code, bsz, bw, bh, gen, dec, signed=False, keep=lambda px:True, count=N):
    vecs=[]
    tries=0
    while len(vecs)<count and tries<20000:
        tries+=1
        b=gen()
        px=blocks(dec,b,bsz,bw,bh,1)[0]
        if keep(px): vecs.append((b,px))
    sets.append((name, code, bw, bh, signed, vecs))
add("BC1", 0x24, 8, 4,4, lambda: rb(8), t.decode_bc1)
def bc3gen():
    # BC3 colour blocks always use four colours; the reference honours c0 <= c1, so keep c0 > c1.
    b=bytearray(rb(16))
    c0=int.from_bytes(b[8:10],'little'); c1=int.from_bytes(b[10:12],'little')
    if c0==c1: c0^=1
    if c0<c1: c0,c1=c1,c0
    b[8:10]=c0.to_bytes(2,'little'); b[10:12]=c1.to_bytes(2,'little'); return bytes(b)
add("BC3", 0x26, 16, 4,4, bc3gen, t.decode_bc3)
add("BC4", 0x27, 8, 4,4, lambda: rb(8), t.decode_bc4)
add("BC5", 0x28, 16, 4,4, lambda: rb(16), t.decode_bc5)
m7=[0]
def bc7gen():
    b=bytearray(rb(16)); m=m7[0]%8; m7[0]+=1
    b[0]=(b[0] & (0xff << (m+1)) & 0xff) | (1<<m); return bytes(b)
add("BC7", 0x17, 16, 4,4, bc7gen, t.decode_bc7, count=64)
modes=[0x00,0x01,0x02,0x06,0x0a,0x0e,0x12,0x16,0x1a,0x1e,0x03,0x07,0x0b,0x0f]
m6=[0]
def bc6gen():
    b=bytearray(rb(16)); m=modes[m6[0]%14]; m6[0]+=1
    if m<2: b[0]=(b[0]&~3)|m
    else: b[0]=(b[0]&~0x1f)|m
    return bytes(b)
add("BC6H_UF16", 0x11, 16, 4,4, bc6gen, lambda d,w,h: t.decode_bc6(d,w,h), count=56)
add("ETC2_RGB", 0x06, 8, 4,4, lambda: rb(8), t.decode_etc2)
add("ETC2_RGB_PTA", 0x0a, 8, 4,4, lambda: rb(8), t.decode_etc2a1)
add("ETC2_RGBA", 0x0b, 16, 4,4, lambda: rb(16), t.decode_etc2a8)
add("EAC", 0x19, 8, 4,4, lambda: rb(8), t.decode_eacr)
add("EAC_SNORM", 0x19, 8, 4,4, lambda: rb(8), t.decode_eacr_signed, signed=True)
add("EACX2", 0x1a, 16, 4,4, lambda: rb(16), t.decode_eacrg)
# Builds random ASTC blocks that are legal for the LDR profile, so the reference (which does not
# reject illegal blocks) and the decoder under test must agree on every texel.
def _bm(m):
    bit=lambda n:(m>>n)&1
    if m&3:
        r=((m&3)<<1)|bit(4); a=(m>>5)&3; b=(m>>7)&3; k=(m>>2)&3
        if k==0: w,h=b+4,a+2
        elif k==1: w,h=b+8,a+2
        elif k==2: w,h=a+2,b+8
        elif bit(8)==0: w,h=a+2,(b&1)+6
        else: w,h=(b&1)+2,a+2
        return w,h,r,bit(9),bit(10)
    if (m>>2)&3==0: return None
    r=(((m>>2)&3)<<1)|bit(4); a=(m>>5)&3; k=(m>>7)&3
    if k==0: return 12,a+2,r,bit(9),bit(10)
    if k==1: return a+2,12,r,bit(9),bit(10)
    if k==2: return a+6,((m>>9)&3)+6,r,0,0
    if a==0: return 6,10,r,bit(9),bit(10)
    if a==1: return 10,6,r,bit(9),bit(10)
    return None
_WR={0:[(0,0,1),(1,0,0),(0,0,2),(0,1,0),(1,0,1),(0,0,3)],1:[(0,1,1),(1,0,2),(0,0,4),(0,1,2),(1,0,3),(0,0,5)]}
def _ise(t,q,b,n): return n*b+((8*n+4)//5 if t else (7*n+2)//3 if q else 0)
_CR=[(0,0,1),(1,0,0),(0,0,2),(0,1,0),(1,0,1),(0,0,3),(0,1,1),(1,0,2),(0,0,4),(0,1,2),(1,0,3),(0,0,5),(0,1,3),(1,0,4),(0,0,6),(0,1,4),(1,0,5),(0,0,7),(0,1,5),(1,0,6),(0,0,8)]
_LDR={0:[0,1],1:[4,5,6],2:[8,9,10],3:[12,13]}
def astc_valid(bw,bh):
    while True:
        v=random.getrandbits(128)
        m=v&0x7ff
        if m&0x1ff==0x1fc: continue
        g=_bm(m)
        if not g: continue
        w,h,r,hi,dual=g
        if r<2: continue
        t,q,b=_WR[hi][r-2]; n=w*h*(1+dual)
        if w>bw or h>bh or n>64: continue
        wb=_ise(t,q,b,n)
        if not 24<=wb<=96: continue
        parts=random.randint(1,4 if not dual else 3)
        v&=~(3<<11); v|=(parts-1)<<11
        extra=0
        if parts==1:
            cems=[random.choice([0,1,4,5,6,8,9,10,12,13])]
            v&=~(0xf<<13); v|=cems[0]<<13; start=17
        else:
            start=29
            if random.random()<0.5:
                c=random.choice([0,1,4,5,6,8,9,10,12,13]); cems=[c]*parts
                v&=~(0x3f<<23); v|=(c<<25)
            else:
                base=random.randint(0,2)
                cems=[]; cs=[]; ms=[]
                for p in range(parts):
                    cl=random.randint(0,1); c=random.choice(_LDR[base+cl]); cems.append(c); cs.append(cl); ms.append(c&3)
                val=0
                for p in range(parts): val|=cs[p]<<p; val|=ms[p]<<(parts+2*p)
                extra=3*parts-4
                v&=~(0x3f<<23); v|=((base+1)<<23)|((val&0xf)<<25)
                pos=128-wb-extra
                v&=~(((1<<extra)-1)<<pos); v|=(val>>4)<<pos
        end=128-wb-extra-(2 if dual else 0)
        nc=sum(2*((c>>2)+1) for c in cems)
        if nc>18 or end<start: continue
        fits=[cr for cr in _CR if _ise(cr[0],cr[1],cr[2],nc)<=end-start]
        if not fits: continue
        cr=fits[-1]; lv=(1<<cr[2])*(3 if cr[0] else 5 if cr[1] else 1)
        if lv<6: continue
        return v.to_bytes(16,'little')

for (code,bw,bh) in [(0x40,4,4),(0x50,5,4),(0x41,5,5),(0x42,6,6),(0x52,8,6),(0x44,8,8),(0x53,10,8),(0x45,10,10),(0x54,12,10),(0x46,12,12)]:
    dec=lambda d,w,h,bw=bw,bh=bh: arm_astc(d,w,h,bw,bh)
    notmag=lambda px: any(px[i:i+4]!=b'\xff\x00\xff\xff' for i in range(0,len(px),4))
    add(f"ASTC_{bw}X{bh}", code, 16, bw,bh, lambda bw=bw,bh=bh: astc_valid(bw,bh), dec, count=12)
    add(f"ASTC_{bw}X{bh}_ERR", code, 16, bw,bh, lambda: rb(16), dec, keep=lambda px: not notmag(px), count=3)
o=open(sys.argv[1],'w')
o.write("//! Reference vectors: random blocks decoded by independent decoders: `texture2ddecoder` (MIT) for\n//! BCn/ETC2/EAC and ARM `astcenc` (Apache-2.0, via astc-encoder-py) for ASTC, used only to produce\n//! these expectations. Generated by `gen_vectors.py`;\n//! regenerate rather than hand-edit.\n\n")
o.write("/// (name, COMPONENTS_SIZES, block w, block h, snorm, [(block hex, RGBA8 texels hex)])\n")
o.write("pub(super) const SETS: &[(&str, u32, u32, u32, bool, &[(&str, &str)])] = &[\n")
for name,code,bw,bh,signed,vecs in sets:
    o.write(f'    ("{name}", 0x{code:02x}, {bw}, {bh}, {"true" if signed else "false"}, &[\n')
    for b,px in vecs:
        o.write(f'        ("{b.hex()}", "{px.hex()}"),\n')
    o.write("    ]),\n")
o.write("];\n")
for s in sets: print(s[0], len(s[5]), file=sys.stderr)
