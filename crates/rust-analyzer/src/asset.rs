///! Rust equivalent of the asset::Project structure from asset.go
///!
///! This module provides data structures to represent a collection of files,
///! their ASTs, and associated diagnostics, similar to the Go implementation.

use std::collections::HashMap;
use std::io::{self, Read, Write, Seek, SeekFrom};

const MAGIC: u8 = 0xde;
const ASSET_ENCODING_VERSION: u32 = 1;

/// Represents a range in a file (start and end offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    /// The starting offset (inclusive).
    pub offset: usize,
    /// The ending offset (exclusive).
    pub end_offset: usize,
}

/// Represents a node in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The range in the source file this node covers.
    pub range: Range,
    /// The type name of the node.
    pub node_type: String,
}

/// Represents a message annotation (or a parser error) for a range/offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// The range in the source file this annotation covers.
    pub range: Range,
    /// The annotation text.
    pub text: String,
}

/// Represents a file which can be encoded as an asset.
/// It optionally contains an expected Tree structure produced by a parser and a list of annotations
/// (messages or parser errors) for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// The file path.
    pub path: String,
    /// The file content.
    pub content: String,
    /// (optional) The AST of the file encoded as a flat list of nodes, in preorder.
    pub tree: Vec<Node>,
    /// (optional) Any problems encountered by the compiler when processing this file.
    pub errors: Vec<Annotation>,
}

/// Represents a collection of files which can be encoded as an asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The files in the project.
    pub files: Vec<File>,
}

impl Project {
    pub fn encode<W: Write + Seek>(&self, mut writer: W) -> anyhow::Result<()> {
        // String table: collect all unique strings and assign indices
        let mut string_table = StringTableBuilder::default();
        for file in &self.files {
            string_table.add(&file.path);
            string_table.add(&file.content);
            for node in &file.tree {
                string_table.add(&node.node_type);
            }
            for ann in &file.errors {
                string_table.add(&ann.text);
            }
        }
        // Write header
        writer.write_all(&[MAGIC])?;
        writer.write_all(&ASSET_ENCODING_VERSION.to_le_bytes())?;
        let string_table_offset_pos = writer.stream_position()?;
        writer.write_all(&0u32.to_le_bytes())?; // placeholder for string table offset
        writer.write_all(&(self.files.len() as u32).to_le_bytes())?;
        // Write files
        for file in &self.files {
            writer.write_all(&(string_table.idx(&file.path)? as u32).to_le_bytes())?;
            writer.write_all(&(string_table.idx(&file.content)? as u32).to_le_bytes())?;
            writer.write_all(&(file.tree.len() as u32).to_le_bytes())?;
            for node in &file.tree {
                writer.write_all(&(node.range.offset as u32).to_le_bytes())?;
                writer.write_all(&(node.range.end_offset as u32).to_le_bytes())?;
                writer.write_all(&(string_table.idx(&node.node_type)? as u32).to_le_bytes())?;
            }
            writer.write_all(&(file.errors.len() as u32).to_le_bytes())?;
            for ann in &file.errors {
                writer.write_all(&(ann.range.offset as u32).to_le_bytes())?;
                writer.write_all(&(ann.range.end_offset as u32).to_le_bytes())?;
                writer.write_all(&(string_table.idx(&ann.text)? as u32).to_le_bytes())?;
            }
        }
        // Write string table offset
        let string_table_offset = writer.stream_position()? as u32;
        let cur = writer.stream_position()?;
        writer.seek(SeekFrom::Start(string_table_offset_pos))?;
        writer.write_all(&string_table_offset.to_le_bytes())?;
        writer.seek(SeekFrom::Start(cur))?;
        // Write string table
        string_table.write(&mut writer)?;
        Ok(())
    }

