//! Simple and easy CBOR serialization.

use std::{
    cmp,
    convert::{TryFrom, TryInto},
    io,
};

use crate::{Error, Result};

/// Recursion limit for nested Cbor objects.
const RECURSION_LIMIT: u32 = 1000;

/// Cbor type parametrised over list type and map type. Use one of the
/// conversion trait to convert language-native-type to a Cbor variant.
#[derive(Clone)]
pub enum Cbor {
    Major0(Info, u64),              // uint 0-23,24,25,26,27
    Major1(Info, u64),              // nint 0-23,24,25,26,27
    Major2(Info, Vec<u8>),          // byts 0-23,24,25,26,27,31
    Major3(Info, Vec<u8>),          // text 0-23,24,25,26,27,31
    Major4(Info, Vec<Cbor>),        // list 0-23,24,25,26,27,31
    Major5(Info, Vec<(Key, Cbor)>), // dict 0-23,24,25,26,27,31
    Major6(Info, Tag),              // tags similar to major0
    Major7(Info, SimpleValue),      // type refer SimpleValue
    Binary(Vec<u8>),                // for lazy decoding cbor data
}

impl Cbor {
    /// Serialize this cbor value.
    pub fn encode(&self, buf: &mut Vec<u8>) -> Result<usize> {
        self.do_encode(buf, 1)
    }

    fn do_encode(&self, buf: &mut Vec<u8>, depth: u32) -> Result<usize> {
        if depth > RECURSION_LIMIT {
            return err_at!(FailCbor, msg: "encode recursion limit exceeded");
        }

        let major = self.to_major_val();
        let n = match self {
            Cbor::Major0(info, num) => {
                let n = encode_hdr(major, *info, buf)?;
                n + encode_addnl(*num, buf)?
            }
            Cbor::Major1(info, num) => {
                let n = encode_hdr(major, *info, buf)?;
                n + encode_addnl(*num, buf)?
            }
            Cbor::Major2(info, byts) => {
                let n = encode_hdr(major, *info, buf)?;
                let m = encode_addnl(err_at!(FailConvert, u64::try_from(byts.len()))?, buf)?;
                buf.copy_from_slice(&byts);
                n + m + byts.len()
            }
            Cbor::Major3(info, text) => {
                let n = encode_hdr(major, *info, buf)?;
                let m = encode_addnl(err_at!(FailCbor, u64::try_from(text.len()))?, buf)?;
                buf.copy_from_slice(&text);
                n + m + text.len()
            }
            Cbor::Major4(info, list) => {
                let n = encode_hdr(major, *info, buf)?;
                let m = encode_addnl(err_at!(FailConvert, u64::try_from(list.len()))?, buf)?;
                let mut acc = 0;
                for x in list.iter() {
                    acc += x.do_encode(buf, depth + 1)?;
                }
                n + m + acc
            }
            Cbor::Major5(info, map) => {
                let n = encode_hdr(major, *info, buf)?;
                let m = encode_addnl(err_at!(FailConvert, u64::try_from(map.len()))?, buf)?;
                let mut acc = 0;
                for (key, val) in map.iter() {
                    let key: Cbor = key.clone().try_into()?;
                    acc += key.do_encode(buf, depth + 1)?;
                    acc += val.do_encode(buf, depth + 1)?;
                }
                n + m + acc
            }
            Cbor::Major6(info, tagg) => {
                let n = encode_hdr(major, *info, buf)?;
                let m = tagg.encode(buf)?;
                n + m
            }
            Cbor::Major7(info, sval) => {
                let n = encode_hdr(major, *info, buf)?;
                let m = sval.encode(buf)?;
                n + m
            }
            Cbor::Binary(data) => {
                buf.extend_from_slice(data);
                data.len()
            }
        };

        Ok(n)
    }

    /// Deserialize a bytes from reader `r` to Cbor value.
    pub fn decode<R: io::Read>(r: &mut R) -> Result<Cbor> {
        Cbor::do_decode(r, 1)
    }

