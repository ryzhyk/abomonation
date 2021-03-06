//! Abomonation (spelling intentional) is a fast serialization / deserialization crate.
//!
//! Abomonation takes typed elements and simply writes their contents as binary.
//! It then gives the element the opportunity to serialize more data, which is
//! useful for types with owned memory such as `String` and `Vec`.
//! The result is effectively a copy of reachable memory, where pointers are zero-ed out and vector
//! capacities are set to the vector length.
//! Deserialization results in a shared reference to the type, pointing at the binary data itself.
//!
//! Abomonation does several unsafe things, and should ideally be used only through the methods
//! `encode` and `decode` on types implementing the `Abomonation` trait. Implementing the
//! `Abomonation` trait is highly discouraged, unless you use the `unsafe_abomonate!` macro, which
//! is only mostly discouraged.
//!
//! **Very important**: Abomonation reproduces the memory as laid out by the serializer, which can
//! reveal architectural variations. Data encoded on a 32bit big-endian machine will not decode
//! properly on a 64bit little-endian machine. Moreover, it could result in undefined behavior if
//! the deserialization results in invalid typed data. Please do not do this.
//!
//!
//! #Examples
//! ```
//! use abomonation::{encode, decode};
//!
//! // create some test data out of abomonation-approved types
//! let vector = (0..256u64).map(|i| (i, format!("{}", i)))
//!                         .collect::<Vec<_>>();
//!
//! // encode a Vec<(u64, String)> into a Vec<u8>
//! let mut bytes = Vec::new();
//! unsafe { encode(&vector, &mut bytes); }
//!
//! // decode a &Vec<(u64, String)> from &mut [u8] binary data
//! if let Some((result, remaining)) = unsafe { decode::<Vec<(u64, String)>>(&mut bytes) } {
//!     assert!(result == &vector);
//!     assert!(remaining.len() == 0);
//! }
//! ```

use std::mem;       // yup, used pretty much everywhere.
use std::io::Write; // for bytes.write_all; push_all is unstable and extend is slow.
use std::marker::PhantomData;

const EMPTY: *mut () = 0x1 as *mut ();

macro_rules! try_option {
    ($expr:expr) => (match $expr {
        Some(val) => val,
        None => { return None }
    })
}

/// Encodes a typed reference into a binary buffer.
///
/// `encode` will transmute `typed` to binary and write its contents to `bytes`. It then offers the
/// element the opportunity to serialize more data. Having done that,
/// it offers the element the opportunity to "tidy up", in which the element can erasing things
/// like local memory addresses that it would be impolite to share.
///
/// #Examples
/// ```
/// use abomonation::{encode, decode};
///
/// // create some test data out of abomonation-approved types
/// let vector = (0..256u64).map(|i| (i, format!("{}", i)))
///                         .collect::<Vec<_>>();
///
/// // encode a Vec<(u64, String)> into a Vec<u8>
/// let mut bytes = Vec::new();
/// unsafe { encode(&vector, &mut bytes); }
///
/// // decode a &Vec<(u64, String)> from &mut [u8] binary data
/// if let Some((result, remaining)) = unsafe { decode::<Vec<(u64, String)>>(&mut bytes) } {
///     assert!(result == &vector);
///     assert!(remaining.len() == 0);
/// }
/// ```
///
#[inline]
pub unsafe fn encode<T: Abomonation>(typed: &T, bytes: &mut Vec<u8>) {
    let start = bytes.len();            // may not be empty!
    let slice = std::slice::from_raw_parts(mem::transmute(typed), mem::size_of::<T>());

    // TODO : At the moment, this does not compile to a memcpy, 
    // due to Rust working around LLVM soundness bugs.

    // bytes.write_all(slice).unwrap();    // Rust claims a write to a Vec<u8> will never fail.

    // TODO : Instead, we use the following hunk of code:
    let position = bytes.len();
    bytes.reserve(slice.len());
    ::std::ptr::copy_nonoverlapping(slice.as_ptr(), bytes.as_mut_ptr().offset(position as isize), slice.len());
    bytes.set_len(position + slice.len());
    // TODO : End hunk of replacement code

    let result: &mut T = mem::transmute(bytes.as_mut_ptr().offset(start as isize));
    result.embalm();
    typed.entomb(bytes);
}

