//! Simple and easy CBOR serialization.

use std::{
    cmp,
    convert::{TryFrom, TryInto},
    io,
};

use crate::{Error, Result};

macro_rules! read {
    ($r:ident, $buf:expr) => {
        err_at!(IOError, $r.read_exact($buf))?
    };
}

macro_rules! write {
    ($w:ident, $buf:expr) => {
        err_at!(IOError, $w.write($buf))?
    };
}

pub trait IntoCbor {
    fn into_cbor(self) -> Result<Cbor>;
}

pub trait FromCbor: Sized {
    fn from_cbor(val: Cbor) -> Result<Self>;
}

/// Recursion limit for nested Cbor objects.
const RECURSION_LIMIT: u32 = 1000;

/// Cbor type parametrised over list type and map type. Use one of the
/// conversion trait to convert language-native-type to a Cbor variant.
#[derive(Clone, Eq, PartialEq)]
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
    pub fn encode<W>(&self, w: &mut W) -> Result<usize>
    where
        W: io::Write,
    {
        self.do_encode(w, 1)
    }

    fn do_encode<W>(&self, w: &mut W, depth: u32) -> Result<usize>
    where
        W: io::Write,
    {
        if depth > RECURSION_LIMIT {
            return err_at!(FailCbor, msg: "encode recursion limit exceeded");
        }

        let major = self.to_major_val();
        let n = match self {
            Cbor::Major0(info, num) => {
                let n = encode_hdr(major, *info, w)?;
                n + encode_addnl(*num, w)?
            }
            Cbor::Major1(info, num) => {
                let n = encode_hdr(major, *info, w)?;
                n + encode_addnl(*num, w)?
            }
            Cbor::Major2(info, byts) => {
                let n = encode_hdr(major, *info, w)?;
                let m = encode_addnl(err_at!(FailConvert, u64::try_from(byts.len()))?, w)?;
                write!(w, &byts);
                n + m + byts.len()
            }
            Cbor::Major3(info, text) => {
                let n = encode_hdr(major, *info, w)?;
                let m = encode_addnl(err_at!(FailCbor, u64::try_from(text.len()))?, w)?;
                write!(w, &text);
                n + m + text.len()
            }
            Cbor::Major4(info, list) => {
                let n = encode_hdr(major, *info, w)?;
                let m = encode_addnl(err_at!(FailConvert, u64::try_from(list.len()))?, w)?;
                let mut acc = 0;
                for x in list.iter() {
                    acc += x.do_encode(w, depth + 1)?;
                }
                n + m + acc
            }
            Cbor::Major5(info, map) => {
                let n = encode_hdr(major, *info, w)?;
                let m = encode_addnl(err_at!(FailConvert, u64::try_from(map.len()))?, w)?;
                let mut acc = 0;
                for (key, val) in map.iter() {
                    let key = key.clone().into_cbor()?;
                    acc += key.do_encode(w, depth + 1)?;
                    acc += val.do_encode(w, depth + 1)?;
                }
                n + m + acc
            }
            Cbor::Major6(info, tag) => {
                let n = encode_hdr(major, *info, w)?;
                let m = Tag::encode(tag, w)?;
                n + m
            }
            Cbor::Major7(info, sval) => {
                let n = encode_hdr(major, *info, w)?;
                let m = SimpleValue::encode(sval, w)?;
                n + m
            }
            Cbor::Binary(data) => {
                write!(w, data);
                data.len()
            }
        };

        Ok(n)
    }

    /// Deserialize a bytes from reader `r` to Cbor value.
    pub fn decode<R>(r: &mut R) -> Result<(Cbor, usize)>
    where
        R: io::Read,
    {
        Cbor::do_decode(r, 1)
    }

    fn do_decode<R>(r: &mut R, depth: u32) -> Result<(Cbor, usize)>
    where
        R: io::Read,
    {
        if depth > RECURSION_LIMIT {
            return err_at!(FailCbor, msg: "decode recursion limt exceeded");
        }

        let (major, info, n) = decode_hdr(r)?;

        let (val, m) = match (major, info) {
            (0, info) => {
                let (val, m) = decode_addnl(info, r)?;
                (Cbor::Major0(info, val), m)
            }
            (1, info) => {
                let (val, m) = decode_addnl(info, r)?;
                (Cbor::Major1(info, val), m)
            }
            (2, Info::Indefinite) => {
                let mut data: Vec<u8> = Vec::default();
                let mut m = 0_usize;
                loop {
                    let (val, k) = Cbor::do_decode(r, depth + 1)?;
                    match val {
                        Cbor::Major2(_, chunk) => data.extend_from_slice(&chunk),
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        _ => err_at!(FailConvert, msg: "expected byte chunk")?,
                    }
                    m += k;
                }
                (Cbor::Major2(info, data), m)
            }
            (2, info) => {
                let (val, m) = decode_addnl(info, r)?;
                let len: usize = err_at!(FailConvert, val.try_into())?;
                let mut data = vec![0; len];
                read!(r, &mut data);
                (Cbor::Major2(info, data), m + len)
            }
            (3, Info::Indefinite) => {
                let mut text: Vec<u8> = Vec::default();
                let mut m = 0_usize;
                loop {
                    let (val, k) = Cbor::do_decode(r, depth + 1)?;
                    match val {
                        Cbor::Major3(_, chunk) => text.extend_from_slice(&chunk),
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        _ => err_at!(FailConvert, msg: "expected byte chunk")?,
                    }
                    m += k;
                }
                (Cbor::Major3(info, text), m)
            }
            (3, info) => {
                let (val, m) = decode_addnl(info, r)?;
                let len: usize = err_at!(FailConvert, val.try_into())?;
                let mut text = vec![0; len];
                read!(r, &mut text);
                (Cbor::Major3(info, text), m + len)
            }
            (4, Info::Indefinite) => {
                let mut list: Vec<Cbor> = vec![];
                let mut m = 0_usize;
                loop {
                    let (val, k) = Cbor::do_decode(r, depth + 1)?;
                    match val {
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        item => list.push(item),
                    }
                    m += k;
                }
                (Cbor::Major4(info, list), m)
            }
            (4, info) => {
                let mut list: Vec<Cbor> = vec![];
                let (len, mut m) = decode_addnl(info, r)?;
                for _ in 0..len {
                    let (val, k) = Cbor::do_decode(r, depth + 1)?;
                    list.push(val);
                    m += k;
                }
                (Cbor::Major4(info, list), m)
            }
            (5, Info::Indefinite) => {
                let mut map: Vec<(Key, Cbor)> = Vec::default();
                let mut m = 0_usize;
                loop {
                    let (key, j) = Cbor::do_decode(r, depth + 1)?;
                    let (val, k) = Cbor::do_decode(r, depth + 1)?;
                    let val = match val {
                        Cbor::Major7(_, SimpleValue::Break) => break,
                        val => val,
                    };
                    map.push((Key::from_cbor(key)?, val));
                    m += j + k;
                }
                (Cbor::Major5(info, map), m)
            }
            (5, info) => {
                let mut map: Vec<(Key, Cbor)> = Vec::default();
                let (len, mut m) = decode_addnl(info, r)?;
                for _ in 0..len {
                    let (key, j) = Cbor::do_decode(r, depth + 1)?;
                    let (val, k) = Cbor::do_decode(r, depth + 1)?;
                    map.push((Key::from_cbor(key)?, val));
                    m += j + k;
                }
                (Cbor::Major5(info, map), m)
            }
            (6, info) => {
                let (tag, m) = Tag::decode(info, r)?;
                (Cbor::Major6(info, tag), m)
            }
            (7, info) => {
                let (sval, m) = SimpleValue::decode(info, r)?;
                (Cbor::Major7(info, sval), m)
            }
            _ => unreachable!(),
        };

        Ok((val, (m + n)))
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

    pub fn from_bytes(val: Vec<u8>) -> Result<Self> {
        let n = err_at!(FailConvert, u64::try_from(val.len()))?;
        Ok(Cbor::Major2(n.into(), val))
    }

    pub fn into_bytes(self) -> Result<Vec<u8>> {
        match self {
            Cbor::Major2(_, val) => Ok(val),
            _ => err_at!(FailConvert, msg: "not bytes"),
        }
    }
}

