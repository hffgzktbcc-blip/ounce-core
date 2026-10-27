// ounce-core Ampere SASS -> WGSL, step 1 (fragment).
// Community-sourced decoding, not a hardware-accurate translation.
//   MOV R0, 0x3f800000
//   MOV R1, 0x3f000000
//   MOV R2, 0x0
//   MOV R3, 0x3f800000
//   EXIT

struct FsIn {
    @builtin(position) frag_pos: vec4<f32>,
};

struct FsOut {
    @location(0) c0: vec4<f32>,
};

@fragment
fn main(fin: FsIn) -> FsOut {
    var out: FsOut;
    out.c0 = vec4<f32>(1.0f, 0.5f, 0.0f, 1.0f);
    return out;
}
