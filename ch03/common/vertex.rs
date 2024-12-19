use wgpu_surfaces::surface_data as sd;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

pub fn create_vertices(
    ss_data: sd::ISurfaceOutput,
) -> (Vec<Vertex>, Vec<Vertex>, Vec<u16>, Vec<u16>) {
    let mut data: Vec<Vertex> = vec![];
    let mut data2: Vec<Vertex> = vec![];
    for i in 0..ss_data.positions.len() {
        data.push(Vertex {
            position: ss_data.positions[i],
            normal: ss_data.normals[i],
            color: ss_data.colors[i],
        });
        data2.push(Vertex {
            position: ss_data.positions[i],
            normal: ss_data.normals[i],
            color: ss_data.colors2[i],
        });
    }
    (
        data.to_vec(),
        data2.to_vec(),
        ss_data.indices,
        ss_data.indices2,
    )
}
