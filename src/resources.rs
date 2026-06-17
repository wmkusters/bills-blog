use std::io::{BufReader, Cursor};

use wgpu::util::DeviceExt;

use crate::{model, texture};

fn format_url(file_name: &str) -> reqwest::Url {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let origin = location.origin().unwrap();
    let mut full_path = file_name.to_string();
    if !full_path.starts_with("assets/") {
        full_path = format!("assets/{}", full_path);
    }
    // if !origin.ends_with("learn-wgpu") {
    //     origin = format!("{}/learn-wgpu", origin);
    // }
    let base = reqwest::Url::parse(&format!("{}/", origin,)).unwrap();
    base.join(&full_path).unwrap()
}

pub async fn load_string(file_name: &str) -> anyhow::Result<String> {
    let txt = {
        log::info!("load string: {}", file_name);
        let url = format_url(file_name);
        reqwest::get(url).await?.text().await?
    };
    Ok(txt)
}

pub async fn load_binary(file_name: &str) -> anyhow::Result<Vec<u8>> {
    let data = {
        let url = format_url(file_name);
        reqwest::get(url).await?.bytes().await?.to_vec()
    };

    Ok(data)
}

pub async fn load_texture(
    file_name: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<texture::Texture> {
    let data = load_binary(file_name).await?;
    texture::Texture::from_bytes(device, queue, &data, file_name)
}

pub async fn load_model(
    file_name: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
) -> anyhow::Result<model::Model> {
    log::info!("load model called with file name: {}", file_name);
    let obj_text = load_string(file_name).await?;
    let obj_cursor = Cursor::new(obj_text);
    let mut obj_reader = BufReader::new(obj_cursor);

    let (models, obj_materials) = tobj::load_obj_buf_async(
        &mut obj_reader,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        |p| async move {
            log::info!("Loading material file: '{}'", p);
            let mut pp = p;
            if !pp.starts_with("assets/") {
                pp = format!("assets/{}", pp);
            }
            let mat_text = match load_string(&pp).await {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Failed to load material '{}': {}", pp, e);
                    return Err(tobj::LoadError::OpenFileFailed);
                }
            };
            tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(mat_text)))
        },
    )
    .await?;

    let mut materials = Vec::new();
    for m in obj_materials? {
        let diffuse_texture = if m.diffuse_texture.is_empty() {
            let color = m.diffuse;
            let rgba = [
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
                255u8,
            ];
            texture::Texture::from_color(device, queue, rgba, &m.name)?
        } else {
            load_texture(&m.diffuse_texture, device, queue).await?
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                },
            ],
            label: None,
        });

        materials.push(model::Material {
            name: m.name,
            diffuse_texture,
            bind_group,
        })
    }
    log::info!("Loaded {} materials", materials.len());

    let meshes = models
        .into_iter()
        .map(|m| {
            let vertices = (0..m.mesh.positions.len() / 3)
                .map(|i| {
                    if m.mesh.normals.is_empty() {
                        model::ModelVertex {
                            position: [
                                m.mesh.positions[i * 3],
                                m.mesh.positions[i * 3 + 1],
                                m.mesh.positions[i * 3 + 2],
                            ],
                            tex_coords: if m.mesh.texcoords.is_empty() {
                                [0.0, 0.0]
                            } else {
                                [m.mesh.texcoords[i * 2], 1.0 - m.mesh.texcoords[i * 2 + 1]]
                            },
                            normal: [0.0, 0.0, 0.0],
                        }
                    } else {
                        model::ModelVertex {
                            position: [
                                m.mesh.positions[i * 3],
                                m.mesh.positions[i * 3 + 1],
                                m.mesh.positions[i * 3 + 2],
                            ],
                            tex_coords: if m.mesh.texcoords.is_empty() {
                                [0.0, 0.0]
                            } else {
                                [m.mesh.texcoords[i * 2], 1.0 - m.mesh.texcoords[i * 2 + 1]]
                            },
                            normal: [
                                m.mesh.normals[i * 3],
                                m.mesh.normals[i * 3 + 1],
                                m.mesh.normals[i * 3 + 2],
                            ],
                        }
                    }
                })
                .collect::<Vec<_>>();

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{:?} Vertex Buffer", file_name)),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{:?} Index Buffer", file_name)),
                contents: bytemuck::cast_slice(&m.mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            log::info!("Mesh: {}", m.name);
            model::Mesh {
                name: file_name.to_string(),
                vertex_buffer,
                index_buffer,
                num_elements: m.mesh.indices.len() as u32,
                material: m
                    .mesh
                    .material_id
                    .unwrap_or(0)
                    .min(materials.len().saturating_sub(1)),
            }
        })
        .collect::<Vec<_>>();

    Ok(model::Model { meshes, materials })
}
