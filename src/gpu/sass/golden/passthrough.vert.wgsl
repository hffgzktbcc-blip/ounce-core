// ounce-core SASS -> WGSL, step 1 (vertex).
// Community-sourced decoding, not a hardware-accurate translation.
//   ALD.4 R0, a[0x80]
//   ALD.4 R4, a[0x90]
//   AST.4 a[0x70], R0
//   AST.4 a[0x80], R4
//   EXIT

struct VsIn {
    @location(0) a0: vec4<f32>,
    @location(1) a1: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) o0: vec4<f32>,
};

@vertex
fn main(vin: VsIn) -> VsOut {
    var out: VsOut;
    let v0: f32 = vin.a0.x;
    let v1: f32 = vin.a0.y;
    let v2: f32 = vin.a0.z;
    let v3: f32 = vin.a0.w;
    let v4: f32 = vin.a1.x;
    let v5: f32 = vin.a1.y;
    let v6: f32 = vin.a1.z;
    let v7: f32 = vin.a1.w;
    out.position.x = v0;
    out.position.y = v1;
    out.position.z = v2;
    out.position.w = v3;
    out.o0.x = v4;
    out.o0.y = v5;
    out.o0.z = v6;
    out.o0.w = v7;
    return out;
}