    pub fn decode<R: Read + Seek>(mut reader: R) -> anyhow::Result<Self> {
        let mut magic = [0u8; 1];
        reader.read_exact(&mut magic)?;
        if magic[0] != MAGIC {
            anyhow::bail!("invalid magic byte: expected 0xde, got {:x}", magic[0]);
        }
        let version = read_u32(&mut reader)?;
        if version != ASSET_ENCODING_VERSION {
            anyhow::bail!("version mismatch: expected {}, got {}", ASSET_ENCODING_VERSION, version);
        }
        let string_table_offset = read_u32(&mut reader)?;
        let num_files = read_u32(&mut reader)?;
        let files_start = reader.stream_position()?;
        // Read string table
        reader.seek(SeekFrom::Start(string_table_offset as u64))?;
        let string_table = StringTable::read(&mut reader)?;
        // Read files
        reader.seek(SeekFrom::Start(files_start))?;
        let mut files = Vec::with_capacity(num_files as usize);
        for _ in 0..num_files {
            let path_idx = read_u32(&mut reader)? as usize;
            let content_idx = read_u32(&mut reader)? as usize;
            let num_nodes = read_u32(&mut reader)?;
            let mut tree = Vec::with_capacity(num_nodes as usize);
            for _ in 0..num_nodes {
                let offset = read_u32(&mut reader)? as usize;
                let end_offset = read_u32(&mut reader)? as usize;
                let type_idx = read_u32(&mut reader)? as usize;
                tree.push(Node {
                    range: Range { offset, end_offset },
                    node_type: string_table.get(type_idx)?.to_owned(),
                });
            }
            let num_errors = read_u32(&mut reader)?;
            let mut errors = Vec::with_capacity(num_errors as usize);
            for _ in 0..num_errors {
                let offset = read_u32(&mut reader)? as usize;
                let end_offset = read_u32(&mut reader)? as usize;
                let text_idx = read_u32(&mut reader)? as usize;
                errors.push(Annotation {
                    range: Range { offset, end_offset },
                    text: string_table.get(text_idx)?.to_owned(),
                });
            }
            files.push(File {
                path: string_table.get(path_idx)?.to_owned(),
                content: string_table.get(content_idx)?.to_owned(),
                tree,
                errors,
            });
        }
        Ok(Project { files })
    }
}

fn read_u32<R: Read>(r: &mut R) -> anyhow::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

#[derive(Default)]
struct StringTableBuilder {
    map: HashMap<String, usize>,
    vec: Vec<String>,
}

impl StringTableBuilder {
    fn add(&mut self, s: &str) {
        if !self.map.contains_key(s) {
            self.map.insert(s.to_owned(), self.vec.len());
            self.vec.push(s.to_owned());
        }
    }
    fn idx(&self, s: &str) -> anyhow::Result<usize> {
        self.map.get(s).copied().ok_or_else(|| anyhow::anyhow!("string not found in table: {}", s))
    }
    fn write<W: Write>(&self, mut w: W) -> anyhow::Result<()> {
        w.write_all(&(self.vec.len() as u32).to_le_bytes())?;
        for s in &self.vec {
            w.write_all(&(s.len() as u32).to_le_bytes())?;
            w.write_all(s.as_bytes())?;
        }
        Ok(())
    }
}

struct StringTable {
    vec: Vec<String>,
}

impl StringTable {
    fn read<R: Read>(mut r: R) -> anyhow::Result<Self> {
        let num_strings = read_u32(&mut r)?;
        let mut vec = Vec::with_capacity(num_strings as usize);
        for _ in 0..num_strings {
            let len = read_u32(&mut r)? as usize;
            let mut buf = vec![0u8; len];
            r.read_exact(&mut buf)?;
            vec.push(String::from_utf8(buf)?);
        }
        Ok(Self { vec })
    }
    fn get(&self, idx: usize) -> anyhow::Result<&str> {
        self.vec.get(idx).map(|s| s.as_str()).ok_or_else(|| anyhow::anyhow!("string index {} out of range", idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encode_decode_roundtrip() {
        let project = Project {
            files: vec![
                File {
                    path: "foo.rs".to_string(),
                    content: "fn main() {}".to_string(),
                    tree: vec![Node {
                        range: Range { offset: 0, end_offset: 10 },
                        node_type: "Function".to_string(),
                    }],
                    errors: vec![Annotation {
                        range: Range { offset: 3, end_offset: 7 },
                        text: "error: something".to_string(),
                    }],
                },
                File {
                    path: "bar.rs".to_string(),
                    content: "let x = 42;".to_string(),
                    tree: vec![Node {
                        range: Range { offset: 0, end_offset: 10 },
                        node_type: "Let".to_string(),
                    }],
                    errors: vec![Annotation {
                        range: Range { offset: 4, end_offset: 5 },
                        text: "warning: unused variable".to_string(),
                    }],
                },
                File {
                    path: "baz.rs".to_string(),
                    content: "struct S;".to_string(),
                    tree: vec![Node {
                        range: Range { offset: 0, end_offset: 8 },
                        node_type: "Struct".to_string(),
                    }],
                    errors: vec![Annotation {
                        range: Range { offset: 0, end_offset: 6 },
                        text: "note: struct defined here".to_string(),
                    }],
                },
            ],
        };
        let mut buf = Cursor::new(Vec::new());
        project.encode(&mut buf).expect("encode");
        buf.set_position(0);
        let decoded = Project::decode(&mut buf).expect("decode");
        assert_eq!(project, decoded);
    }
}