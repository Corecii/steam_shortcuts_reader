//! `steam_vdf` is used to read certain vdf files that follow the format of `shortcuts.vdf`
//!
//! In normal use you will only be using `read_data`, `write_data` and the `ValveData` enum.
//!
//! # Examples
//! Read `shortcuts.vdf` file for a certain user, print out its entries, their properties,
//! remove the last entry, and write the new `shortcuts.vdf` back.
//!
//! ```
//! extern crate steam_vdf;
//!
//! use std::io::Write;
//! use std::fs::OpenOptions;
//! use std::path::Path;
//! use std::ffi::OsString;
//!
//! let shortcuts_path = Path::new("C:\\Program Files (x86)\\Steam\\userdata\\00000000\\config\\shortcuts.vdf");
//! let mut file = OpenOptions::new().read(true).open(shortcuts_path).expect("Could not open file for reading");
//! match steam_vdf::read_data(&mut file) {
//!     Err(err) => println!("Error reading file: {:?}", err),
//!     Ok(data) => match data {
//!         None => println!("File was empty, or it began with a data type not recognized."),
//!         Some(mut base_valve_data) => {
//!             match &mut base_valve_data {
//!                 &mut steam_vdf::ValveData::List(ref base_name, ref base_contents) => {
//!                     assert_eq!(*base_name, OsString::from("shortcuts"));
//!                     for shortcut in base_contents.iter() {
//!                         if let &steam_vdf::ValveData::List(ref shortcut_name, ref shortcut_contents) = shortcut {
//!                             println!("Shortcut: {:?}", shortcut_name);
//!                             for content in shortcut_contents {
//!                                 match content {
//!                                     &steam_vdf::ValveData::List(ref prop_name, _) => {
//!                                         println!("Name: {:?}; Value: [List]", prop_name);
//!                                     },
//!                                     &steam_vdf::ValveData::String(ref prop_name, ref prop_content) => {
//!                                         println!("Name: {:?}; Value: {:?}", prop_name, prop_content);
//!                                     },
//!                                     &steam_vdf::ValveData::Bytes4(ref prop_name, ref prop_content) => {
//!                                         println!("Name: {:?}; Value: {:02X}{:02X}{:02X}{:02X}", prop_name, prop_content[0], prop_content[1], prop_content[2], prop_content[3]);
//!                                     },
//!                                     &steam_vdf::ValveData::EndOfList => {
//!                                         println!("EndOfList");
//!                                     },
//!                                 }
//!                             }
//!                         }
//!                     }
//!                     base_contents.pop();
//!                 },
//!                 _ => println!("base data was not a list."),
//!             }
//!             let mut file = OpenOptions::new()
//!                 .read(false)
//!                 .write(true)
//!                 .truncate(true)
//!                 .create(true)
//!                 .open(shortcuts_path)
//!                 .expect("Could not open file for writing");
//!             steam_vdf::write_data(&mut file, &base_valve_data).expect("Could not write data to file");
//!             // Must manually write a final, extra, closing 0x08 for the file to be valid.
//!             file.write_all(&[0x08]).expect("Could not write data to file");
//!         },
//!     },
//! };
//! ```

use std::io;
use std::io::{Read, Write};
use std::ffi::{OsString, OsStr};

/// Represents a data type in a vdf file without data.
pub enum ValveDataType {
    List,
    String,
    Bytes4,
    EndOfList,
}

/// Represents data in a vdf file.
/// All data except the list terminator has a property name and property value.
pub enum ValveData {
    /// A list. Begins with `0x00`, ends with `EndOfList`, or `0x08`
    /// Internal format: `0x00` `string name` `list_contents` `EndOfList`
    List(OsString, Vec<ValveData>),
    /// A string. Begins with `0x01`. All standalone strings end with `0x00`. The string ValveData is simply two strings.
    /// Internal format: `0x01` `string name` `string value`
    String(OsString, OsString),
    /// 4 bytes. Begins with `0x02`.
    /// Internal format: `0x02` `string name` `4 bytes`
    Bytes4(OsString, [u8; 4]),
    /// Only considered a type for the use of parsing lists. This is always the last element in the list. Begins with `0x08`.
    /// Internal format: `0x08`
    EndOfList,
}

impl ValveData {
    /// Convert from data to type.
    pub fn data_type(&self) -> ValveDataType {
        match self {
            &ValveData::List(_, _) => ValveDataType::List,
            &ValveData::String(_, _) => ValveDataType::String,
            &ValveData::Bytes4(_, _) => ValveDataType::Bytes4,
            &ValveData::EndOfList => ValveDataType::EndOfList,
        }
    }
}

