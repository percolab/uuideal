#![allow(non_snake_case)]

#[cfg(not(Py_3_13))]
use core::ffi::c_uchar;
use core::ffi::{c_char, c_int, c_long, c_ulonglong, c_void};
use pyo3_ffi::*;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

macro_rules! cstr {
    ($value:literal) => {
        concat!($value, "\0").as_ptr().cast::<c_char>()
    };
}

const CAPSULE_NAME: *const c_char = c"uuideal._original_vectorcall".as_ptr();
#[cfg(Py_3_13)]
const PY_ASNATIVEBYTES_BIG_ENDIAN: c_int = 0;
#[cfg(Py_3_13)]
const PY_ASNATIVEBYTES_REJECT_NEGATIVE: c_int = 8;

static PATCHED: AtomicBool = AtomicBool::new(false);

static mut UUID_MODULE: *mut PyObject = ptr::null_mut();
static mut UUID_TYPE: *mut PyObject = ptr::null_mut();
static mut SAFE_UUID_TYPE: *mut PyObject = ptr::null_mut();
static mut SAFE_UUID_UNKNOWN: *mut PyObject = ptr::null_mut();
static mut RESERVED_NCS_VALUE: *mut PyObject = ptr::null_mut();
static mut RFC_4122_VALUE: *mut PyObject = ptr::null_mut();
static mut RESERVED_MICROSOFT_VALUE: *mut PyObject = ptr::null_mut();
static mut RESERVED_FUTURE_VALUE: *mut PyObject = ptr::null_mut();
static mut INT_SLOT_OFFSET: Py_ssize_t = -1;
static mut IS_SAFE_SLOT_OFFSET: Py_ssize_t = -1;
static mut MAX_UUID_VERSION: u8 = 5;
static mut INTERNED_INT: *mut PyObject = ptr::null_mut();
static mut INTERNED_IS_SAFE: *mut PyObject = ptr::null_mut();
static mut INTERNED_VALUE: *mut PyObject = ptr::null_mut();

#[repr(C)]
struct PyDescrObjectLayout {
    ob_base: PyObject,
    d_type: *mut PyTypeObject,
    d_name: *mut PyObject,
    d_qualname: *mut PyObject,
}

#[repr(C)]
struct PyMemberDefLayout {
    name: *const c_char,
    member_type: c_int,
    offset: Py_ssize_t,
    flags: c_int,
    doc: *const c_char,
}

#[repr(C)]
struct PyMemberDescrObjectLayout {
    d_common: PyDescrObjectLayout,
    d_member: *mut PyMemberDefLayout,
}

#[repr(C)]
struct PyASCIIObjectLayout {
    ob_base: PyObject,
    length: Py_ssize_t,
    hash: Py_hash_t,
    state: u32,
}

#[cfg_attr(windows, link(name = "pythonXY"))]
extern "C" {
    fn PyFunction_SetVectorcall(callable: *mut PyObject, vectorcall: vectorcallfunc);
    fn PyVectorcall_Function(callable: *mut PyObject) -> Option<vectorcallfunc>;
    #[cfg(Py_3_13)]
    fn PyLong_FromUnsignedNativeBytes(
        buffer: *const c_void,
        n_bytes: usize,
        flags: c_int,
    ) -> *mut PyObject;
    #[cfg(Py_3_13)]
    fn PyLong_AsNativeBytes(
        value: *mut PyObject,
        buffer: *mut c_void,
        n_bytes: Py_ssize_t,
        flags: c_int,
    ) -> Py_ssize_t;
    #[cfg(not(Py_3_13))]
    fn _PyLong_FromByteArray(
        bytes: *const c_uchar,
        n: usize,
        little_endian: c_int,
        is_signed: c_int,
    ) -> *mut PyObject;
    #[cfg(not(Py_3_13))]
    fn _PyLong_AsByteArray(
        value: *mut PyLongObject,
        bytes: *mut c_uchar,
        n: usize,
        little_endian: c_int,
        is_signed: c_int,
        with_exceptions: c_int,
    ) -> c_int;
    fn PyUnicode_New(size: Py_ssize_t, maxchar: u32) -> *mut PyObject;
}

unsafe fn incref(object: *mut PyObject) -> *mut PyObject {
    unsafe {
        Py_INCREF(object);
        object
    }
}

unsafe fn xdecref(object: *mut PyObject) {
    unsafe {
        if !object.is_null() {
            Py_DECREF(object);
        }
    }
}

unsafe fn none() -> *mut PyObject {
    unsafe { incref(Py_None()) }
}

unsafe fn not_implemented() -> *mut PyObject {
    unsafe { incref(Py_NotImplemented()) }
}

unsafe fn attribute(object: *mut PyObject, name: *const c_char) -> *mut PyObject {
    unsafe { PyObject_GetAttrString(object, name) }
}

unsafe fn set_attribute_interned(
    object: *mut PyObject,
    interned_name: *mut PyObject,
    value: *mut PyObject,
) -> c_int {
    unsafe { PyObject_GenericSetAttr(object, interned_name, value) }
}

unsafe fn set_slot_by_offset(object: *mut PyObject, offset: Py_ssize_t, value: *mut PyObject) {
    unsafe {
        let slot = object.cast::<u8>().offset(offset).cast::<*mut PyObject>();
        let previous = *slot;
        Py_INCREF(value);
        *slot = value;
        xdecref(previous);
    }
}

unsafe fn slot_object(object: *mut PyObject, offset: Py_ssize_t) -> *mut PyObject {
    unsafe { *(object.cast::<u8>().offset(offset).cast::<*mut PyObject>()) }
}

unsafe fn set_uuid_slots(object: *mut PyObject, value: u128, is_safe: *mut PyObject) -> c_int {
    unsafe {
        let int_object = u128_to_pylong(value);
        if int_object.is_null() {
            return -1;
        }
        let safety_value = if is_safe.is_null() {
            SAFE_UUID_UNKNOWN
        } else {
            is_safe
        };
        if INT_SLOT_OFFSET >= 0 && IS_SAFE_SLOT_OFFSET >= 0 {
            set_slot_by_offset(object, INT_SLOT_OFFSET, int_object);
            Py_DECREF(int_object);
            set_slot_by_offset(object, IS_SAFE_SLOT_OFFSET, safety_value);
            return 0;
        }
        if set_attribute_interned(object, INTERNED_INT, int_object) < 0 {
            Py_DECREF(int_object);
            return -1;
        }
        Py_DECREF(int_object);
        set_attribute_interned(object, INTERNED_IS_SAFE, safety_value)
    }
}

#[cfg(Py_3_13)]
unsafe fn u128_to_pylong(value: u128) -> *mut PyObject {
    let bytes = value.to_be_bytes();
    unsafe {
        PyLong_FromUnsignedNativeBytes(
            bytes.as_ptr().cast(),
            bytes.len(),
            PY_ASNATIVEBYTES_BIG_ENDIAN,
        )
    }
}

#[cfg(not(Py_3_13))]
unsafe fn u128_to_pylong(value: u128) -> *mut PyObject {
    let bytes = value.to_be_bytes();
    unsafe { _PyLong_FromByteArray(bytes.as_ptr(), bytes.len(), 0, 0) }
}


const PYLONG_SHIFT: u32 = 30;

