// Copyright (c) IxMilia.  All Rights Reserved.  Licensed under the Apache License, Version 2.0.  See License.txt in the project root for license information.

extern crate byteorder;
use self::byteorder::{
    ByteOrder,
    LittleEndian,
};

use entities::*;
use enums::*;
use header::*;
use objects::*;
use tables::*;

use ::{
    CodePair,
    CodePairValue,
    DxfError,
    DxfResult,
};

use ::dxb_reader::DxbReader;
use ::dxb_writer::DxbWriter;
use ::entity_iter::EntityIter;
use ::helper_functions::*;
use ::object_iter::ObjectIter;

use block::Block;
use class::Class;

use code_pair_iter::CodePairIter;
use code_pair_writer::CodePairWriter;

use std::fs::File;
use std::io::{
    BufReader,
    BufWriter,
    Read,
    Write,
};

use std::path::Path;
use itertools::PutBack;

/// Represents a DXF drawing.
pub struct Drawing {
    /// The drawing's header.  Contains various drawing-specific values and settings.
    pub header: Header,
    /// The classes contained by the drawing.
    pub classes: Vec<Class>,
    /// The AppIds contained by the drawing.
    pub app_ids: Vec<AppId>,
    /// The block records contained by the drawing.
    pub block_records: Vec<BlockRecord>,
    /// The dimension styles contained by the drawing.
    pub dim_styles: Vec<DimStyle>,
    /// The layers contained by the drawing.
    pub layers: Vec<Layer>,
    /// The line types contained by the drawing.
    pub line_types: Vec<LineType>,
    /// The visual styles contained by the drawing.
    pub styles: Vec<Style>,
    /// The user coordinate systems (UCS) contained by the drawing.
    pub ucs: Vec<Ucs>,
    /// The views contained by the drawing.
    pub views: Vec<View>,
    /// The view ports contained by the drawing.
    pub view_ports: Vec<ViewPort>,
    /// The blocks contained by the drawing.
    pub blocks: Vec<Block>,
    /// The entities contained by the drawing.
    pub entities: Vec<Entity>,
    /// The objects contained by the drawing.
    pub objects: Vec<Object>,
    /// The thumbnail image preview of the drawing.
    pub thumbnail: Option<Vec<u8>>,
}

impl Default for Drawing {
    fn default() -> Self {
        Drawing {
            header: Header::default(),
            classes: vec![],
            app_ids: vec![],
            block_records: vec![],
            dim_styles: vec![],
            layers: vec![],
            line_types: vec![],
            styles: vec![],
            ucs: vec![],
            views: vec![],
            view_ports: vec![],
            blocks: vec![],
            entities: vec![],
            objects: vec![],
            thumbnail: None,
        }
    }
}

