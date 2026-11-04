// ounce-core SASS -> WGSL, step 1 (fragment).
// Community-sourced decoding, not a hardware-accurate translation.
//   IPA.PASS R4, a[0x7c]
//   MUFU.RCP R4, R4
//   IPA R0, a[0x80], R4
//   IPA R1, a[0x84], R4
//   IPA R2, a[0x88], R4
//   IPA R3, a[0x8c], R4
//   EXIT

struct FsIn {
    @builtin(position) frag_pos: vec4<f32>,
    @location(0) @interpolate(perspective) l0: vec4<f32>,
};

struct FsOut {
    @location(0) c0: vec4<f32>,
};

@fragment
fn main(fin: FsIn) -> FsOut {
    var out: FsOut;
    let v0: f32 = fin.frag_pos.w;
    let v1: f32 = 1.0f / v0;
    let v2: f32 = (fin.l0.x * fin.frag_pos.w) * v1;
    let v3: f32 = (fin.l0.y * fin.frag_pos.w) * v1;
    let v4: f32 = (fin.l0.z * fin.frag_pos.w) * v1;
    let v5: f32 = (fin.l0.w * fin.frag_pos.w) * v1;
    out.c0 = vec4<f32>(v2, v3, v4, v5);
    return out;
}