/// Decodes a mutable binary slice into an immutable typed reference.
///
/// `decode` treats the first `mem::size_of::<T>()` bytes as a `T`, and will then `exhume` the
/// element, offering it the ability to consume prefixes of `bytes` to back any owned data.
/// The return value is either a pair of the typed reference `&T` and the remaining `&mut [u8]`
/// binary data, or `None` if decoding failed due to lack of data.
///
/// #Safety
///
/// `decode` is unsafe due to a number of unchecked invariants. Decoding arbitrary `&[u8]` data can
/// result in invalid utf8 strings, enums with invalid discriminants, etc. `decode` does presently
/// perform bounds checks, as part of determining if enough data are present to completely decode,
/// and while it should only write to its `&mut [u8]` argument, invalid utf8 and enums are undefined
/// behavior. Please do not decode data that was not encoded by the corresponding implementation.
///
/// #Examples
/// ```
/// use abomonation::{encode, decode};
///
/// // create some test data out of abomonation-approved types
/// let vector = (0..256u64).map(|i| (i, format!("{}", i)))
///                         .collect::<Vec<_>>();
///
/// // encode a Vec<(u64, String)> into a Vec<u8>
/// let mut bytes = Vec::new();
/// unsafe { encode(&vector, &mut bytes); }
///
/// // decode a &Vec<(u64, String)> from &mut [u8] binary data
/// if let Some((result, remaining)) = unsafe { decode::<Vec<(u64, String)>>(&mut bytes) } {
///     assert!(result == &vector);
///     assert!(remaining.len() == 0);
/// }
/// ```
#[inline]
pub unsafe fn decode<T: Abomonation>(bytes: &mut [u8]) -> Option<(&T, &mut [u8])> {
    if bytes.len() < mem::size_of::<T>() { None }
    else {
        let (split1, split2) = bytes.split_at_mut(mem::size_of::<T>());
        let result: &mut T = mem::transmute(split1.get_unchecked_mut(0));
        if let Some(remaining) = result.exhume(split2) {
            Some((result, remaining))
        }
        else {
            None
        }
    }
}

/// Abomonation provides methods to serialize any heap data the implementor owns.
///
/// The default implementations for Abomonation's methods are all empty. Many types have no owned
/// data to transcribe. Some do, however, and need to carefully implement these unsafe methods.
///
/// #Safety
///
/// Abomonation has no safe methods. Please do not call them. They should be called only by
/// `encode` and `decode`, each of which impose restrictions on ownership and lifetime of the data
/// they take as input and return as output.
///
/// If you are concerned about safety, it may be best to avoid Abomonation all together. It does
/// several things that may be undefined behavior, depending on how undefined behavior is defined.
pub trait Abomonation {

    /// Write any additional information about `&self` beyond its binary representation.
    ///
    /// Most commonly this is owned data on the other end of pointers in `&self`.
    #[inline(always)] unsafe fn entomb(&self, _writer: &mut Vec<u8>) { }

    /// Perform any final edits before committing `&mut self`. Importantly, this method should only
    /// manipulate the fields of `self`; any owned memory may not be valid.
    ///
    /// Most commonly this overwrites pointers whose values should not be serialized.
    #[inline(always)] unsafe fn embalm(&mut self) { }

    /// Recover any information for `&mut self` not evident from its binary representation.
    ///
    /// Most commonly this populates pointers with valid references into `bytes`.
    #[inline(always)] unsafe fn exhume<'a,'b>(&'a mut self, bytes: &'b mut [u8]) -> Option<&'b mut [u8]> { Some(bytes) }
}

