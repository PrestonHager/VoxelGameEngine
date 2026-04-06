struct Globals {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) instance_pos: vec3<f32>,
    @location(3) instance_rot: vec3<f32>, // pitch, yaw, roll in radians
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let pitch = v.instance_rot.x;
    let yaw = v.instance_rot.y;
    let roll = v.instance_rot.z;

    let cx = cos(pitch);
    let sx = sin(pitch);
    let cy = cos(yaw);
    let sy = sin(yaw);
    let cz = cos(roll);
    let sz = sin(roll);

    let rx = mat3x3<f32>(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, cx, -sx),
        vec3<f32>(0.0, sx, cx)
    );
    let ry = mat3x3<f32>(
        vec3<f32>(cy, 0.0, sy),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(-sy, 0.0, cy)
    );
    let rz = mat3x3<f32>(
        vec3<f32>(cz, -sz, 0.0),
        vec3<f32>(sz, cz, 0.0),
        vec3<f32>(0.0, 0.0, 1.0)
    );

    // Rotate around cube center so it spins in place.
    let local = v.pos - vec3<f32>(0.5, 0.5, 0.5);
    let rotated = (rz * ry * rx) * local + vec3<f32>(0.5, 0.5, 0.5);
    let world = vec4<f32>(rotated + v.instance_pos, 1.0);
    var o: VsOut;
    o.clip_pos = globals.view_proj * world;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    return i.color;
}
