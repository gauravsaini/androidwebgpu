use gles2wgpu::GlContext;
use pollster::block_on;

#[test]
fn test_gles2wgpu_e2e_render_and_readback() {
    block_on(async {
        let width = 64u32;
        let height = 64u32;

        let mut gl = match GlContext::new(width, height).await {
            Ok(ctx) => ctx,
            Err(err) => {
                println!("Failed to create GlContext (no GPU adapter?): {}", err);
                return;
            }
        };

        // 1. Clear with Blue color
        gl.gl_viewport(0, 0, width, height);
        gl.gl_clear_color(0.0, 0.0, 1.0, 1.0);
        gl.gl_clear(0x00004000); // GL_COLOR_BUFFER_BIT

        // 2. Read back default render target to verify GPU executed clear
        let bytes_per_row = (width * 4 + 255) & !255;
        let buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
        let readback_buffer = gl.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GLES Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = gl.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GLES Readback Encoder"),
        });

        let tex = gl.default_render_target.wgpu_texture.as_ref().unwrap();
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        gl.queue.submit(Some(encoder.finish()));

        let buffer_slice = readback_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| {
            sender.send(v).unwrap();
        });

        gl.device.poll(wgpu::Maintain::Wait);
        receiver.recv().unwrap().expect("Failed to map GLES readback buffer");

        {
            let data = buffer_slice.get_mapped_range();
            let center_offset = (32 * bytes_per_row + 32 * 4) as usize;
            let r = data[center_offset];
            let g = data[center_offset + 1];
            let b = data[center_offset + 2];
            let a = data[center_offset + 3];
            println!("GLES Clear Pixel Color (32, 32): R={}, G={}, B={}, A={}", r, g, b, a);
            assert_eq!(r, 0, "Expected R == 0");
            assert_eq!(g, 0, "Expected G == 0");
            assert!(b > 200, "Expected B > 200");
            assert!(a > 200, "Expected A > 200");
        }

        readback_buffer.unmap();
        println!("gles2wgpu E2E clear and render target check PASSED!");
    });
}

#[test]
fn test_gles2wgpu_indexed_mesh_draw_elements() {
    block_on(async {
        let width = 64u32;
        let height = 64u32;

        let mut gl = match GlContext::new(width, height).await {
            Ok(ctx) => ctx,
            Err(_) => return,
        };

        // 1. Shaders & Program
        let vs = gl.gl_create_shader(0x8B31);
        let vs_src = "attribute vec3 position; void main() { gl_Position = vec4(position, 1.0); }";
        gl.gl_shader_source(vs, vs_src);
        gl.gl_compile_shader(vs).expect("VS compile failed");

        let fs = gl.gl_create_shader(0x8B30);
        let fs_src = "precision mediump float; void main() { gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0); }";
        gl.gl_shader_source(fs, fs_src);
        gl.gl_compile_shader(fs).expect("FS compile failed");

        let program = gl.gl_create_program();
        gl.gl_attach_shader(program, vs);
        gl.gl_attach_shader(program, fs);
        gl.gl_link_program(program).expect("Link failed");
        gl.gl_use_program(program);

        // 2. Vertex buffer + Index buffer setup with VAO
        let vaos = gl.gl_gen_vertex_arrays(1);
        gl.gl_bind_vertex_array(vaos[0]);

        let bufs = gl.gl_gen_buffers(2);
        let vbo = bufs[0];
        let ibo = bufs[1];

        // 4 vertices (quad)
        let vertices: [f32; 12] = [
            -1.0, -1.0, 0.0,
             1.0, -1.0, 0.0,
             1.0,  1.0, 0.0,
            -1.0,  1.0, 0.0,
        ];
        gl.gl_bind_buffer(0x8892, vbo); // GL_ARRAY_BUFFER
        gl.gl_buffer_data(0x8892, bytemuck::cast_slice(&vertices), 0x88E4);
        gl.gl_vertex_attrib_pointer(0, 3, 0x1406, false, 12, 0);
        gl.gl_enable_vertex_attrib_array(0);

        // 2 triangles indices (6 indices)
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        gl.gl_bind_buffer(0x8893, ibo); // GL_ELEMENT_ARRAY_BUFFER
        gl.gl_buffer_data(0x8893, bytemuck::cast_slice(&indices), 0x88E4);

        // 3. Clear to Green, Draw quad to Red
        gl.gl_viewport(0, 0, width, height);
        gl.gl_clear_color(0.0, 1.0, 0.0, 1.0);
        gl.gl_clear(0x00004000);

        gl.gl_draw_elements(0x0004, 6, 0x1403, 0); // GL_TRIANGLES, 6 elements, GL_UNSIGNED_SHORT

        // 4. Verify center pixel is Red
        let bytes_per_row = (width * 4 + 255) & !255;
        let buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
        let readback_buffer = gl.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("DrawElements Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = gl.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("DrawElements Encoder"),
        });
        let tex = gl.default_render_target.wgpu_texture.as_ref().unwrap();
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        gl.queue.submit(Some(encoder.finish()));

        let buffer_slice = readback_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| {
            sender.send(v).unwrap();
        });

        gl.device.poll(wgpu::Maintain::Wait);
        receiver.recv().unwrap().expect("Failed to map buffer");

        {
            let data = buffer_slice.get_mapped_range();
            let center_offset = (32 * bytes_per_row + 32 * 4) as usize;
            let r = data[center_offset];
            let g = data[center_offset + 1];
            let b = data[center_offset + 2];
            println!("glDrawElements Center Pixel Color: R={}, G={}, B={}", r, g, b);
            assert!(r > 200, "Expected center pixel R > 200 after drawing red quad");
            assert_eq!(g, 0, "Expected center pixel G == 0");
        }

        readback_buffer.unmap();
        println!("glDrawElements indexed rendering test PASSED!");
    });
}

#[test]
fn test_gles2wgpu_uniforms() {
    block_on(async {
        let width = 32u32;
        let height = 32u32;

        let mut gl = match GlContext::new(width, height).await {
            Ok(ctx) => ctx,
            Err(_) => return,
        };

        let prog = gl.gl_create_program();
        let loc = gl.gl_get_uniform_location(prog, "u_color");
        assert_eq!(loc, 0);

        gl.gl_uniform_4fv(loc, 1, &[0.5, 0.25, 0.75, 1.0]);
        let float_view: &[f32] = bytemuck::cast_slice(&gl.uniform_data[0..16]);
        assert_eq!(float_view[0], 0.5);
        assert_eq!(float_view[1], 0.25);
        assert_eq!(float_view[2], 0.75);
        assert_eq!(float_view[3], 1.0);

        let mat_loc = gl.gl_get_uniform_location(prog, "u_matrix");
        assert_eq!(mat_loc, 1);
        let mat = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        gl.gl_uniform_matrix_4fv(mat_loc, 1, false, &mat);
        let mat_view: &[f32] = bytemuck::cast_slice(&gl.uniform_data[64..128]);
        assert_eq!(mat_view[0], 1.0);
        assert_eq!(mat_view[5], 1.0);
        assert_eq!(mat_view[10], 1.0);
        assert_eq!(mat_view[15], 1.0);
    });
}