/// The `unsafe_abomonate!` macro takes a type name with an optional list of fields, and implements
/// `Abomonation` for the type, following the pattern of the tuple implementations: each method
/// calls the equivalent method on each of its fields.
///
/// #Safety
/// `unsafe_abomonate` is unsafe because if you fail to specify a field it will not be properly
/// re-initialized from binary data. This can leave you with a dangling pointer, or worse.
///
/// #Examples
/// ```
/// #[macro_use]
/// extern crate abomonation;
/// use abomonation::{encode, decode, Abomonation};
///
/// #[derive(Eq, PartialEq)]
/// struct MyStruct {
///     a: String,
///     b: u64,
///     c: Vec<u8>,
/// }
///
/// unsafe_abomonate!(MyStruct : a, b, c);
///
/// fn main() {
///
///     // create some test data out of recently-abomonable types
///     let my_struct = MyStruct { a: "grawwwwrr".to_owned(), b: 0, c: vec![1,2,3] };
///
///     // encode a &MyStruct into a Vec<u8>
///     let mut bytes = Vec::new();
///     unsafe { encode(&my_struct, &mut bytes); }
///
///     // decode a &MyStruct from &mut [u8] binary data
///     if let Some((result, remaining)) = unsafe { decode::<MyStruct>(&mut bytes) } {
///         assert!(result == &my_struct);
///         assert!(remaining.len() == 0);
///     }
/// }
/// ```
#[macro_export]
macro_rules! unsafe_abomonate {
    ($t:ty) => { impl Abomonation for $t { } };
    ($t:ty : $($field:ident),*) => {
        impl Abomonation for $t {
            #[inline] unsafe fn entomb(&self, _writer: &mut Vec<u8>) {
                $( self.$field.entomb(_writer); )*
            }
            #[inline] unsafe fn embalm(&mut self) {
                $( self.$field.embalm(); )*
            }
            #[inline] unsafe fn exhume<'a,'b>(&'a mut self, mut bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
                $( let temp = bytes; bytes = if let Some(bytes) = self.$field.exhume(temp) { bytes} else { return None }; )*
                Some(bytes)
            }
        }
    };
    // // TODO: This doesn't seem to work. Not obvious how to fix with constraints on generic parameters.
    // ($($name:ident),+ ; $t: ty : $($field:ident),*) => {
    //     impl<$($name: Abomonation),*> Abomonation for $t {
    //         #[inline] unsafe fn entomb(&self, _writer: &mut Vec<u8>) {
    //             $( self.$field.entomb(_writer); )*
    //         }
    //         #[inline] unsafe fn embalm(&mut self) {
    //             $( self.$field.embalm(); )*
    //         }
    //         #[inline] unsafe fn exhume<'a,'b>(&'a mut self, mut bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
    //             $( let temp = bytes; bytes = if let Some(bytes) = self.$field.exhume(temp) { bytes} else { return None }; )*
    //             Some(bytes)
    //         }
    //     }
    // };
}

// general code for tuples (can't use '0', '1', ... as field identifiers)
macro_rules! tuple_abomonate {
    ( $($name:ident)+) => (
        impl<$($name: Abomonation),*> Abomonation for ($($name,)*) {
            #[allow(non_snake_case)]
            #[inline] unsafe fn entomb(&self, _writer: &mut Vec<u8>) {
                let ($(ref $name,)*) = *self;
                $($name.entomb(_writer);)*
            }
            #[allow(non_snake_case)]
            #[inline] unsafe fn embalm(&mut self) {
                let ($(ref mut $name,)*) = *self;
                $($name.embalm();)*
            }
            #[allow(non_snake_case)]
            #[inline] unsafe fn exhume<'a,'b>(&'a mut self, mut bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
                let ($(ref mut $name,)*) = *self;
                $( let temp = bytes; bytes = if let Some(bytes) = $name.exhume(temp) { bytes} else { return None }; )*
                Some(bytes)
            }
        }
    );
}

impl Abomonation for u8 { }
impl Abomonation for u16 { }
impl Abomonation for u32 { }
impl Abomonation for u64 { }
impl Abomonation for usize { }