    fn do_decode<R: io::Read>(r: &mut R, depth: u32) -> Result<Cbor> {
        if depth > RECURSION_LIMIT {
            return err_at!(FailCbor, msg: "decode recursion limt exceeded");
        }

        let (major, info) = decode_hdr(r)?;

        let val = match (major, info) {
            (0, info) => Cbor::Major0(info, decode_addnl(info, r)?),
            (1, info) => Cbor::Major1(info, decode_addnl(info, r)?),
            (2, Info::Indefinite) => {
                let mut data: Vec<u8> = Vec::default();
                loop {
                    match Cbor::do_decode(r, depth + 1)? {
                        Cbor::Major2(_, chunk) => data.extend_from_slice(&chunk),
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        _ => err_at!(FailConvert, msg: "expected byte chunk")?,
                    }
                }
                Cbor::Major2(info, data)
            }
            (2, info) => {
                let n: usize = err_at!(FailConvert, decode_addnl(info, r)?.try_into())?;
                let mut data = vec![0; n];
                err_at!(IOError, r.read(&mut data))?;
                Cbor::Major2(info, data)
            }
            (3, Info::Indefinite) => {
                let mut text: Vec<u8> = Vec::default();
                loop {
                    match Cbor::do_decode(r, depth + 1)? {
                        Cbor::Major3(_, chunk) => text.extend_from_slice(&chunk),
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        _ => err_at!(FailConvert, msg: "expected byte chunk")?,
                    }
                }
                Cbor::Major3(info, text)
            }
            (3, info) => {
                let n: usize = err_at!(FailConvert, decode_addnl(info, r)?.try_into())?;
                let mut text = vec![0; n];
                err_at!(IOError, r.read(&mut text))?;
                Cbor::Major3(info, text)
            }
            (4, Info::Indefinite) => {
                let mut list: Vec<Cbor> = vec![];
                loop {
                    match Cbor::do_decode(r, depth + 1)? {
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        item => list.push(item),
                    }
                }
                Cbor::Major4(info, list)
            }
            (4, info) => {
                let mut list: Vec<Cbor> = vec![];
                let n = decode_addnl(info, r)?;
                for _ in 0..n {
                    list.push(Cbor::do_decode(r, depth + 1)?);
                }
                Cbor::Major4(info, list)
            }
            (5, Info::Indefinite) => {
                let mut map: Vec<(Key, Cbor)> = Vec::default();
                loop {
                    let key = Cbor::do_decode(r, depth + 1)?.try_into()?;
                    let val = match Cbor::do_decode(r, depth + 1)? {
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        val => val,
                    };
                    map.push((key, val));
                }
                Cbor::Major5(info, map)
            }
            (5, info) => {
                let mut map: Vec<(Key, Cbor)> = Vec::default();
                let n = decode_addnl(info, r)?;
                for _ in 0..n {
                    let key = Cbor::do_decode(r, depth + 1)?.try_into()?;
                    let val = Cbor::do_decode(r, depth + 1)?;
                    map.push((key, val));
                }
                Cbor::Major5(info, map)
            }
            (6, info) => Cbor::Major6(info, Tag::decode(info, r)?),
            (7, info) => Cbor::Major7(info, SimpleValue::decode(info, r)?),
            _ => unreachable!(),
        };
        Ok(val)
    }

    fn to_major_val(&self) -> u8 {
        match self {
            Cbor::Major0(_, _) => 0,
            Cbor::Major1(_, _) => 1,
            Cbor::Major2(_, _) => 2,
            Cbor::Major3(_, _) => 3,
            Cbor::Major4(_, _) => 4,
            Cbor::Major5(_, _) => 5,
            Cbor::Major6(_, _) => 6,
            Cbor::Major7(_, _) => 7,
            Cbor::Binary(data) => (data[0] & 0xe0) >> 5,
        }
    }

    /// Convert cbor into optional value of type T.
    pub fn into_optional<T: TryFrom<Cbor, Error = Error>>(self) -> Result<Option<T>> {
        match self {
            Cbor::Major7(_, SimpleValue::Null) => Ok(None),
            val => Ok(Some(val.try_into()?)),
        }
    }
}