#[repr(C)]
struct PyLongInternals {
    ob_refcnt: Py_ssize_t,
    ob_type: *mut PyTypeObject,
    #[cfg(not(Py_3_12))]
    ob_size: Py_ssize_t,
    #[cfg(Py_3_12)]
    lv_tag: usize,
    ob_digit: [u32; 0],
}

unsafe fn pylong_to_u128(object: *mut PyObject) -> Option<u128> {
    unsafe {
        let long = object.cast::<PyLongInternals>();

        #[cfg(not(Py_3_12))]
        let ndigits = {
            let s = (*long).ob_size;
            if s < 0 {
                return None;
            }
            s as usize
        };

        #[cfg(Py_3_12)]
        let ndigits = {
            let tag = (*long).lv_tag;
            match tag & 3 {
                2 => return None,    // negative
                1 => return Some(0), // zero
                _ => {}
            }
            tag >> 3
        };

        if ndigits > 5 {
            return None;
        }

        let digits = (*long).ob_digit.as_ptr();
        let mut value: u128 = 0;
        let mut i = ndigits;
        while i > 0 {
            i -= 1;
            value = (value << PYLONG_SHIFT) | (*digits.add(i) as u128);
        }
        Some(value)
    }
}

unsafe fn uuid_int_from_slot(object: *mut PyObject) -> Option<u128> {
    unsafe {
        if INT_SLOT_OFFSET < 0 {
            return None;
        }
        let int_object = slot_object(object, INT_SLOT_OFFSET);
        if int_object.is_null() {
            return None;
        }
        pylong_to_u128(int_object)
    }
}

unsafe fn uuid_int(object: *mut PyObject) -> Option<u128> {
    unsafe {
        if let Some(value) = uuid_int_from_slot(object) {
            return Some(value);
        }
        let int_object = attribute(object, c"int".as_ptr());
        if int_object.is_null() {
            return None;
        }
        let value = pylong_to_u128(int_object);
        Py_DECREF(int_object);
        value
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

const HEX_DECODE: [i8; 256] = {
    let mut t = [-1i8; 256];
    let mut i = 0u8;
    while i < 10 {
        t[(b'0' + i) as usize] = i as i8;
        i += 1;
    }
    i = 0;
    while i < 6 {
        t[(b'a' + i) as usize] = (10 + i) as i8;
        t[(b'A' + i) as usize] = (10 + i) as i8;
        i += 1;
    }
    t
};

fn write_hex_32(value: u128, output: &mut [u8]) {
    let mut shift = 124u32;
    for byte in output.iter_mut().take(32) {
        *byte = HEX_DIGITS[((value >> shift) & 0x0f) as usize];
        shift = shift.saturating_sub(4);
    }
}

fn write_uuid_string(value: u128, output: &mut [u8; 36]) {
    let mut compact = [0u8; 32];
    write_hex_32(value, &mut compact);
    output[0..8].copy_from_slice(&compact[0..8]);
    output[8] = b'-';
    output[9..13].copy_from_slice(&compact[8..12]);
    output[13] = b'-';
    output[14..18].copy_from_slice(&compact[12..16]);
    output[18] = b'-';
    output[19..23].copy_from_slice(&compact[16..20]);
    output[23] = b'-';
    output[24..36].copy_from_slice(&compact[20..32]);
}

unsafe fn py_ascii_from_bytes(bytes: &[u8]) -> *mut PyObject {
    unsafe {
        let unicode = PyUnicode_New(bytes.len() as Py_ssize_t, 127);
        if unicode.is_null() {
            return ptr::null_mut();
        }
        let data = unicode
            .cast::<u8>()
            .add(std::mem::size_of::<PyASCIIObjectLayout>());
        ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        unicode
    }
}

unsafe fn uuid_hex_object(value: u128) -> *mut PyObject {
    let mut bytes = [0u8; 32];
    write_hex_32(value, &mut bytes);
    unsafe { py_ascii_from_bytes(&bytes) }
}

unsafe fn uuid_string_object(value: u128) -> *mut PyObject {
    let mut bytes = [0u8; 36];
    write_uuid_string(value, &mut bytes);
    unsafe { py_ascii_from_bytes(&bytes) }
}

fn parse_uuid_hex_bytes(buf: &[u8]) -> Option<u128> {
    let mut src = buf;
    if src.len() >= 9 && src[0] == b'u' && src[1] == b'r' && src[2] == b'n' && src[3] == b':' {
        src = &src[4..];
    }
    if src.len() >= 5
        && src[0] == b'u'
        && src[1] == b'u'
        && src[2] == b'i'
        && src[3] == b'd'
        && src[4] == b':'
    {
        src = &src[5..];
    }
    if src.len() >= 2 && src[0] == b'{' && src[src.len() - 1] == b'}' {
        src = &src[1..src.len() - 1];
    }
    let mut value: u128 = 0;
    let mut digits: u32 = 0;
    let mut i = 0;
    while i < src.len() {
        let byte = src[i];
        i += 1;
        if byte == b'-' {
            continue;
        }
        let nibble = HEX_DECODE[byte as usize];
        if nibble < 0 {
            return None;
        }
        value = (value << 4) | nibble as u128;
        digits += 1;
    }
    if digits != 32 {
        return None;
    }
    Some(value)
}

unsafe fn parse_uuid_hex_pyunicode(object: *mut PyObject) -> Option<u128> {
    unsafe {
        if PyUnicode_Check(object) == 0 {
            return None;
        }
        let mut size: Py_ssize_t = 0;
        let ptr = PyUnicode_AsUTF8AndSize(object, &mut size);
        if ptr.is_null() {
            PyErr_Clear();
            return None;
        }
        parse_uuid_hex_bytes(std::slice::from_raw_parts(ptr.cast::<u8>(), size as usize))
    }
}

unsafe fn uuid_bytes_object(value: u128) -> *mut PyObject {
    let bytes = value.to_be_bytes();
    unsafe { PyBytes_FromStringAndSize(bytes.as_ptr().cast(), bytes.len() as Py_ssize_t) }
}

unsafe fn uuid_bytes_le_object(value: u128) -> *mut PyObject {
    let bytes = value.to_be_bytes();
    let little_endian_bytes = [
        bytes[3], bytes[2], bytes[1], bytes[0], bytes[5], bytes[4], bytes[7], bytes[6], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ];
    unsafe {
        PyBytes_FromStringAndSize(
            little_endian_bytes.as_ptr().cast(),
            little_endian_bytes.len() as Py_ssize_t,
        )
    }
}

unsafe fn small_unsigned_long(value: u128) -> *mut PyObject {
    unsafe { PyLong_FromUnsignedLongLong(value as c_ulonglong) }
}

unsafe fn uuid_fields_object(value: u128) -> *mut PyObject {
    unsafe {
        let tuple = PyTuple_New(6);
        if tuple.is_null() {
            return ptr::null_mut();
        }
        let values = [
            value >> 96,
            (value >> 80) & 0xffff,
            (value >> 64) & 0xffff,
            (value >> 56) & 0xff,
            (value >> 48) & 0xff,
            value & 0xffffffffffff,
        ];
        for (index, item) in values.iter().enumerate() {
            let object = small_unsigned_long(*item);
            if object.is_null() || PyTuple_SetItem(tuple, index as Py_ssize_t, object) < 0 {
                xdecref(object);
                Py_DECREF(tuple);
                return ptr::null_mut();
            }
        }
        tuple
    }
}

fn uuid_field_value(value: u128, field: u8) -> u128 {
    match field {
        0 => value >> 96,
        1 => (value >> 80) & 0xffff,
        2 => (value >> 64) & 0xffff,
        3 => (value >> 56) & 0xff,
        4 => (value >> 48) & 0xff,
        5 => ((value >> 64) & 0x0fff) << 48 | (((value >> 80) & 0xffff) << 32) | (value >> 96),
        6 => (((value >> 56) & 0x3f) << 8) | ((value >> 48) & 0xff),
        _ => value & 0xffffffffffff,
    }
}

unsafe fn uuid_field_object(value: u128, field: u8) -> *mut PyObject {
    unsafe { small_unsigned_long(uuid_field_value(value, field)) }
}

unsafe fn sequence_fast_size(object: *mut PyObject) -> Py_ssize_t {
    unsafe { Py_SIZE(object) }
}

unsafe fn sequence_fast_item(object: *mut PyObject, index: Py_ssize_t) -> *mut PyObject {
    unsafe {
        if PyList_Check(object) != 0 {
            PyList_GetItem(object, index)
        } else {
            PyTuple_GetItem(object, index)
        }
    }
}

fn apply_version_with_max(value: u128, version: u8, max_version: u8) -> Option<u128> {
    if !(1..=max_version).contains(&version) {
        return None;
    }
    let with_variant = (value & !(0xc000u128 << 48)) | (0x8000u128 << 48);
    Some((with_variant & !(0xf000u128 << 64)) | ((version as u128) << 76))
}

fn node_to_bytes(node: u128) -> [u8; 6] {
    [
        (node >> 40) as u8,
        (node >> 32) as u8,
        (node >> 24) as u8,
        (node >> 16) as u8,
        (node >> 8) as u8,
        node as u8,
    ]
}

unsafe fn allocate_uuid(value: u128, is_safe: *mut PyObject) -> *mut PyObject {
    unsafe {
        let object = PyType_GenericAlloc(UUID_TYPE.cast::<PyTypeObject>(), 0);
        if object.is_null() {
            return ptr::null_mut();
        }
        if set_uuid_slots(object, value, is_safe) < 0 {
            Py_DECREF(object);
            return ptr::null_mut();
        }
        object
    }
}

unsafe fn original_vectorcall(callable: *mut PyObject) -> Option<vectorcallfunc> {
    unsafe {
        let capsule = PyObject_GetAttrString(callable, CAPSULE_NAME);
        if capsule.is_null() {
            PyErr_Clear();
            return None;
        }
        let pointer = PyCapsule_GetPointer(capsule, CAPSULE_NAME);
        Py_DECREF(capsule);
        if pointer.is_null() {
            return None;
        }
        Some(std::mem::transmute::<*mut c_void, vectorcallfunc>(pointer))
    }
}

unsafe fn call_original(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if callable.is_null() {
            PyErr_SetString(PyExc_TypeError, c"invalid uuid function arguments".as_ptr());
            return ptr::null_mut();
        }
        match original_vectorcall(callable) {
            Some(vectorcall) => vectorcall(callable, args, nargsf, kwnames),
            None => ptr::null_mut(),
        }
    }
}

unsafe fn keyword_count(kwnames: *mut PyObject) -> Py_ssize_t {
    unsafe {
        if kwnames.is_null() {
            0
        } else {
            PyTuple_Size(kwnames)
        }
    }
}

unsafe fn keyword_name(kwnames: *mut PyObject, index: Py_ssize_t) -> *mut PyObject {
    unsafe { PyTuple_GetItem(kwnames, index) }
}

unsafe fn keyword_matches(kwnames: *mut PyObject, index: Py_ssize_t, name: *const c_char) -> bool {
    unsafe { PyUnicode_CompareWithASCIIString(keyword_name(kwnames, index), name) == 0 }
}

unsafe extern "C" fn uuid4_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 0 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        allocate_uuid(uuid::Uuid::new_v4().as_u128(), SAFE_UUID_UNKNOWN)
    }
}