/// 5-bit value for additional info.
#[derive(Copy, Clone, Eq, PartialEq)]
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

fn encode_hdr<W>(major: u8, info: Info, w: &mut W) -> Result<usize>
where
    W: io::Write,
{
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
    write!(w, &[(major as u8) << 5 | info]);
    Ok(1)
}

fn decode_hdr<R>(r: &mut R) -> Result<(u8, Info, usize)>
where
    R: io::Read,
{
    let mut scratch = [0_u8; 8];
    read!(r, &mut scratch[..1]);

    let b = scratch[0];

    let major = (b & 0xe0) >> 5;
    let info = b & 0x1f;
    Ok((major, info.try_into()?, 1 /* only 1-byte read */))
}

fn encode_addnl<W>(num: u64, w: &mut W) -> Result<usize>
where
    W: io::Write,
{
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
    write!(w, &scratch[..n]);
    Ok(n)
}

fn decode_addnl<R>(info: Info, r: &mut R) -> Result<(u64, usize)>
where
    R: io::Read,
{
    let mut scratch = [0_u8; 8];
    let (num, n) = match info {
        Info::Tiny(num) => (num as u64, 0),
        Info::U8 => {
            read!(r, &mut scratch[..1]);
            (
                u8::from_be_bytes(scratch[..1].try_into().unwrap()) as u64,
                1,
            )
        }
        Info::U16 => {
            read!(r, &mut scratch[..2]);
            (
                u16::from_be_bytes(scratch[..2].try_into().unwrap()) as u64,
                2,
            )
        }
        Info::U32 => {
            read!(r, &mut scratch[..4]);
            (
                u32::from_be_bytes(scratch[..4].try_into().unwrap()) as u64,
                4,
            )
        }
        Info::U64 => {
            read!(r, &mut scratch[..8]);
            (
                u64::from_be_bytes(scratch[..8].try_into().unwrap()) as u64,
                8,
            )
        }
        Info::Indefinite => (0, 0),
        _ => err_at!(FailCbor, msg: "no additional value")?,
    };
    Ok((num, n))
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

impl Eq for SimpleValue {}

impl PartialEq for SimpleValue {
    fn eq(&self, other: &Self) -> bool {
        let a = self.to_type_order();
        let b = other.to_type_order();
        a == b
    }
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
    fn to_type_order(&self) -> usize {
        use SimpleValue::*;

        match self {
            Unassigned => 4,
            True => 8,
            False => 12,
            Null => 16,
            Undefined => 20,
            Reserved24(_) => 24,
            F16(_) => 28,
            F32(_) => 32,
            F64(_) => 36,
            Break => 40,
        }
    }

    fn encode<W>(sval: &SimpleValue, w: &mut W) -> Result<usize>
    where
        W: io::Write,
    {
        use SimpleValue::*;

        let mut scratch = [0_u8; 8];
        let n = match sval {
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
        write!(w, &scratch[..n]);
        Ok(n)
    }

    fn decode<R>(info: Info, r: &mut R) -> Result<(SimpleValue, usize)>
    where
        R: io::Read,
    {
        let mut scratch = [0_u8; 8];
        let (val, n) = match info {
            Info::Tiny(20) => (SimpleValue::True, 0),
            Info::Tiny(21) => (SimpleValue::False, 0),
            Info::Tiny(22) => (SimpleValue::Null, 0),
            Info::Tiny(23) => err_at!(FailCbor, msg: "simple-value-undefined")?,
            Info::Tiny(_) => err_at!(FailCbor, msg: "simple-value-unassigned")?,
            Info::U8 => err_at!(FailCbor, msg: "simple-value-unassigned1")?,
            Info::U16 => err_at!(FailCbor, msg: "simple-value-f16")?,
            Info::U32 => {
                read!(r, &mut scratch[..4]);
                let val = f32::from_be_bytes(scratch[..4].try_into().unwrap());
                (SimpleValue::F32(val), 4)
            }
            Info::U64 => {
                read!(r, &mut scratch[..8]);
                let val = f64::from_be_bytes(scratch[..8].try_into().unwrap());
                (SimpleValue::F64(val), 8)
            }
            Info::Reserved28 => err_at!(FailCbor, msg: "simple-value-reserved")?,
            Info::Reserved29 => err_at!(FailCbor, msg: "simple-value-reserved")?,
            Info::Reserved30 => err_at!(FailCbor, msg: "simple-value-reserved")?,
            Info::Indefinite => err_at!(FailCbor, msg: "simple-value-break")?,
        };
        Ok((val, n))
    }
}

/// Major type 6, Tag values.
#[derive(Clone, Eq, PartialEq)]
pub enum Tag {
    /// Tag 39, used as identifier marker for multiple cbor-type.
    Identifier(Box<Cbor>),
    /// Don't worry about the type wrapped by the tag-value, just encode
    /// the tag and leave the subsequent encoding at caller's discretion.
    Value(u64),
}

impl From<Tag> for Cbor {
    fn from(tag: Tag) -> Cbor {
        let num = tag.to_tag_value();
        Cbor::Major6(num.into(), tag)
    }
}

impl Tag {
    pub fn from_value(value: u64) -> Tag {
        Tag::Value(value)
    }

    pub fn from_identifier(value: Cbor) -> Tag {
        Tag::Identifier(Box::new(value))
    }

    pub fn to_tag_value(&self) -> u64 {
        match self {
            Tag::Identifier(_) => 39,
            Tag::Value(val) => *val,
        }
    }

    fn encode<W>(tag: &Tag, w: &mut W) -> Result<usize>
    where
        W: io::Write,
    {
        let num = tag.to_tag_value();
        let mut n = encode_addnl(num, w)?;
        n += match tag {
            Tag::Identifier(val) => val.encode(w)?,
            Tag::Value(_) => 0,
        };

        Ok(n)
    }

    fn decode<R>(info: Info, r: &mut R) -> Result<(Tag, usize)>
    where
        R: io::Read,
    {
        let (tag, n) = decode_addnl(info, r)?;
        let (tag, m) = match tag {
            39 => {
                let (val, m) = Cbor::decode(r)?;
                (Tag::Identifier(Box::new(val)), m)
            }
            val => (Tag::Value(val), 0),
        };
        Ok((tag, m + n))
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
        let a = self.to_type_order();
        let b = other.to_type_order();
        a == b
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

impl IntoCbor for Key {
    fn into_cbor(self: Key) -> Result<Cbor> {
        let val = match self {
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

impl FromCbor for Key {
    fn from_cbor(val: Cbor) -> Result<Key> {
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
            _ => err_at!(FailCbor, msg: "cbor not a valid key")?,
        };

        Ok(key)
    }
}

impl<T, const N: usize> IntoCbor for [T; N]
where
    T: Clone + IntoCbor,
{
    fn into_cbor(self) -> Result<Cbor> {
        let info = err_at!(FailConvert, u64::try_from(self.len()))?.into();
        let mut val: Vec<Cbor> = vec![];
        for item in self.iter() {
            val.push(item.clone().into_cbor()?)
        }
        Ok(Cbor::Major4(info, val))
    }
}

impl<T, const N: usize> FromCbor for [T; N]
where
    T: Copy + Default + FromCbor,
{
    fn from_cbor(val: Cbor) -> Result<[T; N]> {
        let mut arr = [T::default(); N];
        let n = arr.len();
        match val {
            Cbor::Major4(_, data) if n == data.len() => {
                for (i, item) in data.into_iter().enumerate() {
                    arr[i] = T::from_cbor(item)?;
                }
                Ok(arr)
            }
            Cbor::Major4(_, data) => {
                err_at!(FailConvert, msg: "different array arity {} {}", n, data.len())
            }
            _ => err_at!(FailCbor, msg: "not an list"),
        }
    }
}

impl IntoCbor for bool {
    fn into_cbor(self) -> Result<Cbor> {
        match self {
            true => SimpleValue::True.try_into(),
            false => SimpleValue::False.try_into(),
        }
    }
}

impl FromCbor for bool {
    fn from_cbor(val: Cbor) -> Result<bool> {
        match val {
            Cbor::Major7(_, SimpleValue::True) => Ok(true),
            Cbor::Major7(_, SimpleValue::False) => Ok(false),
            _ => err_at!(FailConvert, msg: "not a bool"),
        }
    }
}

impl IntoCbor for f32 {
    fn into_cbor(self) -> Result<Cbor> {
        SimpleValue::F32(self).try_into()
    }
}

impl FromCbor for f32 {
    fn from_cbor(val: Cbor) -> Result<f32> {
        match val {
            Cbor::Major7(_, SimpleValue::F32(val)) => Ok(val),
            _ => err_at!(FailConvert, msg: "not f32"),
        }
    }
}

impl IntoCbor for f64 {
    fn into_cbor(self) -> Result<Cbor> {
        SimpleValue::F64(self).try_into()
    }
}

impl FromCbor for f64 {
    fn from_cbor(val: Cbor) -> Result<f64> {
        match val {
            Cbor::Major7(_, SimpleValue::F64(val)) => Ok(val),
            _ => err_at!(FailConvert, msg: "not f64"),
        }
    }
}

macro_rules! convert_neg_num {
    ($($t:ty)*) => {$(
        impl FromCbor for $t {
            fn from_cbor(val: Cbor) -> Result<$t> {
                use std::result;

                let val = match val {
                    Cbor::Major0(_, val) => {
                        let val: result::Result<$t, _> = val.try_into();
                        err_at!(FailConvert, val)?
                    }
                    Cbor::Major1(_, val) => {
                        let val: result::Result<$t, _> = (val + 1).try_into();
                        -err_at!(FailConvert, val)?
                    }
                    _ => err_at!(FailConvert, msg: "not a number")?,
                };
                Ok(val)
            }
        }

        impl IntoCbor for $t {
            fn into_cbor(self) -> Result<Cbor> {
                let val: i64 = self.into();
                if val >= 0 {
                    Ok(err_at!(FailConvert, u64::try_from(val))?.into_cbor()?)
                } else {
                    let val = err_at!(FailConvert, u64::try_from(val.abs() - 1))?;
                    let info = val.into();
                    Ok(Cbor::Major1(info, val))
                }
            }
        }
    )*}
}

convert_neg_num! {i64 i32 i16 i8}

macro_rules! convert_pos_num {
    ($($t:ty)*) => {$(
        impl FromCbor for $t {
            fn from_cbor(val: Cbor) -> Result<$t> {
                match val {
                    Cbor::Major0(_, val) => Ok(err_at!(FailConvert, val.try_into())?),
                    _ => err_at!(FailConvert, msg: "not a number"),
                }
            }
        }

        impl IntoCbor for $t {
            fn into_cbor(self) -> Result<Cbor> {
                let val = u64::from(self);
                Ok(Cbor::Major0(val.into(), val))
            }
        }
    )*}
}

convert_pos_num! {u64 u32 u16 u8}

impl IntoCbor for usize {
    fn into_cbor(self) -> Result<Cbor> {
        let val = err_at!(FailConvert, u64::try_from(self))?;
        Ok(val.into_cbor()?)
    }
}

impl FromCbor for usize {
    fn from_cbor(val: Cbor) -> Result<usize> {
        match val {
            Cbor::Major0(_, val) => err_at!(FailConvert, usize::try_from(val)),
            _ => err_at!(FailConvert, msg: "not a number"),
        }
    }
}

impl IntoCbor for isize {
    fn into_cbor(self) -> Result<Cbor> {
        err_at!(FailConvert, i64::try_from(self))?.into_cbor()
    }
}

impl FromCbor for isize {
    fn from_cbor(val: Cbor) -> Result<isize> {
        let val = match val {
            Cbor::Major0(_, val) => err_at!(FailConvert, isize::try_from(val))?,
            Cbor::Major1(_, val) => -err_at!(FailConvert, isize::try_from(val + 1))?,
            _ => err_at!(FailConvert, msg: "not a number")?,
        };
        Ok(val)
    }
}

impl<'a> IntoCbor for &'a [u8] {
    fn into_cbor(self) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(self.len()))?;
        Ok(Cbor::Major2(n.into(), self.to_vec()))
    }
}

impl<T> IntoCbor for Vec<T>
where
    T: IntoCbor,
{
    fn into_cbor(self) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(self.len()))?;
        let mut arr = vec![];
        for item in self.into_iter() {
            arr.push(item.into_cbor()?)
        }
        Ok(Cbor::Major4(n.into(), arr))
    }
}

impl<T> FromCbor for Vec<T>
where
    T: FromCbor + Sized,
{
    fn from_cbor(val: Cbor) -> Result<Vec<T>> {
        match val {
            Cbor::Major4(_, data) => {
                let mut arr = vec![];
                for item in data.into_iter() {
                    arr.push(T::from_cbor(item)?)
                }
                Ok(arr)
            }
            _ => err_at!(FailConvert, msg: "not a vector"),
        }
    }
}

impl<'a> IntoCbor for &'a str {
    fn into_cbor(self) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(self.len()))?;
        Ok(Cbor::Major3(n.into(), self.as_bytes().to_vec()))
    }
}