/// Returns the prefix for a specific data type
pub fn get_prefix_from_type(data_type: ValveDataType) -> u8 {
    match data_type {
        ValveDataType::List => 0x00,
        ValveDataType::String => 0x01,
        ValveDataType::Bytes4 => 0x02,
        ValveDataType::EndOfList => 0x08,
    }
}

/// Returns the data type for a specific prefix.
/// If the prefix byte is not valid, `None` is returned.
pub fn get_type_from_prefix(prefix: u8) -> Option<ValveDataType> {
    match prefix {
        0x00 => Some(ValveDataType::List),
        0x01 => Some(ValveDataType::String),
        0x02 => Some(ValveDataType::Bytes4),
        0x08 => Some(ValveDataType::EndOfList),
        _ => None,
    }
}

/// Reads a String, counting up all characters until it reaches `0x00` i.e. null.
/// Returns an `OsString`, but internally uses a `String` and `String::from_utf8_lossy`.
pub fn read_null_string<T: Read>(input: &mut T) -> io::Result<OsString> {
    let mut read_buf = [0; 1];
    let mut output_buf = vec![];
    loop {
        input.read_exact(&mut read_buf)?;
        if read_buf[0] == 0x00 {
            return Ok(OsString::from(String::from_utf8_lossy(output_buf.as_slice()).into_owned()));
        }
        output_buf.push(read_buf[0]);
    }
}

/// Reads the provided input, and returns a `ValveData` representing it. Generally, this will be a `ValveData::List`.
/// If the prefix for this data is invalid, then `Ok(None)` is returned.
/// If the prefix is valid and no errors were encountered while reading, then `Ok(Some(ValveData))` is returned.
/// If errors were encountered while reading, then `Err` is returned with an io error.
/// This does not read the `0x08` at the end of `shortcuts.vdf`.
pub fn read_data<T: Read>(input: &mut T) -> io::Result<Option<ValveData>> {
    let mut prefix_buf = [0; 1];
    input.read_exact(&mut prefix_buf)?;
    match get_type_from_prefix(prefix_buf[0]) {
        Some(ValveDataType::List) => {
            let name = read_null_string(input)?;
            let mut list = vec![];
            loop {
                let data = read_data(input)?;
                match data {
                    Some(ValveData::EndOfList) => break,
                    Some(valve_data) => list.push(valve_data),
                    None => continue,
                }
            }
            Ok(Some(ValveData::List(name, list)))
        },
        Some(ValveDataType::String) => {
            let name = read_null_string(input)?;
            let value = read_null_string(input)?;
            Ok(Some(ValveData::String(name, value)))
        },
        Some(ValveDataType::Bytes4) => {
            let name = read_null_string(input)?;
            let mut value_buf = [0; 4];
            input.read_exact(&mut value_buf)?;
            Ok(Some(ValveData::Bytes4(name, value_buf)))
        },
        Some(ValveDataType::EndOfList) => Ok(Some(ValveData::EndOfList)),
        None => Ok(None),
    }
}

/// Writes a String, then writes 0x00.
/// Takes an `OsStr`, but internally uses a `String` and `OsStr::to_string_lossy`.
pub fn write_null_string<T: Write>(output: &mut T, data: &OsStr) -> io::Result<()> {
    output.write_all(data.to_string_lossy().as_bytes())?;
    output.write_all(&[0x00])?;
    Ok(())
}

/// Writes to the given output the given ValveData.
/// * If it succeeds without errors, it returns `Ok(())`
/// * If it has errors, it returns `Err` with an io error.
///
/// This does not write the extra `0x08` at the end of `shortcuts.vdf`. In order to produce a valid file, you will need to write `0x08` when you're done.
///
/// Open the file you are working with in a hex editor and check if this will produce proper output. If the output is not proper, it may be discarded, and data may be lost.
pub fn write_data<T: Write>(output: &mut T, data: &ValveData) -> io::Result<()> {
    output.write_all(&[get_prefix_from_type(data.data_type())])?;
    match data {
        &ValveData::List(ref name, ref data_vec) => {
            write_null_string(output, &name)?;
            for data in data_vec {
                write_data(output, data)?;
            }
            write_data(output, &ValveData::EndOfList)?;
        },
        &ValveData::String(ref name, ref data) => {
            write_null_string(output, &name)?;
            write_null_string(output, &data)?;
        },
        &ValveData::Bytes4(ref name, ref data) => {
            write_null_string(output, &name)?;
            output.write_all(data)?;
        },
        &ValveData::EndOfList => (),  // only writes prefix, which is already done.
    };
    Ok(())
}