/// 5-bit value for additional info.
#[derive(Copy, Clone)]
pub enum Info {
    /// additional info is part of this info.
    Tiny(u8), // 0..=23
    /// additional info of 8-bit unsigned integer.
    U8,
    /// additional info of 16-bit unsigned integer.
    U16,
    /// additional info of 32-bit unsigned integer.
    U32,
    /// additional info of 64-bit unsigned integer.
    U64,
    /// Reserved.
    Reserved28,
    /// Reserved.
    Reserved29,
    /// Reserved.
    Reserved30,
    /// Indefinite encoding.
    Indefinite,
}

impl TryFrom<u8> for Info {
    type Error = Error;

    fn try_from(b: u8) -> Result<Info> {
        let val = match b {
            0..=23 => Info::Tiny(b),
            24 => Info::U8,
            25 => Info::U16,
            26 => Info::U32,
            27 => Info::U64,
            28 => Info::Reserved28,
            29 => Info::Reserved29,
            30 => Info::Reserved30,
            31 => Info::Indefinite,
            _ => err_at!(Fatal, msg: "unreachable")?,
        };

        Ok(val)
    }
}

impl From<u64> for Info {
    fn from(num: u64) -> Info {
        match num {
            0..=23 => Info::Tiny(num as u8),
            n if n <= (u8::MAX as u64) => Info::U8,
            n if n <= (u16::MAX as u64) => Info::U16,
            n if n <= (u32::MAX as u64) => Info::U32,
            _ => Info::U64,
        }
    }
}

impl TryFrom<usize> for Info {
    type Error = Error;

    fn try_from(num: usize) -> Result<Info> {
        Ok(err_at!(FailConvert, u64::try_from(num))?.into())
    }
}

fn encode_hdr(major: u8, info: Info, buf: &mut Vec<u8>) -> Result<usize> {
    let info = match info {
        Info::Tiny(val) if val <= 23 => val,
        Info::Tiny(val) => err_at!(FailCbor, msg: "{} > 23", val)?,
        Info::U8 => 24,
        Info::U16 => 25,
        Info::U32 => 26,
        Info::U64 => 27,
        Info::Reserved28 => 28,
        Info::Reserved29 => 29,
        Info::Reserved30 => 30,
        Info::Indefinite => 31,
    };
    buf.push((major as u8) << 5 | info);
    Ok(1)
}

fn decode_hdr<R: io::Read>(r: &mut R) -> Result<(u8, Info)> {
    let mut scratch = [0_u8; 8];
    err_at!(IOError, r.read(&mut scratch[..1]))?;

    let b = scratch[0];

    let major = (b & 0xe0) >> 5;
    let info = b & 0x1f;
    Ok((major, info.try_into()?))
}

fn encode_addnl(num: u64, buf: &mut Vec<u8>) -> Result<usize> {
    let mut scratch = [0_u8; 8];
    let n = match num {
        0..=23 => 0,
        n if n <= (u8::MAX as u64) => {
            scratch.copy_from_slice(&(n as u8).to_be_bytes());
            1
        }
        n if n <= (u16::MAX as u64) => {
            scratch.copy_from_slice(&(n as u16).to_be_bytes());
            2
        }
        n if n <= (u32::MAX as u64) => {
            scratch.copy_from_slice(&(n as u32).to_be_bytes());
            4
        }
        n => {
            scratch.copy_from_slice(&n.to_be_bytes());
            8
        }
    };
    buf.copy_from_slice(&scratch[..n]);
    Ok(n)
}

fn decode_addnl<R: io::Read>(info: Info, r: &mut R) -> Result<u64> {
    let mut scratch = [0_u8; 8];
    let num = match info {
        Info::Tiny(num) => num as u64,
        Info::U8 => {
            err_at!(IOError, r.read(&mut scratch[..1]))?;
            u8::from_be_bytes(scratch[..1].try_into().unwrap()) as u64
        }
        Info::U16 => {
            err_at!(IOError, r.read(&mut scratch[..2]))?;
            u16::from_be_bytes(scratch[..2].try_into().unwrap()) as u64
        }
        Info::U32 => {
            err_at!(IOError, r.read(&mut scratch[..4]))?;
            u32::from_be_bytes(scratch[..4].try_into().unwrap()) as u64
        }
        Info::U64 => {
            err_at!(IOError, r.read(&mut scratch[..8]))?;
            u64::from_be_bytes(scratch[..8].try_into().unwrap()) as u64
        }
        Info::Indefinite => 0,
        _ => err_at!(FailCbor, msg: "no additional value")?,
    };
    Ok(num)
}