impl IntoCbor for String {
    fn into_cbor(self) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(self.len()))?;
        Ok(Cbor::Major3(n.into(), self.as_bytes().to_vec()))
    }
}

impl FromCbor for String {
    fn from_cbor(val: Cbor) -> Result<String> {
        use std::str::from_utf8;
        match val {
            Cbor::Major3(_, val) => Ok(err_at!(FailConvert, from_utf8(&val))?.to_string()),
            _ => err_at!(FailConvert, msg: "not utf8-string"),
        }
    }
}

impl IntoCbor for Vec<Cbor> {
    fn into_cbor(self) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(self.len()))?;
        Ok(Cbor::Major4(n.into(), self))
    }
}

impl FromCbor for Vec<Cbor> {
    fn from_cbor(val: Cbor) -> Result<Vec<Cbor>> {
        match val {
            Cbor::Major4(_, data) => Ok(data),
            _ => err_at!(FailConvert, msg: "not a vector"),
        }
    }
}

impl IntoCbor for Vec<(Key, Cbor)> {
    fn into_cbor(self) -> Result<Cbor> {
        let n = err_at!(FailConvert, u64::try_from(self.len()))?;
        Ok(Cbor::Major5(n.into(), self))
    }
}

impl FromCbor for Vec<(Key, Cbor)> {
    fn from_cbor(val: Cbor) -> Result<Vec<(Key, Cbor)>> {
        match val {
            Cbor::Major5(_, data) => Ok(data),
            _ => err_at!(FailConvert, msg: "not a map"),
        }
    }
}

impl<T> IntoCbor for Option<T>
where
    T: IntoCbor,
{
    fn into_cbor(self) -> Result<Cbor> {
        match self {
            Some(val) => val.into_cbor(),
            None => SimpleValue::Null.try_into(),
        }
    }
}

impl<T> FromCbor for Option<T>
where
    T: FromCbor + Sized,
{
    fn from_cbor(val: Cbor) -> Result<Option<T>> {
        match val {
            Cbor::Major7(_, SimpleValue::Null) => Ok(None),
            val => Ok(Some(T::from_cbor(val)?)),
        }
    }
}