unsafe fn name_bytes(name: *mut PyObject) -> Option<Vec<u8>> {
    unsafe {
        if PyUnicode_Check(name) != 0 {
            let bytes = PyUnicode_AsUTF8String(name);
            if bytes.is_null() {
                return None;
            }
            let size = PyBytes_Size(bytes);
            let pointer = PyBytes_AsString(bytes);
            let result = std::slice::from_raw_parts(pointer.cast::<u8>(), size as usize).to_vec();
            Py_DECREF(bytes);
            Some(result)
        } else if PyBytes_Check(name) != 0 {
            let size = PyBytes_Size(name);
            let pointer = PyBytes_AsString(name);
            Some(std::slice::from_raw_parts(pointer.cast::<u8>(), size as usize).to_vec())
        } else {
            None
        }
    }
}

unsafe fn uuid_hash_vectorcall<const VERSION: u8>(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 2 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let namespace = *args;
        let name = *args.add(1);
        let Some(namespace_value) = uuid_int(namespace) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let Some(name_bytes) = name_bytes(name) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let ns = uuid::Uuid::from_u128(namespace_value);
        let value = if VERSION == 3 {
            uuid::Uuid::new_v3(&ns, &name_bytes).as_u128()
        } else {
            uuid::Uuid::new_v5(&ns, &name_bytes).as_u128()
        };
        allocate_uuid(value, SAFE_UUID_UNKNOWN)
    }
}

unsafe extern "C" fn uuid3_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_hash_vectorcall::<3>(callable, args, nargsf, kwnames) }
}

unsafe extern "C" fn uuid5_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_hash_vectorcall::<5>(callable, args, nargsf, kwnames) }
}