// public implementation
impl Drawing {
    /// Loads a `Drawing` from anything that implements the `Read` trait.
    pub fn load<T>(reader: &mut T) -> DxfResult<Drawing>
        where T: Read {

        let first_line = match read_line(reader) {
            Some(Ok(line)) => line,
            Some(Err(e)) => return Err(e),
            None => return Err(DxfError::UnexpectedEndOfInput),
        };
        match &*first_line {
            "AutoCAD DXB 1.0" => {
                let mut reader = DxbReader::new(reader);
                reader.load()
            },
            _ => {
                let reader = CodePairIter::new(reader, first_line);
                let mut drawing = Drawing::default();
                let mut iter = PutBack::new(reader);
                try!(Drawing::read_sections(&mut drawing, &mut iter));
                match iter.next() {
                    Some(Ok(CodePair { code: 0, value: CodePairValue::Str(ref s) })) if s == "EOF" => Ok(drawing),
                    Some(Ok(pair)) => Err(DxfError::UnexpectedCodePair(pair, String::from("expected 0/EOF"))),
                    Some(Err(e)) => Err(e),
                    None => Ok(drawing),
                }
            }
        }
    }
    /// Loads a `Drawing` from disk, using a `BufReader`.
    pub fn load_file(file_name: &str) -> DxfResult<Drawing> {
        let path = Path::new(file_name);
        let file = try!(File::open(&path));
        let mut buf_reader = BufReader::new(file);
        Drawing::load(&mut buf_reader)
    }
    /// Writes a `Drawing` to anything that implements the `Write` trait.
    pub fn save<T>(&self, writer: &mut T) -> DxfResult<()>
        where T: Write {

        let mut writer = CodePairWriter::new_ascii_writer(writer);
        self.save_internal(&mut writer)
    }
    /// Writes a `Drawing` as binary to anything that implements the `Write` trait.
    pub fn save_binary<T>(&self, writer: &mut T) -> DxfResult<()>
        where T: Write {

        let mut writer = CodePairWriter::new_binary_writer(writer);
        self.save_internal(&mut writer)
    }
    fn save_internal<T>(&self, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        try!(writer.write_prelude());
        try!(self.header.write(writer));
        let write_handles = self.header.version >= AcadVersion::R13 || self.header.handles_enabled;
        try!(self.write_classes(writer));
        try!(self.write_tables(write_handles, writer));
        try!(self.write_blocks(write_handles, writer));
        try!(self.write_entities(write_handles, writer));
        try!(self.write_objects(writer));
        try!(self.write_thumbnail(writer));
        try!(writer.write_code_pair(&CodePair::new_str(0, "EOF")));
        Ok(())
    }
    /// Writes a `Drawing` to disk, using a `BufWriter`.
    pub fn save_file(&self, file_name: &str) -> DxfResult<()> {
        self.save_file_internal(file_name, true)
    }
    /// Writes a `Drawing` as binary to disk, using a `BufWriter`.
    pub fn save_file_binary(&self, file_name: &str) -> DxfResult<()> {
        self.save_file_internal(file_name, false)
    }
    fn save_file_internal(&self, file_name: &str, as_ascii: bool) -> DxfResult<()> {
        let path = Path::new(file_name);
        let file = try!(File::create(&path));
        let buf_writer = BufWriter::new(file);
        let mut writer = match as_ascii {
            true => CodePairWriter::new_ascii_writer(buf_writer),
            false => CodePairWriter::new_binary_writer(buf_writer),
        };
        self.save_internal(&mut writer)
    }
    /// Writes a `Drawing` as DXB to anything that implements the `Write` trait.
    pub fn save_dxb<T>(&self, writer: &mut T) -> DxfResult<()>
        where T: Write {

        let mut writer = DxbWriter::new(writer);
        writer.write(self)
    }
    /// Writes a `Drawing` as DXB to disk, using a `BufWriter`.
    pub fn save_file_dxb(&self, file_name: &str) -> DxfResult<()> {
        let path = Path::new(file_name);
        let file = try!(File::create(&path));
        let mut buf_writer = BufWriter::new(file);
        self.save_dxb(&mut buf_writer)
    }
}

