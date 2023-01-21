#![feature(maybe_uninit_slice, round_char_boundary)]
#![cfg_attr(test, feature(test))]

#[cfg(test)]
extern crate test;

use std::{ffi::c_void, mem::MaybeUninit, ptr};
use windows_sys::Win32::{Foundation::*, Globalization::*, System::Console::*};

pub fn write_u16s(h: HANDLE, s: &[u16]) -> Option<&[u16]> {
    let mut written = 0;
    let res = unsafe {
        WriteConsoleW(
            h,
            s.as_ptr() as *const c_void,
            s.len() as u32,
            &mut written,
            ptr::null_mut(),
        )
    };
    if res == 0 {
        None
    } else {
        Some(&s[written as usize..])
    }
}

pub fn write_all_u16s(h: HANDLE, mut s: &[u16]) {
    while !s.is_empty() {
        s = write_u16s(h, s).unwrap();
    }
}

pub struct Utf8ToUtf16Result<'input, 'output> {
    pub remaining: &'input str,
    pub output: &'output [u16],
}

pub trait Utf8ToUtf16 {
    fn cvt<'a, 'b>(s: &'a str, buf: &'b mut [MaybeUninit<u16>]) -> Utf8ToUtf16Result<'a, 'b>;
}

pub struct Rust;
pub struct Windows;

impl Utf8ToUtf16 for Rust {
    fn cvt<'a, 'b>(s: &'a str, buf: &'b mut [MaybeUninit<u16>]) -> Utf8ToUtf16Result<'a, 'b> {
        let mut len_utf16 = 0;
        for (chr, dest) in s.encode_utf16().zip(buf.iter_mut()) {
            *dest = MaybeUninit::new(chr);
            len_utf16 += 1;
        }
        // Safety: We've initialized `len_utf16` values.
        let utf16: &mut [u16] = unsafe { MaybeUninit::slice_assume_init_mut(&mut buf[..len_utf16]) };

        let mut count = 0;
        for ch in utf16.iter() {
            count += match ch {
                0x0000..=0x007F => 1,
                0x0080..=0x07FF => 2,
                0xDCEE..=0xDFFF => 1, // Low surrogate. We already counted 3 bytes for the other.
                _ => 3,
            };
        }

        Utf8ToUtf16Result {
            remaining: &s[count..],
            output: utf16,
        }
    }
}
impl Utf8ToUtf16 for Windows {
    fn cvt<'a, 'b>(s: &'a str, buf: &'b mut [MaybeUninit<u16>]) -> Utf8ToUtf16Result<'a, 'b> {
        let utf8 = &s[..s.floor_char_boundary(buf.len())];
        if utf8.is_empty() {
            return Utf8ToUtf16Result {
                remaining: s,
                output: unsafe { MaybeUninit::slice_assume_init_ref(&mut buf[..0]) },
            };
        }

        let utf16: &mut [u16] = unsafe {
            let result = MultiByteToWideChar(
                CP_UTF8,                      // CodePage
                MB_ERR_INVALID_CHARS,         // dwFlags
                utf8.as_ptr(),                // lpMultiByteStr
                utf8.len() as i32,            // cbMultiByte
                buf.as_mut_ptr() as *mut u16, // lpWideCharStr
                buf.len() as i32,             // cchWideChar
            );
            assert!(result != 0, "Unexpected error in MultiByteToWideChar");

            // Safety: MultiByteToWideChar initializes `result` values.
            MaybeUninit::slice_assume_init_mut(&mut buf[..result as usize])
        };

        Utf8ToUtf16Result {
            remaining: &s[utf8.len()..],
            output: utf16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;

    fn convert_all<T: Utf8ToUtf16>(s: &str) -> Vec<u16> {
        let mut buf = [MaybeUninit::<u16>::uninit(); 4096];
        let mut cvt_result = T::cvt(s, &mut buf);

        let mut result = Vec::new();
        while {
            result.extend(cvt_result.output);
            !cvt_result.remaining.is_empty()
        } {
            cvt_result = T::cvt(cvt_result.remaining, &mut buf);
        }

        result
    }

    const CHINESE_MESSAGE: &'static str = include_str!("../lorem-chinese.txt");
    const ASCII_MESSAGE: &'static str = include_str!("../lorem-ascii.txt");

    /*
    #[test]
    fn chinese_windows_works() {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        write_all_u16s(handle, &convert_all::<Windows>(CHINESE_MESSAGE))
    }
    #[test]
    fn ascii_windows_works() {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        write_all_u16s(handle, &convert_all::<Windows>(ASCII_MESSAGE))
    }
    #[test]
    fn chinese_rust_works() {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        write_all_u16s(handle, &convert_all::<Rust>(CHINESE_MESSAGE))
    }
    #[test]
    fn ascii_rust_works() {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        write_all_u16s(handle, &convert_all::<Rust>(ASCII_MESSAGE))
    }
    */

    #[test]
    fn chinese_windows_rust_equal() {
        assert!(convert_all::<Windows>(CHINESE_MESSAGE) == convert_all::<Rust>(CHINESE_MESSAGE))
    }
    #[test]
    fn ascii_windows_rust_equal() {
        assert!(convert_all::<Windows>(ASCII_MESSAGE) == convert_all::<Rust>(ASCII_MESSAGE))
    }

    #[bench]
    fn bench_chinese_windows(b: &mut Bencher) {
        b.iter(|| {
            convert_all::<Windows>(CHINESE_MESSAGE)
        })
    }
    #[bench]
    fn bench_ascii_windows(b: &mut Bencher) {
        b.iter(|| {
            convert_all::<Windows>(ASCII_MESSAGE)
        })
    }
    #[bench]
    fn bench_chinese_rust(b: &mut Bencher) {
        b.iter(|| {
            convert_all::<Rust>(CHINESE_MESSAGE)
        })
    }
    #[bench]
    fn bench_ascii_rust(b: &mut Bencher) {
        b.iter(|| {
            convert_all::<Rust>(ASCII_MESSAGE)
        })
    }
}