unsafe fn parse_node_and_clock_seq(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> Option<(*mut PyObject, *mut PyObject)> {
    unsafe {
        let positional_count = PyVectorcall_NARGS(nargsf);
        if positional_count > 2 || keyword_count(kwnames) > 2 {
            call_original(callable, args, nargsf, kwnames);
            return None;
        }
        let mut node_object = if positional_count >= 1 {
            *args
        } else {
            ptr::null_mut()
        };
        let mut clock_seq_object = if positional_count >= 2 {
            *args.add(1)
        } else {
            ptr::null_mut()
        };
        for index in 0..keyword_count(kwnames) {
            let value = *args.add(positional_count as usize + index as usize);
            if keyword_matches(kwnames, index, c"node".as_ptr()) {
                if !node_object.is_null() {
                    call_original(callable, args, nargsf, kwnames);
                    return None;
                }
                node_object = value;
            } else if keyword_matches(kwnames, index, c"clock_seq".as_ptr()) {
                if !clock_seq_object.is_null() {
                    call_original(callable, args, nargsf, kwnames);
                    return None;
                }
                clock_seq_object = value;
            } else {
                call_original(callable, args, nargsf, kwnames);
                return None;
            }
        }
        Some((node_object, clock_seq_object))
    }
}

unsafe fn resolve_node(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
    node_object: *mut PyObject,
) -> Option<u128> {
    unsafe {
        let node = if node_object.is_null() || node_object == Py_None() {
            let getnode = attribute(UUID_MODULE, c"getnode".as_ptr());
            if getnode.is_null() {
                return None;
            }
            let result = PyObject_CallNoArgs(getnode);
            Py_DECREF(getnode);
            if result.is_null() {
                return None;
            }
            let Some(value) = pylong_to_u128(result) else {
                Py_DECREF(result);
                call_original(callable, args, nargsf, kwnames);
                return None;
            };
            Py_DECREF(result);
            value
        } else if let Some(value) = pylong_to_u128(node_object) {
            value
        } else {
            call_original(callable, args, nargsf, kwnames);
            return None;
        };
        if node >= (1u128 << 48) {
            call_original(callable, args, nargsf, kwnames);
            return None;
        }
        Some(node)
    }
}

unsafe fn generate_timestamp_uuid(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
    node_bytes: &[u8; 6],
    clock_seq_object: *mut PyObject,
    sorted: bool,
) -> *mut PyObject {
    unsafe {
        let value = if clock_seq_object.is_null() || clock_seq_object == Py_None() {
            if sorted {
                uuid::Uuid::now_v6(node_bytes).as_u128()
            } else {
                uuid::Uuid::now_v1(node_bytes).as_u128()
            }
        } else if let Some(clock_seq_value) = pylong_to_u128(clock_seq_object) {
            let clock_seq = (clock_seq_value & 0x3fff) as u16;
            let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(duration) => duration,
                Err(_) => return call_original(callable, args, nargsf, kwnames),
            };
            let ts = uuid::Timestamp::from_unix(
                uuid::ContextV1::new_random(),
                now.as_secs(),
                now.subsec_nanos(),
            );
            let ticks = ts.to_gregorian().0;
            if sorted {
                uuid::Builder::from_sorted_gregorian_timestamp(ticks, clock_seq, node_bytes)
            } else {
                uuid::Builder::from_gregorian_timestamp(ticks, clock_seq, node_bytes)
            }
            .into_uuid()
            .as_u128()
        } else {
            return call_original(callable, args, nargsf, kwnames);
        };
        allocate_uuid(value, SAFE_UUID_UNKNOWN)
    }
}

unsafe extern "C" fn uuid1_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some((node_object, clock_seq_object)) =
            parse_node_and_clock_seq(callable, args, nargsf, kwnames)
        else {
            return ptr::null_mut();
        };
        let Some(node) = resolve_node(callable, args, nargsf, kwnames, node_object) else {
            return ptr::null_mut();
        };
        let node_bytes = node_to_bytes(node);
        generate_timestamp_uuid(
            callable,
            args,
            nargsf,
            kwnames,
            &node_bytes,
            clock_seq_object,
            false,
        )
    }
}

unsafe fn uuid6_generate(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some((node_object, clock_seq_object)) =
            parse_node_and_clock_seq(callable, args, nargsf, kwnames)
        else {
            return ptr::null_mut();
        };
        let Some(node) = resolve_node(callable, args, nargsf, kwnames, node_object) else {
            return ptr::null_mut();
        };
        let node_bytes = node_to_bytes(node);
        generate_timestamp_uuid(
            callable,
            args,
            nargsf,
            kwnames,
            &node_bytes,
            clock_seq_object,
            true,
        )
    }
}

unsafe extern "C" fn uuid6_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid6_generate(callable, args, nargsf, kwnames) }
}

unsafe fn uuid7_generate() -> *mut PyObject {
    unsafe { allocate_uuid(uuid::Uuid::now_v7().as_u128(), SAFE_UUID_UNKNOWN) }
}

unsafe extern "C" fn uuid7_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 0 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        uuid7_generate()
    }
}

unsafe fn bytes_to_uuid_int(object: *mut PyObject, little_endian: bool) -> Option<u128> {
    unsafe {
        if PyBytes_Check(object) == 0 || PyBytes_Size(object) != 16 {
            return None;
        }
        let pointer = PyBytes_AsString(object).cast::<u8>();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(std::slice::from_raw_parts(pointer, 16));
        if little_endian {
            bytes[0..4].reverse();
            bytes[4..6].reverse();
            bytes[6..8].reverse();
        }
        Some(u128::from_be_bytes(bytes))
    }
}

unsafe fn fields_to_uuid_int(object: *mut PyObject) -> Option<u128> {
    unsafe {
        let sequence = PySequence_Fast(object, c"fields is not a sequence".as_ptr());
        if sequence.is_null() {
            PyErr_Clear();
            return None;
        }
        if sequence_fast_size(sequence) != 6 {
            Py_DECREF(sequence);
            return None;
        }
        let mut values = [0u128; 6];
        for index in 0..6 {
            let item = sequence_fast_item(sequence, index as Py_ssize_t);
            let Some(value) = pylong_to_u128(item) else {
                Py_DECREF(sequence);
                return None;
            };
            values[index] = value;
        }
        Py_DECREF(sequence);
        if values[0] >= (1u128 << 32)
            || values[1] >= (1u128 << 16)
            || values[2] >= (1u128 << 16)
            || values[3] >= (1u128 << 8)
            || values[4] >= (1u128 << 8)
            || values[5] >= (1u128 << 48)
        {
            return None;
        }
        Some(
            (values[0] << 96)
                | (values[1] << 80)
                | (values[2] << 64)
                | (values[3] << 56)
                | (values[4] << 48)
                | values[5],
        )
    }
}

unsafe extern "C" fn uuid_init_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let positional_count = PyVectorcall_NARGS(nargsf);
        if positional_count == 0 || positional_count > 7 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let self_object = *args;
        let mut hex_object = if positional_count >= 2 {
            *args.add(1)
        } else {
            Py_None()
        };
        let mut bytes_object = if positional_count >= 3 {
            *args.add(2)
        } else {
            Py_None()
        };
        let mut bytes_le_object = if positional_count >= 4 {
            *args.add(3)
        } else {
            Py_None()
        };
        let mut fields_object = if positional_count >= 5 {
            *args.add(4)
        } else {
            Py_None()
        };
        let mut int_object = if positional_count >= 6 {
            *args.add(5)
        } else {
            Py_None()
        };
        let mut version_object = if positional_count >= 7 {
            *args.add(6)
        } else {
            Py_None()
        };
        let mut is_safe_object = SAFE_UUID_UNKNOWN;

        for index in 0..keyword_count(kwnames) {
            let value = *args.add(positional_count as usize + index as usize);
            let target = if keyword_matches(kwnames, index, c"hex".as_ptr()) {
                &mut hex_object
            } else if keyword_matches(kwnames, index, c"bytes".as_ptr()) {
                &mut bytes_object
            } else if keyword_matches(kwnames, index, c"bytes_le".as_ptr()) {
                &mut bytes_le_object
            } else if keyword_matches(kwnames, index, c"fields".as_ptr()) {
                &mut fields_object
            } else if keyword_matches(kwnames, index, c"int".as_ptr()) {
                &mut int_object
            } else if keyword_matches(kwnames, index, c"version".as_ptr()) {
                &mut version_object
            } else if keyword_matches(kwnames, index, c"is_safe".as_ptr()) {
                &mut is_safe_object
            } else {
                return call_original(callable, args, nargsf, kwnames);
            };
            if *target != Py_None() {
                return call_original(callable, args, nargsf, kwnames);
            }
            *target = value;
        }

        let sources = [
            hex_object,
            bytes_object,
            bytes_le_object,
            fields_object,
            int_object,
        ];
        if sources
            .iter()
            .filter(|source| **source != Py_None())
            .count()
            != 1
        {
            return call_original(callable, args, nargsf, kwnames);
        }

        let mut value = if hex_object != Py_None() {
            let Some(value) = parse_uuid_hex_pyunicode(hex_object) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            value
        } else if bytes_object != Py_None() {
            let Some(value) = bytes_to_uuid_int(bytes_object, false) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            value
        } else if bytes_le_object != Py_None() {
            let Some(value) = bytes_to_uuid_int(bytes_le_object, true) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            value
        } else if fields_object != Py_None() {
            let Some(value) = fields_to_uuid_int(fields_object) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            value
        } else {
            let Some(value) = pylong_to_u128(int_object) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            value
        };

        if version_object != Py_None() {
            let Some(version_value) = pylong_to_u128(version_object) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            let Ok(version) = u8::try_from(version_value) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            let Some(versioned_value) = apply_version_with_max(value, version, MAX_UUID_VERSION)
            else {
                return call_original(callable, args, nargsf, kwnames);
            };
            value = versioned_value;
        }

        if set_uuid_slots(self_object, value, is_safe_object) < 0 {
            return ptr::null_mut();
        }
        none()
    }
}

