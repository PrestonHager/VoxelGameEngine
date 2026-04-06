struct Globals {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) instance_pos: vec3<f32>,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let world = vec4<f32>(v.pos + v.instance_pos, 1.0);
    var o: VsOut;
    o.clip_pos = globals.view_proj * world;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    return i.color;
}