impl Abomonation for i8 { }
impl Abomonation for i16 { }
impl Abomonation for i32 { }
impl Abomonation for i64 { }
impl Abomonation for isize { }

impl Abomonation for f32 { }
impl Abomonation for f64 { }

impl Abomonation for bool { }
impl Abomonation for () { }

impl Abomonation for char { }

impl<T> Abomonation for PhantomData<T> {}

impl<T: Abomonation> Abomonation for Option<T> {
    #[inline] unsafe fn embalm(&mut self) {
        if let &mut Some(ref mut inner) = self {
            inner.embalm();
        }
    }
    #[inline] unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
        if let &Some(ref inner) = self {
            inner.entomb(bytes);
        }
    }
    #[inline] unsafe fn exhume<'a, 'b>(&'a mut self, mut bytes: &'b mut[u8]) -> Option<&'b mut [u8]> {
        if let &mut Some(ref mut inner) = self {
            let tmp = bytes; bytes = try_option!(inner.exhume(tmp));
        }
        Some(bytes)
    }
}

impl<T: Abomonation, E: Abomonation> Abomonation for Result<T, E> {
    #[inline] unsafe fn embalm(&mut self) {
        match self {
            &mut Ok(ref mut inner) => inner.embalm(),
            &mut Err(ref mut inner) => inner.embalm(),
        }
    }
    #[inline] unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
        match self {
            &Ok(ref inner) => inner.entomb(bytes),
            &Err(ref inner) => inner.entomb(bytes),
        }
    }
    #[inline] unsafe fn exhume<'a, 'b>(&'a mut self, mut bytes: &'b mut[u8]) -> Option<&'b mut [u8]> {
        match self {
            &mut Ok(ref mut inner) => inner.exhume(bytes),
            &mut Err(ref mut inner) => inner.exhume(bytes),
        }
    }
}

tuple_abomonate!(A);
tuple_abomonate!(A B);
tuple_abomonate!(A B C);
tuple_abomonate!(A B C D);
tuple_abomonate!(A B C D E);
tuple_abomonate!(A B C D E F);
tuple_abomonate!(A B C D E F G);
tuple_abomonate!(A B C D E F G H);
tuple_abomonate!(A B C D E F G H I);
tuple_abomonate!(A B C D E F G H I J);

macro_rules! array_abomonate {
    ($size:expr) => (
        impl<T: Abomonation> Abomonation for [T; $size] {
            #[inline] 
            unsafe fn embalm(&mut self) {
                for element in self { element.embalm(); }
            }
            #[inline]
            unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
                for element in self { element.entomb(bytes); }
            }
            #[inline]
            unsafe fn exhume<'a, 'b>(&'a mut self, mut bytes: &'b mut[u8]) -> Option<&'b mut [u8]> {
                for element in self { 
                    let tmp = bytes; bytes = try_option!(element.exhume(tmp));
                }
                Some(bytes)
            }
        }
    )
}

array_abomonate!(0);
array_abomonate!(1);
array_abomonate!(2);
array_abomonate!(3);
array_abomonate!(4);
array_abomonate!(5);
array_abomonate!(6);
array_abomonate!(7);
array_abomonate!(8);
array_abomonate!(9);
array_abomonate!(10);
array_abomonate!(11);
array_abomonate!(12);
array_abomonate!(13);
array_abomonate!(14);
array_abomonate!(15);
array_abomonate!(16);
array_abomonate!(17);
array_abomonate!(18);
array_abomonate!(19);
array_abomonate!(20);
array_abomonate!(21);
array_abomonate!(22);
array_abomonate!(23);
array_abomonate!(24);
array_abomonate!(25);
array_abomonate!(26);
array_abomonate!(27);
array_abomonate!(28);
array_abomonate!(29);
array_abomonate!(30);
array_abomonate!(31);
array_abomonate!(32);