/// Major type 7, simple-value
#[derive(Copy, Clone)]
pub enum SimpleValue {
    /// 0..=19 and 28..=30 and 32..=255 unassigned
    Unassigned,
    /// Boolean type, value true.
    True, // 20, tiny simple-value
    /// Boolean type, value false.
    False, // 21, tiny simple-value
    /// Null unitary type, can be used in place of optional types.
    Null, // 22, tiny simple-value
    /// Undefined unitary type.
    Undefined, // 23, tiny simple-value
    /// Reserver.
    Reserved24(u8), // 24, one-byte simple-value
    /// 16-bit floating point.
    F16(u16), // 25, not-implemented
    /// 32-bit floating point.
    F32(f32), // 26, single-precision float
    /// 64-bit floating point.
    F64(f64), // 27, single-precision float
    /// Break stop for indefinite encoding.
    Break, // 31
}

impl TryFrom<SimpleValue> for Cbor {
    type Error = Error;

    fn try_from(sval: SimpleValue) -> Result<Cbor> {
        use SimpleValue::*;

        let val = match sval {
            Unassigned => err_at!(FailConvert, msg: "simple-value-unassigned")?,
            True => Cbor::Major7(Info::Tiny(20), sval),
            False => Cbor::Major7(Info::Tiny(21), sval),
            Null => Cbor::Major7(Info::Tiny(22), sval),
            Undefined => err_at!(FailConvert, msg: "simple-value-undefined")?,
            Reserved24(_) => err_at!(FailConvert, msg: "simple-value-unassigned1")?,
            F16(_) => err_at!(FailConvert, msg: "simple-value-f16")?,
            F32(_) => Cbor::Major7(Info::U32, sval),
            F64(_) => Cbor::Major7(Info::U64, sval),
            Break => err_at!(FailConvert, msg: "simple-value-break")?,
        };

        Ok(val)
    }
}

impl SimpleValue {
    fn encode(&self, buf: &mut Vec<u8>) -> Result<usize> {
        use SimpleValue::*;

        let mut scratch = [0_u8; 8];
        let n = match self {
            True | False | Null | Undefined | Break | Unassigned => 0,
            Reserved24(num) => {
                scratch[0] = *num;
                1
            }
            F16(f) => {
                scratch.copy_from_slice(&f.to_be_bytes());
                2
            }
            F32(f) => {
                scratch.copy_from_slice(&f.to_be_bytes());
                4
            }
            F64(f) => {
                scratch.copy_from_slice(&f.to_be_bytes());
                8
            }
        };
        buf.copy_from_slice(&scratch[..n]);
        Ok(n)
    }

    fn decode<R: io::Read>(info: Info, r: &mut R) -> Result<SimpleValue> {
        let mut scratch = [0_u8; 8];
        let val = match info {
            Info::Tiny(20) => SimpleValue::True,
            Info::Tiny(21) => SimpleValue::False,
            Info::Tiny(22) => SimpleValue::Null,
            Info::Tiny(23) => err_at!(FailCbor, msg: "simple-value-undefined")?,
            Info::Tiny(_) => err_at!(FailCbor, msg: "simple-value-unassigned")?,
            Info::U8 => err_at!(FailCbor, msg: "simple-value-unassigned1")?,
            Info::U16 => err_at!(FailCbor, msg: "simple-value-f16")?,
            Info::U32 => {
                err_at!(IOError, r.read(&mut scratch[..4]))?;
                let val = f32::from_be_bytes(scratch[..4].try_into().unwrap());
                SimpleValue::F32(val)
            }
            Info::U64 => {
                err_at!(IOError, r.read(&mut scratch[..8]))?;
                let val = f64::from_be_bytes(scratch[..8].try_into().unwrap());
                SimpleValue::F64(val)
            }
            Info::Reserved28 => err_at!(FailCbor, msg: "simple-value-reserved")?,
            Info::Reserved29 => err_at!(FailCbor, msg: "simple-value-reserved")?,
            Info::Reserved30 => err_at!(FailCbor, msg: "simple-value-reserved")?,
            Info::Indefinite => err_at!(FailCbor, msg: "simple-value-break")?,
        };
        Ok(val)
    }
}

