//! Shared texture bindings for desktop and offscreen rendering.
//! Color mips are averaged in linear light; height is non-color data.
use std::io::Cursor;

struct Level {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn decode(bytes: &[u8]) -> Level {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("embedded skin PNG header");
    let mut data = vec![0; reader.output_buffer_size().expect("skin PNG size")];
    let info = reader
        .next_frame(&mut data)
        .expect("embedded skin PNG pixels");
    let mut pixels = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    for p in data[..info.buffer_size()].chunks_exact(info.color_type.samples()) {
        let rgba = match info.color_type {
            png::ColorType::Rgb => [p[0], p[1], p[2], 255],
            png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
            png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
            png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
            _ => panic!("expanded skin PNG cannot be indexed"),
        };
        pixels.extend_from_slice(&rgba);
    }
    Level {
        width: info.width,
        height: info.height,
        pixels,
    }
}
fn downsample(src: &Level, color: bool) -> Level {
    let width = (src.width / 2).max(1);
    let height = (src.height / 2).max(1);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 4];
            let mut count = 0.0;
            // Include the last row/column for odd dimensions.
            for sy in y * src.height / height..(y + 1) * src.height / height {
                for sx in x * src.width / width..(x + 1) * src.width / width {
                    let i = ((sy * src.width + sx) * 4) as usize;
                    let a = src.pixels[i + 3] as f32 / 255.0;
                    for c in 0..3 {
                        let v = src.pixels[i + c] as f32 / 255.0;
                        sum[c] += (if color { linear(v) } else { v }) * a;
                    }
                    sum[3] += a;
                    count += 1.0;
                }
            }
            for c in 0..3 {
                let v = if sum[3] > 0.0 { sum[c] / sum[3] } else { 0.0 };
                pixels.push(
                    ((if color { srgb(v) } else { v }).clamp(0.0, 1.0) * 255.0).round() as u8,
                );
            }
            pixels.push((sum[3] / count * 255.0).round() as u8);
        }
    }
    Level {
        width,
        height,
        pixels,
    }
}
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    color: bool,
) -> wgpu::TextureView {
    let mut first = decode(bytes);
    while first.width > 4096 || first.height > 4096 {
        first = downsample(&first, color);
    }
    let mut levels = vec![first];
    while levels.last().unwrap().width > 1 || levels.last().unwrap().height > 1 {
        levels.push(downsample(levels.last().unwrap(), color));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(if color {
            "hognose albedo"
        } else {
            "hognose scale height"
        }),
        size: wgpu::Extent3d {
            width: levels[0].width,
            height: levels[0].height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if color {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip, level) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(level.width * 4),
                rows_per_image: Some(level.height),
            },
            wgpu::Extent3d {
                width: level.width,
                height: level.height,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&Default::default())
}

pub fn create_bindings(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    uniform: &wgpu::Buffer,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene materials"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture_entry(1),
            texture_entry(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let albedo = upload(
        device,
        queue,
        include_bytes!("../../../assets/hognose/albedo.png"),
        true,
    );
    let height = upload(
        device,
        queue,
        include_bytes!("../../../assets/hognose/scale_height.png"),
        false,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("skin trilinear"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    });
    // Bind groups retain their texture views and sampler resources.
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene materials"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&albedo),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&height),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    (layout, group)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn color_mips_average_light_not_encoded_bytes() {
        let l = Level {
            width: 2,
            height: 1,
            pixels: vec![0, 0, 0, 255, 255, 255, 255, 255],
        };
        assert!((downsample(&l, true).pixels[0] as i32 - 188).abs() <= 1);
        assert_eq!(downsample(&l, false).pixels[0], 128);
    }
    #[test]
    fn transparent_texels_do_not_bleed_and_odd_edges_survive() {
        let l = Level {
            width: 3,
            height: 1,
            pixels: vec![255, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255],
        };
        let m = downsample(&l, true);
        assert_eq!((m.width, m.height), (1, 1));
        assert_eq!(m.pixels, vec![0, 0, 255, 170]);
    }
}