impl Abomonation for String {
    #[inline]
    unsafe fn embalm(&mut self) {
        std::ptr::write(self, String::from_raw_parts(EMPTY as *mut u8, self.len(), self.len()));
    }
    #[inline]
    unsafe fn entomb(&self, bytes: &mut Vec<u8>) {

        // TODO : At the moment, this does not compile to a memcpy, 
        // due to Rust working around LLVM soundness bugs.

        // bytes.write_all(self.as_bytes()).unwrap();

        // TODO : Instead, we use the following hunk of code:
        let position = bytes.len();
        bytes.reserve(self.as_bytes().len());
        ::std::ptr::copy_nonoverlapping(self.as_bytes().as_ptr(), bytes.as_mut_ptr().offset(position as isize), self.as_bytes().len());
        bytes.set_len(position + self.as_bytes().len());
        // TODO : End hunk of replacement code

    }
    #[inline]
    unsafe fn exhume<'a,'b>(&'a mut self, bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
        if self.len() > bytes.len() { None }
        else {
            let (mine, rest) = bytes.split_at_mut(self.len());
            std::ptr::write(self, String::from_raw_parts(mem::transmute(mine.as_ptr()), self.len(), self.len()));
            Some(rest)
        }
    }
}

// TODO: Code deactivated because 'c unbound; would not be safe for e.g. 'static.
//
// impl<'c> Abomonation for &'c str {
//     #[inline]
//     unsafe fn embalm(&mut self) {
//         *self = std::str::from_utf8_unchecked(std::slice::from_raw_parts(EMPTY as *mut u8, self.len()));
//     }
//     #[inline]
//     unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
//
//         // TODO : At the moment, this does not compile to a memcpy, 
//         // due to Rust working around LLVM soundness bugs.
//
//         // bytes.write_all(self.as_bytes()).unwrap();
//
//         // TODO : Instead, we use the following hunk of code:
//         let position = bytes.len();
//         bytes.reserve(self.as_bytes().len());
//         ::std::ptr::copy_nonoverlapping(self.as_bytes().as_ptr(), bytes.as_mut_ptr().offset(position as isize), self.as_bytes().len());
//         bytes.set_len(position + self.as_bytes().len());
//         // TODO : End hunk of replacement code
//
//     }
//     #[inline]
//     unsafe fn exhume<'a,'b>(&'a mut self, bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
//         if self.len() > bytes.len() { None }
//         else {
//             let (mine, mut rest) = bytes.split_at_mut(self.len());
//             *self = std::str::from_utf8_unchecked(std::slice::from_raw_parts_mut(mine.as_mut_ptr() as *mut u8, self.len()));
//             Some(rest)
//         }
//     }
// }

impl<T: Abomonation> Abomonation for Vec<T> {
    #[inline]
    unsafe fn embalm(&mut self) {
        std::ptr::write(self, Vec::from_raw_parts(EMPTY as *mut T, self.len(), self.len()));
    }
    #[inline]
    unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
        let position = bytes.len();

        // TODO : At the moment, this does not compile to a memcpy, 
        // due to Rust working around LLVM soundness bugs.

        // bytes.write_all(typed_to_bytes(&self[..])).unwrap();

        // TODO : Instead, we use the following hunk of code:
        let typed_bytes = typed_to_bytes(&self[..]);
        bytes.reserve(typed_bytes.len());
        ::std::ptr::copy_nonoverlapping(typed_bytes.as_ptr(), bytes.as_mut_ptr().offset(position as isize), typed_bytes.len());
        bytes.set_len(position + typed_bytes.len());
        // TODO : End hunk of replacement code