/// Major type 6, Tag values.
#[derive(Clone)]
pub enum Tag {
    /// Don't worry about the type wrapped by the tag-value, just encode
    /// the tag and leave the subsequent encoding at caller's discretion.
    Value(u64),
}

impl From<Tag> for u64 {
    fn from(tag: Tag) -> u64 {
        match tag {
            Tag::Value(val) => val,
        }
    }
}

impl From<u64> for Tag {
    fn from(tag: u64) -> Tag {
        Tag::Value(tag)
    }
}

impl Tag {
    fn encode(&self, buf: &mut Vec<u8>) -> Result<usize> {
        match self {
            Tag::Value(val) => encode_addnl(*val, buf),
        }
    }

    fn decode<R: io::Read>(info: Info, r: &mut R) -> Result<Tag> {
        let tag = Tag::Value(decode_addnl(info, r)?);
        Ok(tag)
    }
}

/// Possible types that can be used as key in cbor-map.
#[derive(Clone)]
pub enum Key {
    Bool(bool),
    U64(u64),
    N64(i64),
    F32(f32),
    F64(f64),
    Bytes(Vec<u8>),
    Text(String),
}

impl Key {
    fn to_type_order(&self) -> usize {
        use Key::*;

        match self {
            Bool(_) => 4,
            U64(_) => 8,
            N64(_) => 12,
            F32(_) => 16,
            F64(_) => 20,
            Bytes(_) => 24,
            Text(_) => 28,
        }
    }
}

