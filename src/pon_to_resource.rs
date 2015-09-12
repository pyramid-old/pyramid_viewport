extern crate image;

use image::RgbaImage;
use pyramid::pon::*;
use mesh::*;

use std::path::Path;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt};
use std::borrow::Cow;

#[derive(Debug)]
pub struct ShaderSource {
    pub vertex_src: String,
    pub vertex_debug_source_name: String,
    pub fragment_src: String,
    pub fragment_debug_source_name: String
}

#[derive(Clone)]
pub enum Texture {
    Image(RgbaImage),
    Floats {
        width: u32,
        height: u32,
        data: Vec<f32>
    }
}

fn pon_to_layout(layout_node_array: &Vec<Pon>) -> Result<Layout, PonTranslateErr> {
    let mut layout = vec![];
    for p in layout_node_array {
        let p = try!(p.translate::<&Vec<Pon>>());
        layout.push(AttributeSpec(try!(p[0].translate::<&str>()).to_string(), try!(p[1].translate::<i64>()) as usize));
    }
    Ok(Layout::new(layout))
}

pub fn pon_to_mesh(root_path: &Path, node: &Pon) -> Result<Mesh, PonTranslateErr> {
    println!("Pon to mesh");
    let &TypedPon { type_name: ref type_name, ref data } = try!(node.translate());

    match type_name.as_str() {
        "static_mesh" => {
            let layout_node_array = try!(data.field_as::<&Vec<Pon>>("layout"));
            let layout = try!(pon_to_layout(layout_node_array));
            let vertices = try!(data.field_as::<Cow<Vec<f32>>>("vertices")).into_owned();
            let indices = try!(data.field_as::<Cow<Vec<i64>>>("indices")).into_owned();

            return Ok(Mesh {
                layout: layout,
                vertex_data: vertices,
                element_data: indices.iter().map(|x| *x as u32).collect()
            });
        },
        "grid_mesh" => {
            let mut grid = Grid::new();
            grid.layout = match data.field_as::<&Vec<Pon>>("layout") {
                Ok(layout_node_array) => try!(pon_to_layout(layout_node_array)),
                _ => Layout::position_texcoord_normal()
            };
            grid.n_vertices_width = try!(data.field_as::<i64>("n_vertices_width")) as u32;
            grid.n_vertices_height = try!(data.field_as::<i64>("n_vertices_height")) as u32;

            return Ok(grid.into());
        },
        _ => Err(PonTranslateErr::UnrecognizedType(type_name.clone()))
    }
}

pub fn pon_to_texture(root_path: &Path, node: &Pon) -> Result<Texture, PonTranslateErr> {
    println!("Pon to texture");
    let &TypedPon { ref type_name, ref data } = try!(node.translate());

    match type_name.as_str() {
        "static_texture" => {
            let pixel_data = try!(data.field_as::<Cow<Vec<i64>>>("pixels"));
            let pixel_data: Vec<u8> = pixel_data.iter().map(|x| *x as u8).collect();
            let width = try!(data.field_as::<i64>("width")) as u32;
            let height = try!(data.field_as::<i64>("height")) as u32;
            if width * height * 4 != pixel_data.len() as u32 {
                return Err(PonTranslateErr::Generic(format!("Expected {} pixels, found {}", width * height * 4, pixel_data.len())));
            }
            return match RgbaImage::from_raw(width, height, pixel_data) {
                Some(image) => Ok(Texture::Image(image)),
                None => Err(PonTranslateErr::Generic("Failed to create image in static_texture".to_string()))
            }
        },
        "texture_from_file" => {
            let filename = try!(data.translate::<&str>());
            let path_buff = root_path.join(Path::new(filename));
            let path = path_buff.as_path();
            println!("Loading image {:?}", path);
            if path.extension().unwrap().to_str().unwrap() == "dhm" {
                let mut f = File::open(path).unwrap();
                let mut data = vec![];
                f.read_to_end(&mut data);
                let mut rdr = Cursor::new(data);
                let width = rdr.read_i32::<LittleEndian>().unwrap() as u32;
                let height = rdr.read_i32::<LittleEndian>().unwrap() as u32;
                println!("SIZE {}, {}", width, height);
                let mut data = vec![];
                for y in 0..height {
                    for x in 0..width {
                        data.push(rdr.read_f32::<LittleEndian>().unwrap());
                    }
                }
                return Ok(Texture::Floats { width: width, height: height, data: data })
            } else {
                let img = image::open(&path);
                println!("Image loaded!");
                return match img {
                    Ok(img) => Ok(Texture::Image(img.to_rgba())),
                    Err(err) => Err(PonTranslateErr::Generic(format!("Failed to load image: {}: {:?}", filename, err)))
                };
            }
        },
        _ => Err(PonTranslateErr::UnrecognizedType(type_name.clone()))
    }
}

pub fn pon_to_shader(root_path: &Path, node: &Pon) -> Result<ShaderSource, PonTranslateErr> {
    println!("Pon to shader");
    let &TypedPon { ref type_name, ref data } = try!(node.translate());

    match type_name.as_str() {
        "shader_program" => {
            let vertex = try!(data.field_as::<&TypedPon>("vertex"));
            let fragment = try!(data.field_as::<&TypedPon>("fragment"));

            let vertex_string_arg = try!(vertex.data.translate::<&str>()).to_string();
            let vertex_src = match vertex.type_name.as_str() {
                "shader_from_file" => string_from_file(&root_path.join(Path::new(&vertex_string_arg))),
                "static_shader" => vertex_string_arg,
                _ => return Err(PonTranslateErr::UnrecognizedType(vertex.type_name.to_string()))
            };
            let fragment_string_arg = try!(fragment.data.translate::<&str>()).to_string();
            let fragment_src = match fragment.type_name.as_str() {
                "shader_from_file" => string_from_file(&root_path.join(Path::new(&fragment_string_arg))),
                "static_shader" => fragment_string_arg,
                _ => return Err(PonTranslateErr::UnrecognizedType(fragment.type_name.to_string()))
            };

            return Ok(ShaderSource {
                vertex_src: vertex_src,
                vertex_debug_source_name: vertex.to_string(),
                fragment_src: fragment_src,
                fragment_debug_source_name: fragment.to_string()
            })
        },
        _ => Err(PonTranslateErr::UnrecognizedType(type_name.clone()))
    }
}

fn string_from_file(path: &Path) -> String {
    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {:?}: {}", path, Error::description(&why)),
        Ok(file) => file,
    };
    let mut content = String::new();
    match file.read_to_string(&mut content) {
        Ok(_) => content,
        Err(err) => panic!("Failed to read file {}", Error::description(&err))
    }
}
