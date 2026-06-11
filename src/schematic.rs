#![allow(dead_code)]
//! Parser for Sponge schematic (`.schem`) files, versions 2 and 3.
//!
//! A `.schem` is gzipped NBT. We gunzip it, deserialize the NBT into [`RawRoot`]
//! with `fastnbt`, and normalize both schema versions into a single [`Schematic`].
//!
//! ## Layout differences we paper over
//! - **v2**: `Palette`, `BlockData`, `BlockEntities`, `Width/Height/Length`,
//!   `Offset` all live at the root compound.
//! - **v3**: everything is nested under a `Schematic` compound, and block data is
//!   under a `Blocks` sub-compound as `Palette` + `Data` (note: `Data`, not
//!   `BlockData`).
//!
//! ## Block data encoding
//! `BlockData`/`Data` is a flat byte array of **LEB128 varints**, one per block,
//! each a local palette index. Blocks are ordered `x + z*Width + y*Width*Length`,
//! so x varies fastest, then z, then y.

use std::collections::HashMap;

use fastnbt::{ByteArray, IntArray};
use serde::{Deserialize, Serialize};

/// A parsed, version-normalized schematic ready to paste.
pub struct Schematic {
    pub width: u16,
    pub height: u16,
    pub length: u16,
    /// Relative origin offset stored in the file (defaults to `[0,0,0]`).
    pub offset: [i32; 3],
    /// Decoded block at each position; index = `x + z*width + y*width*length`.
    /// Each entry is the palette key, e.g. `"minecraft:oak_log[axis=x]"`.
    pub blocks: Vec<String>,
    /// Non-air block cells, precomputed while parsing so normal pastes do not scan
    /// millions of air cells.
    pub non_air_blocks: Vec<SchematicBlock>,
}

/// A non-air block cell in local schematic coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchematicBlock {
    pub x: u16,
    pub y: u16,
    pub z: u16,
    pub index: usize,
}

impl Schematic {
    pub fn volume(&self) -> usize {
        self.width as usize * self.height as usize * self.length as usize
    }

    /// Palette key at local coordinates, or `None` if out of bounds.
    pub fn block_at(&self, x: u16, y: u16, z: u16) -> Option<&str> {
        if x >= self.width || y >= self.height || z >= self.length {
            return None;
        }
        self.blocks.get(self.index_of(x, y, z)).map(String::as_str)
    }

    /// Flat block index for local coords: `x + z*W + y*W*L` (x fastest, then z, then y).
    pub fn index_of(&self, x: u16, y: u16, z: u16) -> usize {
        x as usize
            + z as usize * self.width as usize
            + y as usize * self.width as usize * self.length as usize
    }
}

/// Errors that can occur while loading a schematic.
#[derive(Debug)]
#[allow(dead_code)] // variants are constructed conditionally on file shape
pub enum SchematicError {
    Gunzip(String),
    Nbt(String),
    Unsupported(String),
    Malformed(String),
}

impl std::fmt::Display for SchematicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gunzip(e) => write!(f, "gunzip failed: {e}"),
            Self::Nbt(e) => write!(f, "NBT parse failed: {e}"),
            Self::Unsupported(e) => write!(f, "unsupported schematic: {e}"),
            Self::Malformed(e) => write!(f, "malformed schematic: {e}"),
        }
    }
}

