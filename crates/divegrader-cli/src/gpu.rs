use rayon::prelude::*;
use std::sync::OnceLock;
use wgpu::util::DeviceExt;

struct Ctx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

static CTX: OnceLock<Option<Ctx>> = OnceLock::new();

fn get_ctx() -> Option<&'static Ctx> {
    CTX.get_or_init(|| {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("divegrader"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            ..Default::default()
        }))
        .ok()?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("apply"),
            source: wgpu::ShaderSource::Wgsl(include_str!("apply.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[Some(&bgl)],
            ..Default::default()
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cp"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Some(Ctx {
            device,
            queue,
            pipeline,
            bgl,
        })
    })
    .as_ref()
}

pub fn apply_filter_gpu(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    matrix: &[f32; 20],
    strength: f32,
    dehaze: f32,
) -> bool {
    apply_filter_gpu_frames(
        pixels,
        width,
        height,
        1,
        &[*matrix],
        strength,
        1.0,
        1.0,
        1.0,
        0.0,
        dehaze,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_filter_gpu_frames(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    frames: usize,
    matrices: &[[f32; 20]],
    strength: f32,
    contrast: f32,
    saturation: f32,
    gamma: f32,
    temperature: f32,
    dehaze: f32,
) -> bool {
    if frames == 0 {
        return true;
    }
    let ctx = match get_ctx() {
        Some(c) => c,
        None => return false,
    };
    let n = width * height * frames;

    // pack RGB into u32 per pixel (parallel)
    let packed: Vec<u32> = pixels
        .par_chunks_exact(3)
        .map(|c| (c[0] as u32) | ((c[1] as u32) << 8) | ((c[2] as u32) << 16))
        .collect();

    let byte_size = (n * 4) as u64;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(usize, wgpu::Buffer, wgpu::Buffer)>> = Mutex::new(None);
    let (storage_buf, staging) = {
        let mut cache = CACHE.lock().unwrap();
        if let Some((sz, s, st)) = cache.take() {
            if sz == n {
                (s, st)
            } else {
                (
                    ctx.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("pixels"),
                        size: byte_size,
                        usage: wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    ctx.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("staging"),
                        size: byte_size,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                )
            }
        } else {
            (
                ctx.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("pixels"),
                    size: byte_size,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                ctx.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("staging"),
                    size: byte_size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            )
        }
    };
    ctx.queue
        .write_buffer(&storage_buf, 0, bytemuck::cast_slice(&packed));

    // matrices storage
    let mut mat_data = vec![0.0f32; frames * 20];
    for f in 0..frames {
        for k in 0..20 {
            mat_data[f * 20 + k] = matrices[f][k];
        }
    }
    let mat_buf = ctx
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mats"),
            contents: bytemuck::cast_slice(&mat_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
    struct Cfg {
        width: u32,
        height: u32,
        frames: u32,
        strength: f32,
        contrast: f32,
        saturation: f32,
        gamma: f32,
        temperature: f32,
        dehaze: f32,
        _pad: f32,
    }
    let cfg = Cfg {
        width: width as u32,
        height: height as u32,
        frames: frames as u32,
        strength,
        contrast,
        saturation,
        gamma,
        temperature,
        dehaze,
        _pad: 0.0,
    };
    let cfg_buf = ctx
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cfg"),
            contents: bytemuck::cast_slice(&[cfg]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
        layout: &ctx.bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: storage_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: mat_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: cfg_buf.as_entire_binding(),
            },
        ],
    });

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cpass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&ctx.pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        let gx = 65535u32;
        let threads_per_layer = gx * 256;
        let total_threads = n as u32;
        let gy = total_threads.div_ceil(threads_per_layer).max(1);
        cpass.dispatch_workgroups(gx, gy, 1);
    }
    encoder.copy_buffer_to_buffer(&storage_buf, 0, &staging, 0, byte_size);
    ctx.queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    let _ = ctx.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    if rx.recv().unwrap().is_err() {
        return false;
    }
    let data = slice.get_mapped_range().unwrap();
    let words: &[u32] = bytemuck::cast_slice(&data);
    pixels
        .par_chunks_exact_mut(3)
        .zip(words.par_iter())
        .for_each(|(p, &v)| {
            p[0] = (v & 0xff) as u8;
            p[1] = ((v >> 8) & 0xff) as u8;
            p[2] = ((v >> 16) & 0xff) as u8;
        });
    drop(data);
    staging.unmap();
    *CACHE.lock().unwrap() = Some((n, storage_buf, staging));
    true
}
