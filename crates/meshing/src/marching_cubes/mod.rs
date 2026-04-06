//! Marching cubes for a single grid cell (tables from `marching-cubes` 0.1.2, MIT).
//! The upstream `polygonise` loop indexed `TRI_TABLE` incorrectly; this version follows the
//! standard "three indices per triangle until -1" layout.

mod tables;

use tables::{EDGE_TABLE, TRI_TABLE};

#[derive(Debug, Clone, Copy)]
pub struct MarchingCubes {
    iso_value: f32,
    grid: GridCell,
}

#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub positions: [[f32; 3]; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct GridCell {
    pub positions: [[f32; 3]; 8],
    pub value: [f32; 8],
}

impl MarchingCubes {
    pub fn new(iso_value: f32, grid: GridCell) -> Self {
        Self { iso_value, grid }
    }

    pub fn polygonise(self, triangles: &mut [Triangle]) -> i32 {
        polygonise(self.grid, self.iso_value, triangles)
    }
}

fn interpolate_vertex(
    isolevel: f32,
    point1: [f32; 3],
    point2: [f32; 3],
    v0: f32,
    v1: f32,
) -> [f32; 3] {
    const ISO_THRESHOLD: f32 = 0.00001;

    let factor = (isolevel - v0) / (v1 - v0);

    if (isolevel - v0).abs() < ISO_THRESHOLD || (v0 - v1).abs() < ISO_THRESHOLD {
        return point1;
    } else if (isolevel - v1).abs() < ISO_THRESHOLD {
        return point2;
    }

    [
        point1[0] + factor * (point2[0] - point1[0]),
        point1[1] + factor * (point2[1] - point1[1]),
        point1[2] + factor * (point2[2] - point1[2]),
    ]
}

pub fn polygonise(grid_cell: GridCell, isolevel: f32, triangles: &mut [Triangle]) -> i32 {
    let mut cube_index: usize = 0;

    if grid_cell.value[0] < isolevel {
        cube_index |= 1;
    }
    if grid_cell.value[1] < isolevel {
        cube_index |= 2;
    }
    if grid_cell.value[2] < isolevel {
        cube_index |= 4;
    }
    if grid_cell.value[3] < isolevel {
        cube_index |= 8;
    }
    if grid_cell.value[4] < isolevel {
        cube_index |= 16;
    }
    if grid_cell.value[5] < isolevel {
        cube_index |= 32;
    }
    if grid_cell.value[6] < isolevel {
        cube_index |= 64;
    }
    if grid_cell.value[7] < isolevel {
        cube_index |= 128;
    }

    if EDGE_TABLE[cube_index] == 0 {
        return 0i32;
    }

    let mut vertices_list: [[f32; 3]; 12] = [[0.0; 3]; 12];
    let e = EDGE_TABLE[cube_index];

    if e & 1 == 0 {
        vertices_list[0] = interpolate_vertex(
            isolevel,
            grid_cell.positions[0],
            grid_cell.positions[1],
            grid_cell.value[0],
            grid_cell.value[1],
        );
    }
    if e & 2 == 0 {
        vertices_list[1] = interpolate_vertex(
            isolevel,
            grid_cell.positions[1],
            grid_cell.positions[2],
            grid_cell.value[1],
            grid_cell.value[2],
        );
    }
    if e & 4 == 0 {
        vertices_list[2] = interpolate_vertex(
            isolevel,
            grid_cell.positions[2],
            grid_cell.positions[3],
            grid_cell.value[2],
            grid_cell.value[3],
        );
    }
    if e & 8 == 0 {
        vertices_list[3] = interpolate_vertex(
            isolevel,
            grid_cell.positions[3],
            grid_cell.positions[0],
            grid_cell.value[3],
            grid_cell.value[0],
        );
    }
    if e & 16 == 0 {
        vertices_list[4] = interpolate_vertex(
            isolevel,
            grid_cell.positions[4],
            grid_cell.positions[5],
            grid_cell.value[4],
            grid_cell.value[5],
        );
    }
    if e & 32 == 0 {
        vertices_list[5] = interpolate_vertex(
            isolevel,
            grid_cell.positions[5],
            grid_cell.positions[6],
            grid_cell.value[5],
            grid_cell.value[6],
        );
    }
    if e & 64 == 0 {
        vertices_list[6] = interpolate_vertex(
            isolevel,
            grid_cell.positions[6],
            grid_cell.positions[7],
            grid_cell.value[6],
            grid_cell.value[7],
        );
    }
    if e & 128 == 0 {
        vertices_list[7] = interpolate_vertex(
            isolevel,
            grid_cell.positions[7],
            grid_cell.positions[4],
            grid_cell.value[7],
            grid_cell.value[4],
        );
    }
    if e & 256 == 0 {
        vertices_list[8] = interpolate_vertex(
            isolevel,
            grid_cell.positions[0],
            grid_cell.positions[4],
            grid_cell.value[0],
            grid_cell.value[4],
        );
    }
    if e & 512 == 0 {
        vertices_list[9] = interpolate_vertex(
            isolevel,
            grid_cell.positions[1],
            grid_cell.positions[5],
            grid_cell.value[1],
            grid_cell.value[5],
        );
    }
    if e & 1024 == 0 {
        vertices_list[10] = interpolate_vertex(
            isolevel,
            grid_cell.positions[2],
            grid_cell.positions[6],
            grid_cell.value[2],
            grid_cell.value[6],
        );
    }
    if e & 2048 == 0 {
        vertices_list[11] = interpolate_vertex(
            isolevel,
            grid_cell.positions[3],
            grid_cell.positions[7],
            grid_cell.value[3],
            grid_cell.value[7],
        );
    }

    let row = &TRI_TABLE[cube_index];
    let mut triangle_num = 0usize;
    let mut i = 0usize;
    while i < row.len() {
        let a = row[i];
        if a < 0 {
            break;
        }
        if i + 2 >= row.len() {
            break;
        }
        let b = row[i + 1];
        let c = row[i + 2];
        if b < 0 || c < 0 {
            break;
        }
        let ai = a as usize;
        let bi = b as usize;
        let ci = c as usize;
        if ai >= 12 || bi >= 12 || ci >= 12 {
            break;
        }
        if triangle_num >= triangles.len() {
            break;
        }
        triangles[triangle_num].positions[0] = vertices_list[ai];
        triangles[triangle_num].positions[1] = vertices_list[bi];
        triangles[triangle_num].positions[2] = vertices_list[ci];
        triangle_num += 1;
        i += 3;
    }

    triangle_num as i32
}