/// Parse raw `.schem` file bytes into a [`Schematic`].
pub fn parse(bytes: &[u8]) -> Result<Schematic, SchematicError> {
    let nbt = gunzip_if_needed(bytes)?;
    let root: RawRoot =
        fastnbt::from_bytes(&nbt).map_err(|e| SchematicError::Nbt(e.to_string()))?;

    // v3 nests under "Schematic"; v2 is flat. Prefer the nested one if present.
    let body = root.schematic.as_ref().unwrap_or(&root.flat);

    let (palette, data) = if let Some(blocks) = &body.blocks {
        // v3: Blocks { Palette, Data }
        (
            blocks
                .palette
                .as_ref()
                .ok_or_else(|| SchematicError::Malformed("v3 Blocks missing Palette".into()))?,
            blocks
                .data
                .as_ref()
                .ok_or_else(|| SchematicError::Malformed("v3 Blocks missing Data".into()))?,
        )
    } else {
        // v2: root Palette + BlockData
        (
            body.palette
                .as_ref()
                .ok_or_else(|| SchematicError::Malformed("missing Palette".into()))?,
            body.block_data
                .as_ref()
                .ok_or_else(|| SchematicError::Malformed("missing BlockData".into()))?,
        )
    };

    let width = body.width.ok_or_else(|| miss("Width"))?;
    let height = body.height.ok_or_else(|| miss("Height"))?;
    let length = body.length.ok_or_else(|| miss("Length"))?;

    // Invert the palette: local-id -> name.
    let mut id_to_name: HashMap<i32, &str> = HashMap::with_capacity(palette.len());
    for (name, id) in palette {
        id_to_name.insert(*id, name.as_str());
    }

    // fastnbt gives byte arrays as i8; reinterpret as unsigned for varint decode.
    let raw: Vec<u8> = data.iter().map(|b| *b as u8).collect();
    let indices = decode_varints(&raw);

    let expected = width as usize * height as usize * length as usize;
    if indices.len() != expected {
        return Err(SchematicError::Malformed(format!(
            "block count {} != Width*Height*Length {}",
            indices.len(),
            expected
        )));
    }

    let mut blocks = Vec::with_capacity(expected);
    let mut non_air_blocks = Vec::new();
    for (index, local) in indices.into_iter().enumerate() {
        let name = id_to_name.get(&local).copied().unwrap_or("minecraft:air");
        blocks.push(name.to_string());

        if !is_air_key(name) {
            let x = (index % width as usize) as u16;
            let z = ((index / width as usize) % length as usize) as u16;
            let y = (index / (width as usize * length as usize)) as u16;
            non_air_blocks.push(SchematicBlock { x, y, z, index });
        }
    }

    // `Offset` is a 3-element int array; fall back to origin if absent/wrong length.
    let offset = body
        .offset
        .as_ref()
        .and_then(|o| <[i32; 3]>::try_from(&o[..]).ok())
        .unwrap_or([0, 0, 0]);

    Ok(Schematic {
        width,
        height,
        length,
        offset,
        blocks,
        non_air_blocks,
    })
}

pub fn is_air_key(key: &str) -> bool {
    key == "minecraft:air" || key.starts_with("minecraft:air[")
}

fn miss(field: &str) -> SchematicError {
    SchematicError::Malformed(format!("missing {field}"))
}

/// Gunzip if the bytes start with the gzip magic (0x1f 0x8b); otherwise assume
/// the NBT is already uncompressed.
fn gunzip_if_needed(bytes: &[u8]) -> Result<Vec<u8>, SchematicError> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| SchematicError::Gunzip(e.to_string()))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Decode a buffer of unsigned LEB128 varints into palette indices.
fn decode_varints(buf: &[u8]) -> Vec<i32> {
    let mut out = Vec::new();
    let mut value: u32 = 0;
    let mut shift = 0u32;
    for &byte in buf {
        value |= ((byte & 0x7f) as u32) << shift;
        if byte & 0x80 == 0 {
            out.push(value as i32);
            value = 0;
            shift = 0;
        } else {
            shift += 7;
        }
    }
    out
}