        for element in bytes_to_typed::<T>(&mut bytes[position..], self.len()) { element.embalm(); }
        for element in self.iter() { element.entomb(bytes); }
    }
    #[inline]
    unsafe fn exhume<'a,'b>(&'a mut self, bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {

        // extract memory from bytes to back our vector
        let binary_len = self.len() * mem::size_of::<T>();
        if binary_len > bytes.len() { None }
        else {
            let (mine, mut rest) = bytes.split_at_mut(binary_len);
            let slice = std::slice::from_raw_parts_mut(mine.as_mut_ptr() as *mut T, self.len());
            std::ptr::write(self, Vec::from_raw_parts(slice.as_mut_ptr(), self.len(), self.len()));
            for element in self.iter_mut() {
                let temp = rest;             // temp variable explains lifetimes (mysterious!)
                rest = try_option!(element.exhume(temp));
            }
            Some(rest)
        }
    }
}

// TODO: Code deactivated because 'c unbound; would not be safe for e.g. 'static.
//
// impl<'c, T: Abomonation> Abomonation for &'c [T] {
//     #[inline]
//     unsafe fn embalm(&mut self) {
//         std::ptr::write(self, std::slice::from_raw_parts(EMPTY as *mut T, self.len()));
//     }
//     #[inline]
//     unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
//         let position = bytes.len();
//
//         // TODO : At the moment, this does not compile to a memcpy, 
//         // due to Rust working around LLVM soundness bugs.
//
//         // bytes.write_all(typed_to_bytes(self)).unwrap();
//
//         // TODO : Instead, we use the following hunk of code:
//         let typed_bytes = typed_to_bytes(&self[..]);
//         bytes.reserve(typed_bytes.len());
//         ::std::ptr::copy_nonoverlapping(typed_bytes.as_ptr(), bytes.as_mut_ptr().offset(position as isize), typed_bytes.len());
//         bytes.set_len(position + typed_bytes.len());
//         // TODO : End hunk of replacement code
//
//         for element in bytes_to_typed::<T>(&mut bytes[position..], self.len()) { element.embalm(); }
//         for element in self.iter() { element.entomb(bytes); }
//     }
//     #[inline]
//     unsafe fn exhume<'a,'b>(&'a mut self, bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
//
//         // extract memory from bytes to back our slice
//         let binary_len = self.len() * mem::size_of::<T>();
//         if binary_len > bytes.len() { None }
//         else {
//             let (mine, mut rest) = bytes.split_at_mut(binary_len);
//             let slice = std::slice::from_raw_parts_mut(mine.as_mut_ptr() as *mut T, self.len());
//             for element in slice.iter_mut() {
//                 let temp = rest;
//                 rest = try_option!(element.exhume(temp));
//             }
//             *self = slice;
//             Some(rest)
//         }
//     }
// }

impl<T: Abomonation> Abomonation for Box<T> {
    #[inline]
    unsafe fn embalm(&mut self) {
        std::ptr::write(self, mem::transmute(EMPTY as *mut T));
    }
    #[inline]
    unsafe fn entomb(&self, bytes: &mut Vec<u8>) {
        let position = bytes.len();
        bytes.write_all(std::slice::from_raw_parts(mem::transmute(&**self), mem::size_of::<T>())).unwrap();
        bytes_to_typed::<T>(&mut bytes[position..], 1)[0].embalm();
        (**self).entomb(bytes);
    }
    #[inline]
    unsafe fn exhume<'a,'b>(&'a mut self, bytes: &'b mut [u8]) -> Option<&'b mut [u8]> {
        let binary_len = mem::size_of::<T>();
        if binary_len > bytes.len() { None }
        else {
            let (mine, mut rest) = bytes.split_at_mut(binary_len);
            std::ptr::write(self, mem::transmute(mine.as_mut_ptr() as *mut T));
            let temp = rest; rest = try_option!((**self).exhume(temp));
            Some(rest)
        }
    }
}

// currently enables UB, by exposing padding bytes
#[inline] unsafe fn typed_to_bytes<T>(slice: &[T]) -> &[u8] {
    std::slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * mem::size_of::<T>())
}

// takes a len to make working with zero-size types easier
#[inline] unsafe fn bytes_to_typed<T>(slice: &mut [u8], len: usize) -> &mut [T] {
    std::slice::from_raw_parts_mut(slice.as_mut_ptr() as *mut T, len)
}
