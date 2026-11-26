struct Cfg { width: u32, height: u32, frames: u32, strength: f32, contrast: f32, saturation: f32, gamma: f32, temperature: f32, dehaze: f32, _pad: f32 };

@group(0) @binding(0) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(1) var<storage, read> mats: array<f32>;
@group(0) @binding(2) var<uniform> cfg: Cfg;

const X_STRIDE: u32 = 65535u * 256u;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x + gid.y * X_STRIDE;
    let total = cfg.width * cfg.height * cfg.frames;
    if (idx >= total) { return; }
    let frame = idx / (cfg.width * cfg.height);
    let mb = frame * 20u;
    let v = pixels[idx];
    let r = f32(v & 0xffu);
    let g = f32((v >> 8u) & 0xffu);
    let b = f32((v >> 16u) & 0xffu);
    var nr = r * mats[mb] + g * mats[mb+1u] + b * mats[mb+2u] + mats[mb+4u] * 255.0;
    var ng = g * mats[mb+6u] + mats[mb+9u] * 255.0;
    var nb = b * mats[mb+12u] + mats[mb+14u] * 255.0;
    nr = clamp(nr, 0.0, 255.0);
    ng = clamp(ng, 0.0, 255.0);
    nb = clamp(nb, 0.0, 255.0);
    if (cfg.strength < 1.0) {
        nr = nr * cfg.strength + r * (1.0 - cfg.strength);
        ng = ng * cfg.strength + g * (1.0 - cfg.strength);
        nb = nb * cfg.strength + b * (1.0 - cfg.strength);
    }
    // temperature
    nr = nr + cfg.temperature * 30.0;
    nb = nb - cfg.temperature * 30.0;
    // contrast
    nr = (nr - 127.5) * cfg.contrast + 127.5;
    ng = (ng - 127.5) * cfg.contrast + 127.5;
    nb = (nb - 127.5) * cfg.contrast + 127.5;
    // saturation
    var gray = 0.299 * nr + 0.587 * ng + 0.114 * nb;
    nr = gray + (nr - gray) * cfg.saturation;
    ng = gray + (ng - gray) * cfg.saturation;
    nb = gray + (nb - gray) * cfg.saturation;
    // gamma
    nr = pow(nr / 255.0, 1.0 / max(cfg.gamma, 0.01)) * 255.0;
    ng = pow(ng / 255.0, 1.0 / max(cfg.gamma, 0.01)) * 255.0;
    nb = pow(nb / 255.0, 1.0 / max(cfg.gamma, 0.01)) * 255.0;
    nr = clamp(nr, 0.0, 255.0);
    ng = clamp(ng, 0.0, 255.0);
    nb = clamp(nb, 0.0, 255.0);
    // dehaze: boost saturation in mid/shadows
    if (cfg.dehaze > 0.0) {
        let gray = (nr + ng + nb) / 3.0;
        let k = 1.0 + cfg.dehaze;
        nr = gray + (nr - gray) * k;
        ng = gray + (ng - gray) * k;
        nb = gray + (nb - gray) * k;
        nr = clamp(nr, 0.0, 255.0);
        ng = clamp(ng, 0.0, 255.0);
        nb = clamp(nb, 0.0, 255.0);
    }
    let rr = u32(nr);
    let gg = u32(ng);
    let bb = u32(nb);
    pixels[idx] = rr | (gg << 8u) | (bb << 16u);
}