impl Eq for Key {}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        use Key::*;

        match (self, other) {
            (U64(a), U64(b)) => a == b,
            (N64(a), N64(b)) => a == b,
            (Bytes(a), Bytes(b)) => a == b,
            (Text(a), Text(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (F32(a), F32(b)) => a == b,
            (F64(a), F64(b)) => a == b,
            (_, _) => false,
        }
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Key) -> cmp::Ordering {
        use Key::*;

        let (a, b) = (self.to_type_order(), other.to_type_order());
        if a == b {
            match (self, other) {
                (U64(a), U64(b)) => a.cmp(b),
                (N64(a), N64(b)) => a.cmp(b),
                (Bytes(a), Bytes(b)) => a.cmp(b),
                (Text(a), Text(b)) => a.cmp(b),
                (Bool(a), Bool(b)) => a.cmp(b),
                (F32(a), F32(b)) => a.total_cmp(b),
                (F64(a), F64(b)) => a.total_cmp(b),
                (_, _) => unreachable!(),
            }
        } else {
            a.cmp(&b)
        }
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<Key> for Cbor {
    type Error = Error;

    fn try_from(key: Key) -> Result<Cbor> {
        let val = match key {
            Key::U64(key) => Cbor::Major0(key.into(), key),
            Key::N64(key) if key >= 0 => {
                let val = err_at!(FailConvert, u64::try_from(key))?;
                Cbor::Major0(val.into(), val)
            }
            Key::N64(key) => {
                let val = err_at!(FailConvert, u64::try_from(key.abs() - 1))?;
                Cbor::Major1(val.into(), val)
            }
            Key::Bytes(key) => Cbor::Major2(err_at!(FailConvert, key.len().try_into())?, key),
            Key::Text(key) => Cbor::Major3(err_at!(FailConvert, key.len().try_into())?, key.into()),
            Key::Bool(true) => SimpleValue::True.try_into()?,
            Key::Bool(false) => SimpleValue::False.try_into()?,
            Key::F32(key) => SimpleValue::F32(key).try_into()?,
            Key::F64(key) => SimpleValue::F64(key).try_into()?,
        };

        Ok(val)
    }
}

impl TryFrom<Cbor> for Key {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<Key> {
        use std::str::from_utf8;

        let key = match val {
            Cbor::Major0(_, key) => Key::U64(key),
            Cbor::Major1(_, key) => Key::N64(-err_at!(FailConvert, i64::try_from(key + 1))?),
            Cbor::Major2(_, key) => Key::Bytes(key),
            Cbor::Major3(_, key) => Key::Text(err_at!(FailConvert, from_utf8(&key))?.to_string()),
            Cbor::Major7(_, SimpleValue::True) => Key::Bool(true),
            Cbor::Major7(_, SimpleValue::False) => Key::Bool(false),
            Cbor::Major7(_, SimpleValue::F32(key)) => Key::F32(key),
            Cbor::Major7(_, SimpleValue::F64(key)) => Key::F64(key),
            _ => err_at!(FailKey, msg: "cbor not a valid key")?,
        };

        Ok(key)
    }
}

impl<T: Clone + Into<Cbor>, const N: usize> TryFrom<[T; N]> for Cbor {
    type Error = Error;

    fn try_from(arr: [T; N]) -> Result<Cbor> {
        let info = err_at!(FailConvert, u64::try_from(arr.len()))?.into();
        Ok(Cbor::Major4(
            info,
            arr.iter().map(|x| x.clone().into()).collect(),
        ))
    }
}

impl<T: Copy + Default + TryFrom<Cbor, Error = Error>, const N: usize> TryFrom<Cbor> for [T; N] {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<[T; N]> {
        let mut arr = [T::default(); N];
        let n = arr.len();
        match val {
            Cbor::Major4(_, data) if n == data.len() => {
                for (i, item) in data.into_iter().enumerate() {
                    arr[i] = item.try_into()?;
                }
                Ok(arr)
            }
            Cbor::Major4(_, data) => {
                err_at!(FailConvert, msg: "different array arity {} {}", n, data.len())
            }
            _ => err_at!(FailKey, msg: "not an list"),
        }
    }
}

impl TryFrom<bool> for Cbor {
    type Error = Error;

    fn try_from(val: bool) -> Result<Cbor> {
        match val {
            true => SimpleValue::True.try_into(),
            false => SimpleValue::False.try_into(),
        }
    }
}

impl TryFrom<Cbor> for bool {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<bool> {
        match val {
            Cbor::Major7(_, SimpleValue::True) => Ok(true),
            Cbor::Major7(_, SimpleValue::False) => Ok(false),
            _ => err_at!(FailConvert, msg: "not a bool"),
        }
    }
}

impl TryFrom<f32> for Cbor {
    type Error = Error;

    fn try_from(val: f32) -> Result<Cbor> {
        SimpleValue::F32(val).try_into()
    }
}

impl TryFrom<Cbor> for f32 {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<f32> {
        match val {
            Cbor::Major7(_, SimpleValue::F32(val)) => Ok(val),
            _ => err_at!(FailConvert, msg: "not f32"),
        }
    }
}

impl TryFrom<f64> for Cbor {
    type Error = Error;

    fn try_from(val: f64) -> Result<Cbor> {
        SimpleValue::F64(val).try_into()
    }
}

impl TryFrom<Cbor> for f64 {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<f64> {
        match val {
            Cbor::Major7(_, SimpleValue::F64(val)) => Ok(val),
            _ => err_at!(FailConvert, msg: "not f64"),
        }
    }
}

impl TryFrom<i64> for Cbor {
    type Error = Error;

    fn try_from(val: i64) -> Result<Cbor> {
        if val >= 0 {
            Ok(err_at!(FailConvert, u64::try_from(val))?.try_into()?)
        } else {
            let val = err_at!(FailConvert, u64::try_from(val.abs() - 1))?;
            let info = val.into();
            Ok(Cbor::Major1(info, val))
        }
    }
}

impl TryFrom<Cbor> for i64 {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<i64> {
        let val = match val {
            Cbor::Major0(_, val) => err_at!(FailConvert, i64::try_from(val))?,
            Cbor::Major1(_, val) => -err_at!(FailConvert, i64::try_from(val + 1))?,
            _ => err_at!(FailConvert, msg: "not a number")?,
        };
        Ok(val)
    }
}

impl TryFrom<u64> for Cbor {
    type Error = Error;

    fn try_from(val: u64) -> Result<Cbor> {
        Ok(Cbor::Major0(val.into(), val))
    }
}

impl TryFrom<Cbor> for u64 {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<u64> {
        match val {
            Cbor::Major0(_, val) => Ok(val),
            _ => err_at!(FailConvert, msg: "not a number"),
        }
    }
}

impl TryFrom<usize> for Cbor {
    type Error = Error;

    fn try_from(val: usize) -> Result<Cbor> {
        let val = err_at!(FailConvert, u64::try_from(val))?;
        Ok(val.try_into()?)
    }
}

impl TryFrom<Cbor> for usize {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<usize> {
        match val {
            Cbor::Major0(_, val) => err_at!(FailConvert, usize::try_from(val)),
            _ => err_at!(FailConvert, msg: "not a number"),
        }
    }
}

impl TryFrom<isize> for Cbor {
    type Error = Error;

    fn try_from(val: isize) -> Result<Cbor> {
        err_at!(FailConvert, i64::try_from(val))?.try_into()
    }
}

impl TryFrom<Cbor> for isize {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<isize> {
        let val = match val {
            Cbor::Major0(_, val) => err_at!(FailConvert, isize::try_from(val))?,
            Cbor::Major1(_, val) => -err_at!(FailConvert, isize::try_from(val + 1))?,
            _ => err_at!(FailConvert, msg: "not a number")?,
        };
        Ok(val)
    }
}

impl<'a> TryFrom<&'a [u8]> for Cbor {
    type Error = Error;

    fn try_from(val: &'a [u8]) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(val.len()))?;
        Ok(Cbor::Major2(n.into(), val.to_vec()))
    }
}

impl TryFrom<Cbor> for Vec<u8> {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<Vec<u8>> {
        match val {
            Cbor::Major2(_, val) => Ok(val),
            _ => err_at!(FailConvert, msg: "not bytes"),
        }
    }
}

impl<'a> TryFrom<&'a str> for Cbor {
    type Error = Error;

    fn try_from(val: &'a str) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(val.len()))?;
        Ok(Cbor::Major3(n.into(), val.as_bytes().to_vec()))
    }
}

impl TryFrom<Cbor> for String {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<String> {
        use std::str::from_utf8;

        match val {
            Cbor::Major3(_, val) => Ok(err_at!(FailConvert, from_utf8(&val))?.to_string()),
            _ => err_at!(FailConvert, msg: "not utf8-string"),
        }
    }
}

impl<T: TryInto<Cbor, Error = Error>> TryFrom<Vec<T>> for Cbor {
    type Error = Error;

    fn try_from(val: Vec<T>) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(val.len()))?;
        let mut arr = vec![];
        for item in val.into_iter() {
            arr.push(item.try_into()?)
        }
        Ok(Cbor::Major4(n.into(), arr))
    }
}