unsafe fn unary_self(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> Option<*mut PyObject> {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 1 || keyword_count(kwnames) != 0 {
            call_original(callable, args, nargsf, kwnames);
            None
        } else {
            Some(*args)
        }
    }
}

unsafe extern "C" fn uuid_str_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_string_object(value)
    }
}

unsafe extern "C" fn uuid_hex_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_hex_object(value)
    }
}

unsafe extern "C" fn uuid_repr_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let class = PyObject_Type(self_object);
        if class.is_null() {
            return ptr::null_mut();
        }
        let name = attribute(class, c"__name__".as_ptr());
        Py_DECREF(class);
        if name.is_null() {
            return ptr::null_mut();
        }
        let string = uuid_str_vectorcall(callable, args, nargsf, kwnames);
        if string.is_null() {
            Py_DECREF(name);
            return ptr::null_mut();
        }
        let result = PyUnicode_FromFormat(c"%U('%U')".as_ptr(), name, string);
        Py_DECREF(name);
        Py_DECREF(string);
        result
    }
}

unsafe extern "C" fn uuid_int_method_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let int_object = attribute(self_object, c"int".as_ptr());
        if int_object.is_null() {
            call_original(callable, args, nargsf, kwnames)
        } else {
            int_object
        }
    }
}

unsafe extern "C" fn uuid_hash_method_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let int_object = if INT_SLOT_OFFSET >= 0 {
            slot_object(self_object, INT_SLOT_OFFSET)
        } else {
            ptr::null_mut()
        };
        let owned_int_object = if int_object.is_null() {
            let object = attribute(self_object, c"int".as_ptr());
            if object.is_null() {
                return call_original(callable, args, nargsf, kwnames);
            }
            object
        } else {
            ptr::null_mut()
        };
        let hash_target = if int_object.is_null() {
            owned_int_object
        } else {
            int_object
        };
        let hash = PyObject_Hash(hash_target);
        xdecref(owned_int_object);
        if hash == -1 && !PyErr_Occurred().is_null() {
            return ptr::null_mut();
        }
        PyLong_FromSsize_t(hash)
    }
}

unsafe extern "C" fn uuid_from_int_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 2 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let class = *args;
        let value_object = *args.add(1);
        if PyLong_Check(value_object) == 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let subclass_check = PyObject_IsSubclass(class, UUID_TYPE);
        if subclass_check < 0 {
            return ptr::null_mut();
        }
        if subclass_check == 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        if pylong_to_u128(value_object).is_none() {
            let repr = PyObject_Repr(value_object);
            if repr.is_null() {
                return ptr::null_mut();
            }
            PyErr_SetObject(PyExc_AssertionError, repr);
            Py_DECREF(repr);
            return ptr::null_mut();
        };
        let object = PyType_GenericAlloc(class.cast::<PyTypeObject>(), 0);
        if object.is_null() {
            return ptr::null_mut();
        }
        if INT_SLOT_OFFSET >= 0 && IS_SAFE_SLOT_OFFSET >= 0 {
            set_slot_by_offset(object, INT_SLOT_OFFSET, value_object);
            set_slot_by_offset(object, IS_SAFE_SLOT_OFFSET, SAFE_UUID_UNKNOWN);
        } else {
            if set_attribute_interned(object, INTERNED_INT, value_object) < 0 {
                Py_DECREF(object);
                return ptr::null_mut();
            }
            if set_attribute_interned(object, INTERNED_IS_SAFE, SAFE_UUID_UNKNOWN) < 0 {
                Py_DECREF(object);
                return ptr::null_mut();
            }
        }
        object
    }
}

unsafe extern "C" fn uuid_setattr_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 3 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let self_object = *args;
        let name = *args.add(1);
        let value = *args.add(2);
        let object_type = PyObject_Type(self_object);
        if object_type.is_null() {
            return ptr::null_mut();
        }
        let is_exact_uuid = object_type == UUID_TYPE;
        Py_DECREF(object_type);
        let is_uuid_slot = PyUnicode_CompareWithASCIIString(name, c"int".as_ptr()) == 0
            || PyUnicode_CompareWithASCIIString(name, c"is_safe".as_ptr()) == 0;
        if !is_exact_uuid && !is_uuid_slot {
            if PyObject_GenericSetAttr(self_object, name, value) < 0 {
                return ptr::null_mut();
            }
            return none();
        }
        PyErr_SetString(PyExc_TypeError, c"UUID objects are immutable".as_ptr());
        ptr::null_mut()
    }
}

unsafe fn rich_compare<const OP: u8>(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 2 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let self_object = *args;
        let other_object = *args.add(1);
        let instance_check = PyObject_IsInstance(other_object, UUID_TYPE);
        if instance_check < 0 {
            return ptr::null_mut();
        }
        if instance_check == 0 {
            return not_implemented();
        }
        let Some(left) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let Some(right) = uuid_int(other_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let result = match OP {
            0 => left == right,
            1 => left < right,
            2 => left > right,
            3 => left <= right,
            _ => left >= right,
        };
        PyBool_FromLong(result as c_long)
    }
}

unsafe extern "C" fn uuid_eq_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<0>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_lt_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<1>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_gt_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<2>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_le_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<3>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_ge_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<4>(callable, args, nargsf, kwnames) }
}

unsafe extern "C" fn uuid_bytes_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_bytes_object(value)
    }
}

unsafe extern "C" fn uuid_bytes_le_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_bytes_le_object(value)
    }
}

unsafe extern "C" fn uuid_fields_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_fields_object(value)
    }
}

unsafe extern "C" fn uuid_field_value_vectorcall<const FIELD: u8>(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_field_object(value, FIELD)
    }
}

unsafe extern "C" fn uuid_time_low_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<0>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_time_mid_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<1>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_time_hi_version_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<2>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_clock_seq_hi_variant_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<3>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_clock_seq_low_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<4>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_time_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<5>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_clock_seq_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<6>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_node_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { uuid_field_value_vectorcall::<7>(callable, args, nargsf, kwnames) }
}

unsafe fn uuid_urn_bytes(value: u128) -> *mut PyObject {
    let mut bytes = [0u8; 45];
    bytes[0..9].copy_from_slice(b"urn:uuid:");
    let mut uuid_part = [0u8; 36];
    write_uuid_string(value, &mut uuid_part);
    bytes[9..45].copy_from_slice(&uuid_part);
    unsafe { py_ascii_from_bytes(&bytes) }
}