// private implementation
impl Drawing {
    fn write_classes<T>(&self, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        if self.classes.len() == 0 {
            return Ok(());
        }

        try!(writer.write_code_pair(&CodePair::new_str(0, "SECTION")));
        try!(writer.write_code_pair(&CodePair::new_str(2, "CLASSES")));
        for c in &self.classes {
            try!(c.write(&self.header.version, writer));
        }

        try!(writer.write_code_pair(&CodePair::new_str(0, "ENDSEC")));
        Ok(())
    }
    fn write_tables<T>(&self, write_handles: bool, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        try!(writer.write_code_pair(&CodePair::new_str(0, "SECTION")));
        try!(writer.write_code_pair(&CodePair::new_str(2, "TABLES")));
        try!(write_tables(&self, write_handles, writer));
        try!(writer.write_code_pair(&CodePair::new_str(0, "ENDSEC")));
        Ok(())
    }
    fn write_blocks<T>(&self, write_handles: bool, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        if self.blocks.len() == 0 {
            return Ok(());
        }

        try!(writer.write_code_pair(&CodePair::new_str(0, "SECTION")));
        try!(writer.write_code_pair(&CodePair::new_str(2, "BLOCKS")));
        for b in &self.blocks {
            try!(b.write(&self.header.version, write_handles, writer));
        }

        try!(writer.write_code_pair(&CodePair::new_str(0, "ENDSEC")));
        Ok(())
    }
    fn write_entities<T>(&self, write_handles: bool, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        try!(writer.write_code_pair(&CodePair::new_str(0, "SECTION")));
        try!(writer.write_code_pair(&CodePair::new_str(2, "ENTITIES")));
        for e in &self.entities {
            try!(e.write(&self.header.version, write_handles, writer));
        }

        try!(writer.write_code_pair(&CodePair::new_str(0, "ENDSEC")));
        Ok(())
    }
    fn write_objects<T>(&self, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        try!(writer.write_code_pair(&CodePair::new_str(0, "SECTION")));
        try!(writer.write_code_pair(&CodePair::new_str(2, "OBJECTS")));
        for o in &self.objects {
            try!(o.write(&self.header.version, writer));
        }

        try!(writer.write_code_pair(&CodePair::new_str(0, "ENDSEC")));
        Ok(())
    }
    fn write_thumbnail<T>(&self, writer: &mut CodePairWriter<T>) -> DxfResult<()>
        where T: Write {

        if &self.header.version >= &AcadVersion::R2000 {
            match self.thumbnail {
                Some(ref data) => {
                    try!(writer.write_code_pair(&CodePair::new_str(0, "SECTION")));
                    try!(writer.write_code_pair(&CodePair::new_str(2, "THUMBNAILIMAGE")));
                    let length = data.len() - 14;
                    try!(writer.write_code_pair(&CodePair::new_i32(90, length as i32)));
                    for s in data[14..].chunks(128) {
                        let mut line = String::new();
                        for b in s {
                            line.push_str(&format!("{:X}", b));
                        }
                        try!(writer.write_code_pair(&CodePair::new_string(310, &line)));
                    }
                    try!(writer.write_code_pair(&CodePair::new_str(0, "ENDSEC")));
                },
                None => (), // nothing to write
            }
        }
        Ok(())
    }
    fn read_sections<I>(drawing: &mut Drawing, iter: &mut PutBack<I>) -> DxfResult<()>
        where I: Iterator<Item = DxfResult<CodePair>> {

        loop {
            match iter.next() {
                Some(Ok(pair @ CodePair { code: 0, .. })) => {
                    match &*try!(pair.value.assert_string()) {
                        "EOF" => {
                            iter.put_back(Ok(pair));
                            break;
                        },
                        "SECTION" => {
                            match iter.next() {
                               Some(Ok(CodePair { code: 2, value: CodePairValue::Str(s) })) => {
                                    match &*s {
                                        "HEADER" => drawing.header = try!(Header::read(iter)),
                                        "CLASSES" => try!(Class::read_classes(drawing, iter)),
                                        "TABLES" => try!(drawing.read_section_item(iter, "TABLE", read_specific_table)),
                                        "BLOCKS" => try!(drawing.read_section_item(iter, "BLOCK", Block::read_block)),
                                        "ENTITIES" => try!(drawing.read_entities(iter)),
                                        "OBJECTS" => try!(drawing.read_objects(iter)),
                                        "THUMBNAILIMAGE" => { let _ = try!(drawing.read_thumbnail(iter)); },
                                        _ => try!(Drawing::swallow_section(iter)),
                                    }

                                    match iter.next() {
                                        Some(Ok(CodePair { code: 0, value: CodePairValue::Str(ref s) })) if s == "ENDSEC" => (),
                                        Some(Ok(pair)) => return Err(DxfError::UnexpectedCodePair(pair, String::from("expected 0/ENDSEC"))),
                                        Some(Err(e)) => return Err(e),
                                        None => return Err(DxfError::UnexpectedEndOfInput),
                                    }
                                },
                                Some(Ok(pair)) => return Err(DxfError::UnexpectedCodePair(pair, String::from("expected 2/<section-name>"))),
                                Some(Err(e)) => return Err(e),
                                None => return Err(DxfError::UnexpectedEndOfInput),
                            }
                        },
                        _ => return Err(DxfError::UnexpectedCodePair(pair, String::from("expected 0/SECTION"))),
                    }
                },
                Some(Ok(pair)) => return Err(DxfError::UnexpectedCodePair(pair, String::from("expected 0/SECTION or 0/EOF"))),
                Some(Err(e)) => return Err(e),
                None => break, // ideally should have been 0/EOF
            }
        }

        Ok(())
    }
    fn swallow_section<I>(iter: &mut PutBack<I>) -> DxfResult<()>
        where I: Iterator<Item = DxfResult<CodePair>> {

        loop {
            match iter.next() {
                Some(Ok(pair)) => {
                    if pair.code == 0 && try!(pair.value.assert_string()) == "ENDSEC" {
                        iter.put_back(Ok(pair));
                        break;
                    }
                },
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }

        Ok(())
    }
    fn read_entities<I>(&mut self, iter: &mut PutBack<I>) -> DxfResult<()>
        where I: Iterator<Item = DxfResult<CodePair>> {

        let mut iter = EntityIter { iter: iter };
        try!(iter.read_entities_into_vec(&mut self.entities));
        Ok(())
    }
    fn read_objects<I>(&mut self, iter: &mut PutBack<I>) -> DxfResult<()>
        where I: Iterator<Item = DxfResult<CodePair>> {

        let mut iter = PutBack::new(ObjectIter { iter: iter });
        loop {
            match iter.next() {
                Some(obj) => self.objects.push(obj),
                None => break,
            }
        }

        Ok(())
    }
    fn read_thumbnail<I>(&mut self, iter: &mut PutBack<I>) -> DxfResult<bool>
        where I: Iterator<Item = DxfResult<CodePair>> {

        // get the length; we don't really care about this since we'll just read whatever's there
        let length_pair = next_pair!(iter);
        let _length = match length_pair.code {
            90 => try!(length_pair.value.assert_i32()) as usize,
            _ => return Err(DxfError::UnexpectedCode(length_pair.code)),
        };

        // prepend the BMP header that always seems to be missing from DXF files
        let mut data = vec![
            'B' as u8, 'M' as u8, // magic number
            0x00, 0x00, 0x00, 0x00, // file length (calculated later)
            0x00, 0x00, // reserved
            0x00, 0x00, // reserved
            0x36, 0x04, 0x00, 0x00 // bit offset; always 1078
        ];
        let header_length = data.len();

        // read the hex data
        loop {
            match iter.next() {
                Some(Ok(pair @ CodePair { code: 0, .. })) => {
                    // likely 0/ENDSEC
                    iter.put_back(Ok(pair));
                    break;
                },
                Some(Ok(pair @ CodePair { code: 310, .. })) => { try!(parse_hex_string(&try!(pair.value.assert_string()), &mut data)); },
                Some(Ok(pair)) => { return Err(DxfError::UnexpectedCode(pair.code)); },
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }

        // set the length
        let length = data.len() - header_length;
        let mut length_bytes = vec![];
        LittleEndian::write_i32(&mut length_bytes, length as i32);
        data[2] = length_bytes[0];
        data[3] = length_bytes[1];
        data[4] = length_bytes[2];
        data[5] = length_bytes[3];

        self.thumbnail = Some(data);
        Ok(true)
    }
    fn read_section_item<I, F>(&mut self, iter: &mut PutBack<I>, item_type: &str, callback: F) -> DxfResult<()>
        where I: Iterator<Item = DxfResult<CodePair>>,
              F: Fn(&mut Drawing, &mut PutBack<I>) -> DxfResult<()> {

        loop {
            match iter.next() {
                Some(Ok(pair)) => {
                    if pair.code == 0 {
                        match &*try!(pair.value.assert_string()) {
                            "ENDSEC" => {
                                iter.put_back(Ok(pair));
                                break;
                            },
                            val => {
                                if val == item_type {
                                    try!(callback(self, iter));
                                }
                                else {
                                    return Err(DxfError::UnexpectedCodePair(pair, String::new()));
                                }
                            },
                        }
                    }
                    else {
                        return Err(DxfError::UnexpectedCodePair(pair, String::new()));
                    }
                },
                Some(Err(e)) => return Err(e),
                None => return Err(DxfError::UnexpectedEndOfInput),
            }
        }

        Ok(())
    }
    #[doc(hidden)]
    pub fn swallow_table<I>(iter: &mut PutBack<I>) -> DxfResult<()>
        where I: Iterator<Item = DxfResult<CodePair>> {

        loop {
            match iter.next() {
                Some(Ok(pair)) => {
                    if pair.code == 0 {
                        match &*try!(pair.value.assert_string()) {
                            "TABLE" | "ENDSEC" | "ENDTAB" => {
                                iter.put_back(Ok(pair));
                                break;
                            },
                            _ => (), // swallow the code pair
                        }
                    }
                }
                Some(Err(e)) => return Err(e),
                None => return Err(DxfError::UnexpectedEndOfInput),
            }
        }

        Ok(())
    }
}