impl<T: TryFrom<Cbor, Error = Error>> TryFrom<Cbor> for Vec<T> {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<Vec<T>> {
        match val {
            Cbor::Major4(_, data) => {
                let mut arr = vec![];
                for item in data.into_iter() {
                    arr.push(item.try_into()?)
                }
                Ok(arr)
            }
            _ => err_at!(FailConvert, msg: "not a vector"),
        }
    }
}

impl TryFrom<Vec<Cbor>> for Cbor {
    type Error = Error;

    fn try_from(val: Vec<Cbor>) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(val.len()))?;
        Ok(Cbor::Major4(n.into(), val))
    }
}

impl TryFrom<Cbor> for Vec<Cbor> {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<Vec<Cbor>> {
        match val {
            Cbor::Major4(_, data) => Ok(data),
            _ => err_at!(FailConvert, msg: "not a vector"),
        }
    }
}

impl TryFrom<Vec<(Key, Cbor)>> for Cbor {
    type Error = Error;

    fn try_from(val: Vec<(Key, Cbor)>) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(val.len()))?;
        Ok(Cbor::Major5(n.into(), val))
    }
}

impl TryFrom<Cbor> for Vec<(Key, Cbor)> {
    type Error = Error;

    fn try_from(val: Cbor) -> Result<Vec<(Key, Cbor)>> {
        match val {
            Cbor::Major5(_, data) => Ok(data),
            _ => err_at!(FailConvert, msg: "not a map"),
        }
    }
}

impl<T: TryInto<Cbor, Error = Error>> TryFrom<Option<T>> for Cbor {
    type Error = Error;

    fn try_from(val: Option<T>) -> Result<Cbor> {
        match val {
            Some(val) => val.try_into(),
            None => SimpleValue::Null.try_into(),
        }
    }
}