unsafe extern "C" fn uuid_urn_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        uuid_urn_bytes(value)
    }
}

unsafe extern "C" fn uuid_variant_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let result = if value & (0x8000u128 << 48) == 0 {
            RESERVED_NCS_VALUE
        } else if value & (0x4000u128 << 48) == 0 {
            RFC_4122_VALUE
        } else if value & (0x2000u128 << 48) == 0 {
            RESERVED_MICROSOFT_VALUE
        } else {
            RESERVED_FUTURE_VALUE
        };
        incref(result)
    }
}

unsafe extern "C" fn uuid_version_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        let Some(value) = uuid_int(self_object) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        if value & (0x8000u128 << 48) != 0 && value & (0x4000u128 << 48) == 0 {
            PyLong_FromLong(((value >> 76) & 0xf) as c_long)
        } else {
            none()
        }
    }
}

unsafe extern "C" fn uuid_getstate_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else {
            return ptr::null_mut();
        };
        if INT_SLOT_OFFSET < 0 || IS_SAFE_SLOT_OFFSET < 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let int_object = slot_object(self_object, INT_SLOT_OFFSET);
        let is_safe = slot_object(self_object, IS_SAFE_SLOT_OFFSET);
        if int_object.is_null() || is_safe.is_null() {
            return call_original(callable, args, nargsf, kwnames);
        }
        let state = PyDict_New();
        if state.is_null() {
            return ptr::null_mut();
        }
        if PyDict_SetItem(state, INTERNED_INT, int_object) < 0 {
            Py_DECREF(state);
            return ptr::null_mut();
        }
        if is_safe != SAFE_UUID_UNKNOWN {
            let is_safe_value = PyObject_GetAttr(is_safe, INTERNED_VALUE);
            if is_safe_value.is_null() {
                Py_DECREF(state);
                return ptr::null_mut();
            }
            let result = PyDict_SetItem(state, INTERNED_IS_SAFE, is_safe_value);
            Py_DECREF(is_safe_value);
            if result < 0 {
                Py_DECREF(state);
                return ptr::null_mut();
            }
        }
        state
    }
}

unsafe extern "C" fn uuid_setstate_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 2 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        if INT_SLOT_OFFSET < 0 || IS_SAFE_SLOT_OFFSET < 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let self_object = *args;
        let state = *args.add(1);
        if PyDict_Check(state) == 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let int_object = PyDict_GetItem(state, INTERNED_INT);
        if int_object.is_null() {
            PyErr_Clear();
            return call_original(callable, args, nargsf, kwnames);
        }
        let is_safe_value = PyDict_GetItem(state, INTERNED_IS_SAFE);
        if !is_safe_value.is_null() {
            PyErr_Clear();
        }
        let is_safe = if is_safe_value.is_null() {
            SAFE_UUID_UNKNOWN
        } else {
            let object = PyObject_CallOneArg(SAFE_UUID_TYPE, is_safe_value);
            if object.is_null() {
                return ptr::null_mut();
            }
            object
        };
        set_slot_by_offset(self_object, INT_SLOT_OFFSET, int_object);
        set_slot_by_offset(self_object, IS_SAFE_SLOT_OFFSET, is_safe);
        if !is_safe_value.is_null() {
            Py_DECREF(is_safe);
        }
        none()
    }
}

unsafe fn patch_function(function: *mut PyObject, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let current = match PyVectorcall_Function(function) {
            Some(current) => current,
            None => {
                PyErr_SetString(
                    PyExc_RuntimeError,
                    c"uuideal: function has no vectorcall slot".as_ptr(),
                );
                return -1;
            }
        };
        if PyObject_HasAttrString(function, CAPSULE_NAME) != 0 {
            return 0;
        }
        let capsule = PyCapsule_New(
            std::mem::transmute::<vectorcallfunc, *mut c_void>(current),
            CAPSULE_NAME,
            None,
        );
        if capsule.is_null() {
            return -1;
        }
        if PyObject_SetAttrString(function, CAPSULE_NAME, capsule) < 0 {
            Py_DECREF(capsule);
            return -1;
        }
        Py_DECREF(capsule);
        PyFunction_SetVectorcall(function, vectorcall);
        0
    }
}

unsafe fn restore_function(function: *mut PyObject) -> c_int {
    unsafe {
        let capsule = PyObject_GetAttrString(function, CAPSULE_NAME);
        if capsule.is_null() {
            PyErr_Clear();
            return 0;
        }
        let pointer = PyCapsule_GetPointer(capsule, CAPSULE_NAME);
        Py_DECREF(capsule);
        if pointer.is_null() {
            return -1;
        }
        let vectorcall = std::mem::transmute::<*mut c_void, vectorcallfunc>(pointer);
        PyFunction_SetVectorcall(function, vectorcall);
        PyObject_DelAttrString(function, CAPSULE_NAME);
        PyErr_Clear();
        0
    }
}

unsafe fn patch_module_function(
    module: *mut PyObject,
    name: *const c_char,
    vectorcall: vectorcallfunc,
) -> c_int {
    unsafe {
        let function = attribute(module, name);
        if function.is_null() {
            return -1;
        }
        let result = patch_function(function, vectorcall);
        Py_DECREF(function);
        result
    }
}

unsafe fn patch_optional_module_function(
    module: *mut PyObject,
    name: *const c_char,
    vectorcall: vectorcallfunc,
) -> c_int {
    unsafe {
        let function = attribute(module, name);
        if function.is_null() {
            PyErr_Clear();
            return 0;
        }
        let result = patch_function(function, vectorcall);
        Py_DECREF(function);
        result
    }
}

unsafe fn restore_module_function(module: *mut PyObject, name: *const c_char) -> c_int {
    unsafe {
        let function = attribute(module, name);
        if function.is_null() {
            PyErr_Clear();
            return 0;
        }
        let result = restore_function(function);
        Py_DECREF(function);
        result
    }
}

unsafe fn patch_uuid_method(name: *const c_char, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let function = attribute(UUID_TYPE, name);
        if function.is_null() {
            return -1;
        }
        let result = patch_function(function, vectorcall);
        Py_DECREF(function);
        result
    }
}

unsafe fn restore_uuid_method(name: *const c_char) -> c_int {
    unsafe {
        let function = attribute(UUID_TYPE, name);
        if function.is_null() {
            PyErr_Clear();
            return 0;
        }
        let result = restore_function(function);
        Py_DECREF(function);
        result
    }
}

unsafe fn patch_optional_uuid_classmethod(
    name: *const c_char,
    vectorcall: vectorcallfunc,
) -> c_int {
    unsafe {
        let method = attribute(UUID_TYPE, name);
        if method.is_null() {
            PyErr_Clear();
            return 0;
        }
        let function = attribute(method, c"__func__".as_ptr());
        Py_DECREF(method);
        if function.is_null() {
            PyErr_Clear();
            return 0;
        }
        let result = patch_function(function, vectorcall);
        Py_DECREF(function);
        result
    }
}

unsafe fn restore_uuid_classmethod(name: *const c_char) -> c_int {
    unsafe {
        let method = attribute(UUID_TYPE, name);
        if method.is_null() {
            PyErr_Clear();
            return 0;
        }
        let function = attribute(method, c"__func__".as_ptr());
        Py_DECREF(method);
        if function.is_null() {
            PyErr_Clear();
            return 0;
        }
        let result = restore_function(function);
        Py_DECREF(function);
        result
    }
}