/// Encode palette indices as unsigned LEB128 varints (the inverse of
/// [`decode_varints`]).
fn encode_varints(indices: &[i32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(indices.len());
    for &index in indices {
        let mut value = index as u32;
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
    }
    out
}

/// Approximate Minecraft data version for the schematic's `DataVersion`
/// field. WorldEdit/FAWE use this to decide whether block states need
/// updating on load; readers tolerate a slightly stale value, so this is
/// kept as a single constant rather than detected at runtime.
const DATA_VERSION: i32 = 3953; // Minecraft 1.21

/// Serialize `width`x`height`x`length` `blocks` (in `x + z*W + y*W*L` order,
/// one Sponge palette key per cell) into a gzip-compressed Sponge v2
/// `.schem`.
///
/// `offset` is stored as-is and reproduced by [`parse`] on load.
pub fn write(
    width: u16,
    height: u16,
    length: u16,
    offset: [i32; 3],
    blocks: &[String],
) -> Result<Vec<u8>, SchematicError> {
    let expected = width as usize * height as usize * length as usize;
    if blocks.len() != expected {
        return Err(SchematicError::Malformed(format!(
            "block count {} != Width*Height*Length {}",
            blocks.len(),
            expected
        )));
    }

    // Build the palette: assign each distinct key a local id in first-seen
    // order, with `minecraft:air` always present (and id 0) even if unused,
    // matching what most readers expect.
    let mut palette: HashMap<String, i32> = HashMap::new();
    palette.insert("minecraft:air".to_string(), 0);
    let mut indices = Vec::with_capacity(blocks.len());
    for key in blocks {
        let next_id = palette.len() as i32;
        let id = *palette.entry(key.clone()).or_insert(next_id);
        indices.push(id);
    }

    let raw = RawRootOwned {
        width,
        height,
        length,
        offset: IntArray::new(offset.to_vec()),
        palette_max: palette.len() as i32,
        palette,
        block_data: ByteArray::new(
            encode_varints(&indices)
                .into_iter()
                .map(|b| b as i8)
                .collect(),
        ),
        data_version: DATA_VERSION,
        version: 2,
    };

    let nbt = fastnbt::to_bytes(&raw).map_err(|e| SchematicError::Nbt(e.to_string()))?;
    gzip(&nbt)
}

/// Gzip `bytes` (the inverse of [`gunzip_if_needed`]).
fn gzip(bytes: &[u8]) -> Result<Vec<u8>, SchematicError> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|e| SchematicError::Gunzip(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| SchematicError::Gunzip(e.to_string()))
}

// ---- Raw NBT shapes -------------------------------------------------------

/// The root compound. v3 puts data under `Schematic`; v2 is flat at the root, so
/// we flatten the rest of the root fields into `flat`.
#[derive(Deserialize, Default)]
struct RawRoot {
    #[serde(rename = "Schematic")]
    schematic: Option<RawBody>,
    #[serde(flatten)]
    flat: RawBody,
}

#[derive(Deserialize, Default)]
struct RawBody {
    #[serde(rename = "Width")]
    width: Option<u16>,
    #[serde(rename = "Height")]
    height: Option<u16>,
    #[serde(rename = "Length")]
    length: Option<u16>,
    // `Offset` is a TAG_Int_Array in the file; fastnbt needs `IntArray`, not `[i32;3]`.
    #[serde(rename = "Offset")]
    offset: Option<IntArray>,

    // v2 flat fields:
    #[serde(rename = "Palette")]
    palette: Option<HashMap<String, i32>>,
    // `BlockData` is a TAG_Byte_Array; fastnbt needs `ByteArray`, not `Vec<i8>`.
    #[serde(rename = "BlockData")]
    block_data: Option<ByteArray>,

    // v3 nested:
    #[serde(rename = "Blocks")]
    blocks: Option<RawBlocks>,
}

#[derive(Deserialize, Default)]
struct RawBlocks {
    #[serde(rename = "Palette")]
    palette: Option<HashMap<String, i32>>,
    // `Data` is a TAG_Byte_Array; fastnbt needs `ByteArray`, not `Vec<i8>`.
    #[serde(rename = "Data")]
    data: Option<ByteArray>,
}