unsafe fn patch_uuid_property(name: *const c_char, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let property = attribute(UUID_TYPE, name);
        if property.is_null() {
            return -1;
        }
        let getter = attribute(property, c"fget".as_ptr());
        Py_DECREF(property);
        if getter.is_null() {
            return -1;
        }
        let result = patch_function(getter, vectorcall);
        Py_DECREF(getter);
        result
    }
}

unsafe fn restore_uuid_property(name: *const c_char) -> c_int {
    unsafe {
        let property = attribute(UUID_TYPE, name);
        if property.is_null() {
            PyErr_Clear();
            return 0;
        }
        let getter = attribute(property, c"fget".as_ptr());
        Py_DECREF(property);
        if getter.is_null() {
            PyErr_Clear();
            return 0;
        }
        let result = restore_function(getter);
        Py_DECREF(getter);
        result
    }
}

unsafe fn member_descriptor_offset(descriptor: *mut PyObject) -> Option<Py_ssize_t> {
    unsafe {
        if descriptor.is_null() {
            return None;
        }
        let member_descriptor = descriptor.cast::<PyMemberDescrObjectLayout>();
        let member = (*member_descriptor).d_member;
        if member.is_null() {
            None
        } else {
            Some((*member).offset)
        }
    }
}

unsafe fn load_uuid_slot_offsets() -> c_int {
    unsafe {
        let int_descriptor = attribute(UUID_TYPE, c"int".as_ptr());
        let is_safe_descriptor = attribute(UUID_TYPE, c"is_safe".as_ptr());
        let int_offset = member_descriptor_offset(int_descriptor);
        let is_safe_offset = member_descriptor_offset(is_safe_descriptor);
        xdecref(int_descriptor);
        xdecref(is_safe_descriptor);
        let (Some(int_offset), Some(is_safe_offset)) = (int_offset, is_safe_offset) else {
            PyErr_SetString(
                PyExc_RuntimeError,
                c"uuideal: unable to resolve UUID slot offsets".as_ptr(),
            );
            return -1;
        };
        INT_SLOT_OFFSET = int_offset;
        IS_SAFE_SLOT_OFFSET = is_safe_offset;
        0
    }
}

unsafe fn load_uuid_references() -> c_int {
    unsafe {
        if !UUID_MODULE.is_null() {
            return 0;
        }
        let module = PyImport_ImportModule(c"uuid".as_ptr());
        if module.is_null() {
            return -1;
        }
        UUID_MODULE = module;
        UUID_TYPE = attribute(module, c"UUID".as_ptr());
        SAFE_UUID_TYPE = attribute(module, c"SafeUUID".as_ptr());
        if SAFE_UUID_TYPE.is_null() {
            return -1;
        }
        SAFE_UUID_UNKNOWN = attribute(SAFE_UUID_TYPE, c"unknown".as_ptr());
        RESERVED_NCS_VALUE = attribute(module, c"RESERVED_NCS".as_ptr());
        RFC_4122_VALUE = attribute(module, c"RFC_4122".as_ptr());
        RESERVED_MICROSOFT_VALUE = attribute(module, c"RESERVED_MICROSOFT".as_ptr());
        RESERVED_FUTURE_VALUE = attribute(module, c"RESERVED_FUTURE".as_ptr());
        let uuid6_function = attribute(module, c"uuid6".as_ptr());
        if uuid6_function.is_null() {
            PyErr_Clear();
            MAX_UUID_VERSION = 5;
        } else {
            Py_DECREF(uuid6_function);
            MAX_UUID_VERSION = 8;
        }
        if UUID_TYPE.is_null()
            || SAFE_UUID_TYPE.is_null()
            || SAFE_UUID_UNKNOWN.is_null()
            || RESERVED_NCS_VALUE.is_null()
            || RFC_4122_VALUE.is_null()
            || RESERVED_MICROSOFT_VALUE.is_null()
            || RESERVED_FUTURE_VALUE.is_null()
        {
            return -1;
        }
        if INTERNED_INT.is_null() {
            INTERNED_INT = PyUnicode_InternFromString(c"int".as_ptr());
            INTERNED_IS_SAFE = PyUnicode_InternFromString(c"is_safe".as_ptr());
            INTERNED_VALUE = PyUnicode_InternFromString(c"value".as_ptr());
            if INTERNED_INT.is_null()
                || INTERNED_IS_SAFE.is_null()
                || INTERNED_VALUE.is_null()
            {
                return -1;
            }
        }
        if load_uuid_slot_offsets() < 0 {
            return -1;
        }
        0
    }
}

unsafe fn apply_all_patches() -> c_int {
    unsafe {
        if load_uuid_references() < 0 {
            return -1;
        }
        let module_patches = [
            (c"uuid1".as_ptr(), uuid1_vectorcall as vectorcallfunc),
            (c"uuid3".as_ptr(), uuid3_vectorcall as vectorcallfunc),
            (c"uuid4".as_ptr(), uuid4_vectorcall as vectorcallfunc),
            (c"uuid5".as_ptr(), uuid5_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in module_patches {
            if patch_module_function(UUID_MODULE, name, vectorcall) < 0 {
                return -1;
            }
        }
        let optional_module_patches = [
            (c"uuid6".as_ptr(), uuid6_vectorcall as vectorcallfunc),
            (c"uuid7".as_ptr(), uuid7_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in optional_module_patches {
            if patch_optional_module_function(UUID_MODULE, name, vectorcall) < 0 {
                return -1;
            }
        }
        let method_patches = [
            (c"__init__".as_ptr(), uuid_init_vectorcall as vectorcallfunc),
            (
                c"__getstate__".as_ptr(),
                uuid_getstate_vectorcall as vectorcallfunc,
            ),
            (
                c"__setstate__".as_ptr(),
                uuid_setstate_vectorcall as vectorcallfunc,
            ),
            (c"__eq__".as_ptr(), uuid_eq_vectorcall as vectorcallfunc),
            (c"__lt__".as_ptr(), uuid_lt_vectorcall as vectorcallfunc),
            (c"__gt__".as_ptr(), uuid_gt_vectorcall as vectorcallfunc),
            (c"__le__".as_ptr(), uuid_le_vectorcall as vectorcallfunc),
            (c"__ge__".as_ptr(), uuid_ge_vectorcall as vectorcallfunc),
            (
                c"__hash__".as_ptr(),
                uuid_hash_method_vectorcall as vectorcallfunc,
            ),
            (
                c"__int__".as_ptr(),
                uuid_int_method_vectorcall as vectorcallfunc,
            ),
            (c"__repr__".as_ptr(), uuid_repr_vectorcall as vectorcallfunc),
            (
                c"__setattr__".as_ptr(),
                uuid_setattr_vectorcall as vectorcallfunc,
            ),
            (c"__str__".as_ptr(), uuid_str_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in method_patches {
            if patch_uuid_method(name, vectorcall) < 0 {
                return -1;
            }
        }
        if patch_optional_uuid_classmethod(
            c"_from_int".as_ptr(),
            uuid_from_int_vectorcall as vectorcallfunc,
        ) < 0
        {
            return -1;
        }
        let property_patches = [
            (c"bytes".as_ptr(), uuid_bytes_vectorcall as vectorcallfunc),
            (
                c"bytes_le".as_ptr(),
                uuid_bytes_le_vectorcall as vectorcallfunc,
            ),
            (c"fields".as_ptr(), uuid_fields_vectorcall as vectorcallfunc),
            (
                c"time_low".as_ptr(),
                uuid_time_low_vectorcall as vectorcallfunc,
            ),
            (
                c"time_mid".as_ptr(),
                uuid_time_mid_vectorcall as vectorcallfunc,
            ),
            (
                c"time_hi_version".as_ptr(),
                uuid_time_hi_version_vectorcall as vectorcallfunc,
            ),
            (
                c"clock_seq_hi_variant".as_ptr(),
                uuid_clock_seq_hi_variant_vectorcall as vectorcallfunc,
            ),
            (
                c"clock_seq_low".as_ptr(),
                uuid_clock_seq_low_vectorcall as vectorcallfunc,
            ),
            (c"time".as_ptr(), uuid_time_vectorcall as vectorcallfunc),
            (
                c"clock_seq".as_ptr(),
                uuid_clock_seq_vectorcall as vectorcallfunc,
            ),
            (c"node".as_ptr(), uuid_node_vectorcall as vectorcallfunc),
            (c"hex".as_ptr(), uuid_hex_vectorcall as vectorcallfunc),
            (c"urn".as_ptr(), uuid_urn_vectorcall as vectorcallfunc),
            (c"variant".as_ptr(), uuid_variant_vectorcall as vectorcallfunc),
            (c"version".as_ptr(), uuid_version_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in property_patches {
            if patch_uuid_property(name, vectorcall) < 0 {
                return -1;
            }
        }
        0
    }
}

unsafe fn restore_all_patches() -> c_int {
    unsafe {
        if UUID_MODULE.is_null() {
            return 0;
        }
        for name in [
            c"uuid1".as_ptr(),
            c"uuid3".as_ptr(),
            c"uuid4".as_ptr(),
            c"uuid5".as_ptr(),
            c"uuid6".as_ptr(),
            c"uuid7".as_ptr(),
        ] {
            if restore_module_function(UUID_MODULE, name) < 0 {
                return -1;
            }
        }
        for name in [
            c"__init__".as_ptr(),
            c"__getstate__".as_ptr(),
            c"__setstate__".as_ptr(),
            c"__eq__".as_ptr(),
            c"__lt__".as_ptr(),
            c"__gt__".as_ptr(),
            c"__le__".as_ptr(),
            c"__ge__".as_ptr(),
            c"__hash__".as_ptr(),
            c"__int__".as_ptr(),
            c"__repr__".as_ptr(),
            c"__setattr__".as_ptr(),
            c"__str__".as_ptr(),
        ] {
            if restore_uuid_method(name) < 0 {
                return -1;
            }
        }
        if restore_uuid_classmethod(c"_from_int".as_ptr()) < 0 {
            return -1;
        }
        for name in [
            c"bytes".as_ptr(),
            c"bytes_le".as_ptr(),
            c"fields".as_ptr(),
            c"time_low".as_ptr(),
            c"time_mid".as_ptr(),
            c"time_hi_version".as_ptr(),
            c"clock_seq_hi_variant".as_ptr(),
            c"clock_seq_low".as_ptr(),
            c"time".as_ptr(),
            c"clock_seq".as_ptr(),
            c"node".as_ptr(),
            c"hex".as_ptr(),
            c"urn".as_ptr(),
            c"variant".as_ptr(),
            c"version".as_ptr(),
        ] {
            if restore_uuid_property(name) < 0 {
                return -1;
            }
        }
        0
    }
}

unsafe extern "C" fn py_uuid6(
    _self: *mut PyObject,
    args: *const *mut PyObject,
    nargs: Py_ssize_t,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if load_uuid_references() < 0 {
            return ptr::null_mut();
        }
        uuid6_generate(ptr::null_mut(), args, nargs as usize, kwnames)
    }
}

unsafe extern "C" fn py_uuid7(
    _self: *mut PyObject,
    _args: *const *mut PyObject,
    nargs: Py_ssize_t,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if load_uuid_references() < 0 {
            return ptr::null_mut();
        }
        if nargs != 0 || keyword_count(kwnames) != 0 {
            PyErr_SetString(PyExc_TypeError, c"uuid7() takes no arguments".as_ptr());
            return ptr::null_mut();
        }
        uuid7_generate()
    }
}

unsafe extern "C" fn py_enable(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe {
        if !PATCHED.load(Ordering::SeqCst) {
            if apply_all_patches() < 0 {
                return ptr::null_mut();
            }
            PATCHED.store(true, Ordering::SeqCst);
        }
        none()
    }
}

unsafe extern "C" fn py_disable(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe {
        if PATCHED.load(Ordering::SeqCst) {
            if restore_all_patches() < 0 {
                return ptr::null_mut();
            }
            PATCHED.store(false, Ordering::SeqCst);
        }
        none()
    }
}

unsafe extern "C" fn py_enabled(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe { PyBool_FromLong(PATCHED.load(Ordering::SeqCst) as c_long) }
}

static mut METHODS: [PyMethodDef; 9] = [PyMethodDef::zeroed(); 9];

unsafe fn init_methods() {
    unsafe {
        METHODS[0] = PyMethodDef {
            ml_name: c"enable".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_enable,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Enable uuid vectorcall patches.".as_ptr(),
        };
        METHODS[1] = PyMethodDef {
            ml_name: c"install".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_enable,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Enable uuid vectorcall patches.".as_ptr(),
        };
        METHODS[2] = PyMethodDef {
            ml_name: c"disable".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_disable,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Disable uuid vectorcall patches.".as_ptr(),
        };
        METHODS[3] = PyMethodDef {
            ml_name: c"uninstall".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_disable,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Disable uuid vectorcall patches.".as_ptr(),
        };
        METHODS[4] = PyMethodDef {
            ml_name: c"enabled".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_enabled,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Return whether uuid vectorcall patches are enabled.".as_ptr(),
        };
        METHODS[5] = PyMethodDef {
            ml_name: c"uuid6".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunctionFastWithKeywords: py_uuid6,
            },
            ml_flags: METH_FASTCALL | METH_KEYWORDS,
            ml_doc: c"Generate a version 6 UUID.".as_ptr(),
        };
        METHODS[6] = PyMethodDef {
            ml_name: c"uuid7".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunctionFastWithKeywords: py_uuid7,
            },
            ml_flags: METH_FASTCALL | METH_KEYWORDS,
            ml_doc: c"Generate a version 7 UUID.".as_ptr(),
        };
        METHODS[8] = PyMethodDef::zeroed();
    }
}

static mut MODULE_DEF: PyModuleDef = PyModuleDef {
    m_base: PyModuleDef_HEAD_INIT,
    m_name: c"uuideal._uuideal".as_ptr(),
    m_doc: c"Vectorcall patches for stdlib uuid.".as_ptr(),
    m_size: 0,
    m_methods: ptr::null_mut(),
    m_slots: ptr::null_mut(),
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

#[no_mangle]
pub unsafe extern "C" fn PyInit__uuideal() -> *mut PyObject {
    unsafe {
        init_methods();
        MODULE_DEF.m_methods = ptr::addr_of_mut!(METHODS).cast::<PyMethodDef>();
        PyModuleDef_Init(ptr::addr_of_mut!(MODULE_DEF))
    }
}