/// Flat Sponge v2 root compound, written by [`write`] and readable by
/// [`parse`] (which accepts v2's flat layout via `RawRoot::flat`).
#[derive(Serialize)]
struct RawRootOwned {
    #[serde(rename = "Width")]
    width: u16,
    #[serde(rename = "Height")]
    height: u16,
    #[serde(rename = "Length")]
    length: u16,
    #[serde(rename = "Offset")]
    offset: IntArray,
    #[serde(rename = "PaletteMax")]
    palette_max: i32,
    #[serde(rename = "Palette")]
    palette: HashMap<String, i32>,
    #[serde(rename = "BlockData")]
    block_data: ByteArray,
    #[serde(rename = "DataVersion")]
    data_version: i32,
    #[serde(rename = "Version")]
    version: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_single_byte_varints() {
        assert_eq!(decode_varints(&[0, 1, 2, 127]), vec![0, 1, 2, 127]);
    }

    #[test]
    fn decodes_multi_byte_varint() {
        // 300 = 0b1_0010_1100 -> varint [0xAC, 0x02]
        assert_eq!(decode_varints(&[0xAC, 0x02]), vec![300]);
    }

    fn empty_schem(width: u16, height: u16, length: u16) -> Schematic {
        Schematic {
            width,
            height,
            length,
            offset: [0, 0, 0],
            blocks: Vec::new(),
            non_air_blocks: Vec::new(),
        }
    }

    #[test]
    fn index_of_is_a_bijection_over_the_volume() {
        let s = empty_schem(4, 3, 5); // W=4, H=3, L=5 => 60 cells
        let mut seen = vec![false; s.volume()];
        for y in 0..s.height {
            for z in 0..s.length {
                for x in 0..s.width {
                    let idx = s.index_of(x, y, z);
                    assert!(idx < s.volume(), "index must be in range");
                    assert!(!seen[idx], "index_of must be unique per coord");
                    seen[idx] = true;
                }
            }
        }
        assert!(seen.into_iter().all(|b| b), "every index must be hit");
    }

    #[test]
    fn index_of_orders_x_then_z_then_y() {
        let s = empty_schem(2, 2, 2);
        // x fastest, then z, then y.
        assert_eq!(s.index_of(0, 0, 0), 0);
        assert_eq!(s.index_of(1, 0, 0), 1);
        assert_eq!(s.index_of(0, 0, 1), 2); // z+1
        assert_eq!(s.index_of(0, 1, 0), 4); // y+1 (skips a full W*L layer)
    }

    #[test]
    fn encode_varints_round_trips_decode_varints() {
        let indices = vec![0, 1, 2, 127, 300, 16384];
        assert_eq!(decode_varints(&encode_varints(&indices)), indices);
    }

    #[test]
    fn write_then_parse_round_trips_blocks_and_dimensions() {
        let blocks = vec![
            "minecraft:stone".to_string(),
            "minecraft:air".to_string(),
            "minecraft:oak_log[axis=x]".to_string(),
            "minecraft:dirt".to_string(),
            "minecraft:dirt".to_string(),
            "minecraft:dirt".to_string(),
            "minecraft:dirt".to_string(),
            "minecraft:dirt".to_string(),
        ];
        let bytes = write(2, 2, 2, [1, -2, 3], &blocks).unwrap();
        let parsed = parse(&bytes).unwrap();

        assert_eq!((parsed.width, parsed.height, parsed.length), (2, 2, 2));
        assert_eq!(parsed.offset, [1, -2, 3]);
        assert_eq!(parsed.blocks, blocks);
        // One air cell out of eight, so seven non-air blocks.
        assert_eq!(parsed.non_air_blocks.len(), 7);
    }

    #[test]
    fn write_rejects_mismatched_block_count() {
        let blocks = vec!["minecraft:stone".to_string()];
        assert!(write(2, 2, 2, [0, 0, 0], &blocks).is_err());
    }
}
