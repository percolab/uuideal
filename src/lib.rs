#![allow(non_snake_case)]

use core::ffi::{c_char, c_int, c_long, c_void};
use md5::{Digest, Md5};
use pyo3_ffi::*;
use sha1::Sha1;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

macro_rules! cstr {
    ($value:literal) => {
        concat!($value, "\0").as_ptr().cast::<c_char>()
    };
}

const CAPSULE_NAME: *const c_char = cstr!("uuideal._original_vectorcall");
const UUID_EPOCH_OFFSET: u128 = 0x01b21dd213814000;
const PY_ASNATIVEBYTES_BIG_ENDIAN: c_int = 0;
const PY_ASNATIVEBYTES_REJECT_NEGATIVE: c_int = 8;

static PATCHED: AtomicBool = AtomicBool::new(false);
static LAST_TIMESTAMP: AtomicU64 = AtomicU64::new(0);

static mut UUID_MODULE: *mut PyObject = ptr::null_mut();
static mut UUID_TYPE: *mut PyObject = ptr::null_mut();
static mut SAFE_UUID_UNKNOWN: *mut PyObject = ptr::null_mut();
static mut RESERVED_NCS_VALUE: *mut PyObject = ptr::null_mut();
static mut RFC_4122_VALUE: *mut PyObject = ptr::null_mut();
static mut RESERVED_MICROSOFT_VALUE: *mut PyObject = ptr::null_mut();
static mut RESERVED_FUTURE_VALUE: *mut PyObject = ptr::null_mut();

#[cfg_attr(windows, link(name = "pythonXY"))]
extern "C" {
    fn PyFunction_SetVectorcall(callable: *mut PyObject, vectorcall: vectorcallfunc);
    fn PyVectorcall_Function(callable: *mut PyObject) -> Option<vectorcallfunc>;
    fn PyLong_FromUnsignedNativeBytes(buffer: *const c_void, n_bytes: usize, flags: c_int) -> *mut PyObject;
    fn PyLong_AsNativeBytes(value: *mut PyObject, buffer: *mut c_void, n_bytes: Py_ssize_t, flags: c_int) -> Py_ssize_t;
}

unsafe fn incref(object: *mut PyObject) -> *mut PyObject {
    unsafe {
        Py_INCREF(object);
        object
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

unsafe fn generic_set_attribute(object: *mut PyObject, name: *const c_char, value: *mut PyObject) -> c_int {
    unsafe {
        let py_name = PyUnicode_FromString(name);
        if py_name.is_null() {
            return -1;
        }
        let result = PyObject_GenericSetAttr(object, py_name, value);
        Py_DECREF(py_name);
        result
    }
}

unsafe fn set_uuid_slots(object: *mut PyObject, value: u128, is_safe: *mut PyObject) -> c_int {
    unsafe {
        let int_object = u128_to_pylong(value);
        if int_object.is_null() {
            return -1;
        }
        if generic_set_attribute(object, cstr!("int"), int_object) < 0 {
            Py_DECREF(int_object);
            return -1;
        }
        Py_DECREF(int_object);
        let safety_value = if is_safe.is_null() { SAFE_UUID_UNKNOWN } else { is_safe };
        generic_set_attribute(object, cstr!("is_safe"), safety_value)
    }
}

unsafe fn u128_to_pylong(value: u128) -> *mut PyObject {
    let bytes = value.to_be_bytes();
    unsafe { PyLong_FromUnsignedNativeBytes(bytes.as_ptr().cast(), bytes.len(), PY_ASNATIVEBYTES_BIG_ENDIAN) }
}

unsafe fn pylong_to_u128(object: *mut PyObject) -> Option<u128> {
    let mut bytes = [0u8; 16];
    let flags = PY_ASNATIVEBYTES_BIG_ENDIAN | PY_ASNATIVEBYTES_REJECT_NEGATIVE;
    let written = unsafe { PyLong_AsNativeBytes(object, bytes.as_mut_ptr().cast(), bytes.len() as Py_ssize_t, flags) };
    if written < 0 || written > bytes.len() as Py_ssize_t {
        unsafe { PyErr_Clear() };
        return None;
    }
    Some(u128::from_be_bytes(bytes))
}

unsafe fn uuid_int(object: *mut PyObject) -> Option<u128> {
    unsafe {
        let int_object = attribute(object, cstr!("int"));
        if int_object.is_null() {
            return None;
        }
        let value = pylong_to_u128(int_object);
        Py_DECREF(int_object);
        value
    }
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

fn apply_version(value: u128, version: u8) -> Option<u128> {
    if !(1..=5).contains(&version) {
        return None;
    }
    let with_variant = (value & !(0xc000u128 << 48)) | (0x8000u128 << 48);
    Some((with_variant & !(0xf000u128 << 64)) | ((version as u128) << 76))
}

fn set_random_version_and_variant(bytes: &mut [u8; 16], version: u8) {
    bytes[6] = (bytes[6] & 0x0f) | (version << 4);
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
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
        let mut bytes = [0u8; 16];
        if let Err(error) = getrandom::fill(&mut bytes) {
            PyErr_SetString(PyExc_OSError, std::ffi::CString::new(error.to_string()).unwrap().as_ptr());
            return ptr::null_mut();
        }
        set_random_version_and_variant(&mut bytes, 4);
        allocate_uuid(u128::from_be_bytes(bytes), SAFE_UUID_UNKNOWN)
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

unsafe fn namespace_bytes(namespace: *mut PyObject) -> Option<[u8; 16]> {
    unsafe {
        let value = uuid_int(namespace)?;
        Some(value.to_be_bytes())
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
        let Some(namespace_bytes) = namespace_bytes(namespace) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let Some(name_bytes) = name_bytes(name) else {
            return call_original(callable, args, nargsf, kwnames);
        };
        let mut digest = if VERSION == 3 {
            let mut hasher = Md5::new();
            hasher.update(namespace_bytes);
            hasher.update(name_bytes);
            let output = hasher.finalize();
            let mut digest = [0u8; 16];
            digest.copy_from_slice(&output[..16]);
            digest
        } else {
            let mut hasher = Sha1::new();
            hasher.update(namespace_bytes);
            hasher.update(name_bytes);
            let output = hasher.finalize();
            let mut digest = [0u8; 16];
            digest.copy_from_slice(&output[..16]);
            digest
        };
        set_random_version_and_variant(&mut digest, VERSION);
        allocate_uuid(u128::from_be_bytes(digest), SAFE_UUID_UNKNOWN)
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

unsafe extern "C" fn uuid1_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let positional_count = PyVectorcall_NARGS(nargsf);
        if positional_count > 2 || keyword_count(kwnames) > 2 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let mut node_object = if positional_count >= 1 { *args } else { ptr::null_mut() };
        let mut clock_seq_object = if positional_count >= 2 { *args.add(1) } else { ptr::null_mut() };
        for index in 0..keyword_count(kwnames) {
            let value = *args.add(positional_count as usize + index as usize);
            if keyword_matches(kwnames, index, cstr!("node")) {
                if !node_object.is_null() {
                    return call_original(callable, args, nargsf, kwnames);
                }
                node_object = value;
            } else if keyword_matches(kwnames, index, cstr!("clock_seq")) {
                if !clock_seq_object.is_null() {
                    return call_original(callable, args, nargsf, kwnames);
                }
                clock_seq_object = value;
            } else {
                return call_original(callable, args, nargsf, kwnames);
            }
        }

        let node = if node_object.is_null() || node_object == Py_None() {
            let getnode = attribute(UUID_MODULE, cstr!("getnode"));
            if getnode.is_null() {
                return ptr::null_mut();
            }
            let result = PyObject_CallNoArgs(getnode);
            Py_DECREF(getnode);
            if result.is_null() {
                return ptr::null_mut();
            }
            let Some(value) = pylong_to_u128(result) else {
                Py_DECREF(result);
                return call_original(callable, args, nargsf, kwnames);
            };
            Py_DECREF(result);
            value
        } else if let Some(value) = pylong_to_u128(node_object) {
            value
        } else {
            return call_original(callable, args, nargsf, kwnames);
        };
        if node >= (1u128 << 48) {
            return call_original(callable, args, nargsf, kwnames);
        }

        let clock_seq = if clock_seq_object.is_null() || clock_seq_object == Py_None() {
            let mut random_bytes = [0u8; 2];
            if getrandom::fill(&mut random_bytes).is_err() {
                return ptr::null_mut();
            }
            u16::from_be_bytes(random_bytes) as u128 & 0x3fff
        } else if let Some(value) = pylong_to_u128(clock_seq_object) {
            value & 0x3fff
        } else {
            return call_original(callable, args, nargsf, kwnames);
        };

        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration,
            Err(_) => return call_original(callable, args, nargsf, kwnames),
        };
        let mut timestamp = (now.as_nanos() / 100) + UUID_EPOCH_OFFSET;
        loop {
            let last = LAST_TIMESTAMP.load(Ordering::Relaxed) as u128;
            if timestamp <= last {
                timestamp = last + 1;
            }
            if LAST_TIMESTAMP
                .compare_exchange(last as u64, timestamp as u64, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }

        let time_low = timestamp & 0xffff_ffff;
        let time_mid = (timestamp >> 32) & 0xffff;
        let time_hi_version = (timestamp >> 48) & 0x0fff;
        let clock_seq_low = clock_seq & 0xff;
        let clock_seq_hi_variant = (clock_seq >> 8) & 0x3f;
        let value = (time_low << 96)
            | (time_mid << 80)
            | (time_hi_version << 64)
            | (clock_seq_hi_variant << 56)
            | (clock_seq_low << 48)
            | node;
        let value = apply_version(value, 1).unwrap();
        allocate_uuid(value, SAFE_UUID_UNKNOWN)
    }
}

unsafe fn unicode_to_string(object: *mut PyObject) -> Option<String> {
    unsafe {
        let utf8 = PyUnicode_AsUTF8AndSize(object, ptr::null_mut());
        if utf8.is_null() {
            PyErr_Clear();
            None
        } else {
            Some(std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned())
        }
    }
}

fn parse_hex_string(mut value: String) -> Option<u128> {
    value = value.replace("urn:", "").replace("uuid:", "");
    value = value.trim_matches(['{', '}']).replace('-', "");
    if value.len() != 32 {
        return None;
    }
    u128::from_str_radix(&value, 16).ok()
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
        let sequence = PySequence_Fast(object, cstr!("fields is not a sequence"));
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
        Some((values[0] << 96) | (values[1] << 80) | (values[2] << 64) | (values[3] << 56) | (values[4] << 48) | values[5])
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
        let mut hex_object = if positional_count >= 2 { *args.add(1) } else { Py_None() };
        let mut bytes_object = if positional_count >= 3 { *args.add(2) } else { Py_None() };
        let mut bytes_le_object = if positional_count >= 4 { *args.add(3) } else { Py_None() };
        let mut fields_object = if positional_count >= 5 { *args.add(4) } else { Py_None() };
        let mut int_object = if positional_count >= 6 { *args.add(5) } else { Py_None() };
        let mut version_object = if positional_count >= 7 { *args.add(6) } else { Py_None() };
        let mut is_safe_object = SAFE_UUID_UNKNOWN;

        for index in 0..keyword_count(kwnames) {
            let value = *args.add(positional_count as usize + index as usize);
            let target = if keyword_matches(kwnames, index, cstr!("hex")) {
                &mut hex_object
            } else if keyword_matches(kwnames, index, cstr!("bytes")) {
                &mut bytes_object
            } else if keyword_matches(kwnames, index, cstr!("bytes_le")) {
                &mut bytes_le_object
            } else if keyword_matches(kwnames, index, cstr!("fields")) {
                &mut fields_object
            } else if keyword_matches(kwnames, index, cstr!("int")) {
                &mut int_object
            } else if keyword_matches(kwnames, index, cstr!("version")) {
                &mut version_object
            } else if keyword_matches(kwnames, index, cstr!("is_safe")) {
                &mut is_safe_object
            } else {
                return call_original(callable, args, nargsf, kwnames);
            };
            if *target != Py_None() {
                return call_original(callable, args, nargsf, kwnames);
            }
            *target = value;
        }

        let sources = [hex_object, bytes_object, bytes_le_object, fields_object, int_object];
        if sources.iter().filter(|source| **source != Py_None()).count() != 1 {
            return call_original(callable, args, nargsf, kwnames);
        }

        let mut value = if hex_object != Py_None() {
            let Some(string) = unicode_to_string(hex_object) else {
                return call_original(callable, args, nargsf, kwnames);
            };
            let Some(value) = parse_hex_string(string) else {
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
            let Some(versioned_value) = apply_version(value, version) else {
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

unsafe fn unary_self(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> Option<*mut PyObject> {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 1 || keyword_count(kwnames) != 0 {
            call_original(callable, args, nargsf, kwnames);
            None
        } else {
            Some(*args)
        }
    }
}

unsafe extern "C" fn uuid_str_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let hex = format!("{value:032x}");
        let string = format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]);
        PyUnicode_FromStringAndSize(string.as_ptr().cast(), string.len() as Py_ssize_t)
    }
}

unsafe extern "C" fn uuid_hex_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let string = format!("{value:032x}");
        PyUnicode_FromStringAndSize(string.as_ptr().cast(), string.len() as Py_ssize_t)
    }
}

unsafe extern "C" fn uuid_repr_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let class = PyObject_Type(self_object);
        if class.is_null() { return ptr::null_mut(); }
        let name = attribute(class, cstr!("__name__"));
        Py_DECREF(class);
        if name.is_null() { return ptr::null_mut(); }
        let string = uuid_str_vectorcall(callable, args, nargsf, kwnames);
        if string.is_null() { Py_DECREF(name); return ptr::null_mut(); }
        let result = PyUnicode_FromFormat(cstr!("%U('%U')"), name, string);
        Py_DECREF(name);
        Py_DECREF(string);
        result
    }
}

unsafe extern "C" fn uuid_int_method_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let int_object = attribute(self_object, cstr!("int"));
        if int_object.is_null() { call_original(callable, args, nargsf, kwnames) } else { int_object }
    }
}

unsafe extern "C" fn uuid_hash_method_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let int_object = attribute(self_object, cstr!("int"));
        if int_object.is_null() { return call_original(callable, args, nargsf, kwnames); }
        let hash = PyObject_Hash(int_object);
        Py_DECREF(int_object);
        if hash == -1 && !PyErr_Occurred().is_null() { return ptr::null_mut(); }
        PyLong_FromSsize_t(hash)
    }
}

unsafe extern "C" fn uuid_setattr_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 3 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        PyErr_SetString(PyExc_TypeError, cstr!("UUID objects are immutable"));
        ptr::null_mut()
    }
}

unsafe fn rich_compare<const OP: u8>(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 2 || keyword_count(kwnames) != 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
        let self_object = *args;
        let other_object = *args.add(1);
        let instance_check = PyObject_IsInstance(other_object, UUID_TYPE);
        if instance_check < 0 { return ptr::null_mut(); }
        if instance_check == 0 { return not_implemented(); }
        let Some(left) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let Some(right) = uuid_int(other_object) else { return call_original(callable, args, nargsf, kwnames); };
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

unsafe extern "C" fn uuid_eq_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { rich_compare::<0>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_lt_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { rich_compare::<1>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_gt_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { rich_compare::<2>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_le_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { rich_compare::<3>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_ge_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { rich_compare::<4>(callable, args, nargsf, kwnames) } }

unsafe extern "C" fn uuid_bytes_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let bytes = value.to_be_bytes();
        PyBytes_FromStringAndSize(bytes.as_ptr().cast(), bytes.len() as Py_ssize_t)
    }
}

unsafe extern "C" fn uuid_bytes_le_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let mut bytes = value.to_be_bytes();
        bytes[0..4].reverse();
        bytes[4..6].reverse();
        bytes[6..8].reverse();
        PyBytes_FromStringAndSize(bytes.as_ptr().cast(), bytes.len() as Py_ssize_t)
    }
}

unsafe fn pylong_from_u128_lossless(value: u128) -> *mut PyObject { unsafe { u128_to_pylong(value) } }

unsafe fn tuple_set_u128(tuple: *mut PyObject, index: Py_ssize_t, value: u128) -> c_int {
    unsafe {
        let object = pylong_from_u128_lossless(value);
        if object.is_null() { return -1; }
        PyTuple_SetItem(tuple, index, object)
    }
}

unsafe extern "C" fn uuid_fields_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let tuple = PyTuple_New(6);
        if tuple.is_null() { return ptr::null_mut(); }
        let values = [value >> 96, (value >> 80) & 0xffff, (value >> 64) & 0xffff, (value >> 56) & 0xff, (value >> 48) & 0xff, value & 0xffffffffffff];
        for (index, item) in values.iter().enumerate() {
            if tuple_set_u128(tuple, index as Py_ssize_t, *item) < 0 { Py_DECREF(tuple); return ptr::null_mut(); }
        }
        tuple
    }
}

unsafe extern "C" fn uuid_field_value_vectorcall<const FIELD: u8>(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        let field_value = match FIELD {
            0 => value >> 96,
            1 => (value >> 80) & 0xffff,
            2 => (value >> 64) & 0xffff,
            3 => (value >> 56) & 0xff,
            4 => (value >> 48) & 0xff,
            5 => ((value >> 64) & 0x0fff) << 48 | (((value >> 80) & 0xffff) << 32) | (value >> 96),
            6 => (((value >> 56) & 0x3f) << 8) | ((value >> 48) & 0xff),
            _ => value & 0xffffffffffff,
        };
        pylong_from_u128_lossless(field_value)
    }
}

unsafe extern "C" fn uuid_time_low_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<0>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_time_mid_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<1>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_time_hi_version_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<2>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_clock_seq_hi_variant_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<3>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_clock_seq_low_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<4>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_time_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<5>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_clock_seq_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<6>(callable, args, nargsf, kwnames) } }
unsafe extern "C" fn uuid_node_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject { unsafe { uuid_field_value_vectorcall::<7>(callable, args, nargsf, kwnames) } }

unsafe extern "C" fn uuid_urn_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let string = uuid_str_vectorcall(callable, args, nargsf, kwnames);
        if string.is_null() { return ptr::null_mut(); }
        let result = PyUnicode_FromFormat(cstr!("urn:uuid:%U"), string);
        Py_DECREF(string);
        result
    }
}

unsafe extern "C" fn uuid_variant_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
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

unsafe extern "C" fn uuid_version_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe {
        let Some(self_object) = unary_self(callable, args, nargsf, kwnames) else { return ptr::null_mut(); };
        let Some(value) = uuid_int(self_object) else { return call_original(callable, args, nargsf, kwnames); };
        if value & (0x8000u128 << 48) != 0 && value & (0x4000u128 << 48) == 0 {
            PyLong_FromLong(((value >> 76) & 0xf) as c_long)
        } else {
            none()
        }
    }
}

unsafe extern "C" fn uuid_getstate_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe { call_original(callable, args, nargsf, kwnames) }
}

unsafe extern "C" fn uuid_setstate_vectorcall(callable: *mut PyObject, args: *const *mut PyObject, nargsf: usize, kwnames: *mut PyObject) -> *mut PyObject {
    unsafe { call_original(callable, args, nargsf, kwnames) }
}

unsafe fn patch_function(function: *mut PyObject, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let current = match PyVectorcall_Function(function) {
            Some(current) => current,
            None => {
                PyErr_SetString(PyExc_RuntimeError, cstr!("uuideal: function has no vectorcall slot"));
                return -1;
            }
        };
        if PyObject_HasAttrString(function, CAPSULE_NAME) != 0 {
            return 0;
        }
        let capsule = PyCapsule_New(std::mem::transmute::<vectorcallfunc, *mut c_void>(current), CAPSULE_NAME, None);
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

unsafe fn patch_module_function(module: *mut PyObject, name: *const c_char, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let function = attribute(module, name);
        if function.is_null() { return -1; }
        let result = patch_function(function, vectorcall);
        Py_DECREF(function);
        result
    }
}

unsafe fn restore_module_function(module: *mut PyObject, name: *const c_char) -> c_int {
    unsafe {
        let function = attribute(module, name);
        if function.is_null() { PyErr_Clear(); return 0; }
        let result = restore_function(function);
        Py_DECREF(function);
        result
    }
}

unsafe fn patch_uuid_method(name: *const c_char, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let function = attribute(UUID_TYPE, name);
        if function.is_null() { return -1; }
        let result = patch_function(function, vectorcall);
        Py_DECREF(function);
        result
    }
}

unsafe fn restore_uuid_method(name: *const c_char) -> c_int {
    unsafe {
        let function = attribute(UUID_TYPE, name);
        if function.is_null() { PyErr_Clear(); return 0; }
        let result = restore_function(function);
        Py_DECREF(function);
        result
    }
}

unsafe fn patch_uuid_property(name: *const c_char, vectorcall: vectorcallfunc) -> c_int {
    unsafe {
        let property = attribute(UUID_TYPE, name);
        if property.is_null() { return -1; }
        let getter = attribute(property, cstr!("fget"));
        Py_DECREF(property);
        if getter.is_null() { return -1; }
        let result = patch_function(getter, vectorcall);
        Py_DECREF(getter);
        result
    }
}

unsafe fn restore_uuid_property(name: *const c_char) -> c_int {
    unsafe {
        let property = attribute(UUID_TYPE, name);
        if property.is_null() { PyErr_Clear(); return 0; }
        let getter = attribute(property, cstr!("fget"));
        Py_DECREF(property);
        if getter.is_null() { PyErr_Clear(); return 0; }
        let result = restore_function(getter);
        Py_DECREF(getter);
        result
    }
}

unsafe fn load_uuid_references() -> c_int {
    unsafe {
        if !UUID_MODULE.is_null() { return 0; }
        let module = PyImport_ImportModule(cstr!("uuid"));
        if module.is_null() { return -1; }
        UUID_MODULE = module;
        UUID_TYPE = attribute(module, cstr!("UUID"));
        SAFE_UUID_UNKNOWN = attribute(attribute(module, cstr!("SafeUUID")), cstr!("unknown"));
        RESERVED_NCS_VALUE = attribute(module, cstr!("RESERVED_NCS"));
        RFC_4122_VALUE = attribute(module, cstr!("RFC_4122"));
        RESERVED_MICROSOFT_VALUE = attribute(module, cstr!("RESERVED_MICROSOFT"));
        RESERVED_FUTURE_VALUE = attribute(module, cstr!("RESERVED_FUTURE"));
        if UUID_TYPE.is_null() || SAFE_UUID_UNKNOWN.is_null() || RESERVED_NCS_VALUE.is_null() || RFC_4122_VALUE.is_null() || RESERVED_MICROSOFT_VALUE.is_null() || RESERVED_FUTURE_VALUE.is_null() {
            return -1;
        }
        0
    }
}

unsafe fn apply_all_patches() -> c_int {
    unsafe {
        if load_uuid_references() < 0 { return -1; }
        let module_patches = [
            (cstr!("uuid1"), uuid1_vectorcall as vectorcallfunc),
            (cstr!("uuid3"), uuid3_vectorcall as vectorcallfunc),
            (cstr!("uuid4"), uuid4_vectorcall as vectorcallfunc),
            (cstr!("uuid5"), uuid5_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in module_patches { if patch_module_function(UUID_MODULE, name, vectorcall) < 0 { return -1; } }
        let method_patches = [
            (cstr!("__init__"), uuid_init_vectorcall as vectorcallfunc),
            (cstr!("__getstate__"), uuid_getstate_vectorcall as vectorcallfunc),
            (cstr!("__setstate__"), uuid_setstate_vectorcall as vectorcallfunc),
            (cstr!("__eq__"), uuid_eq_vectorcall as vectorcallfunc),
            (cstr!("__lt__"), uuid_lt_vectorcall as vectorcallfunc),
            (cstr!("__gt__"), uuid_gt_vectorcall as vectorcallfunc),
            (cstr!("__le__"), uuid_le_vectorcall as vectorcallfunc),
            (cstr!("__ge__"), uuid_ge_vectorcall as vectorcallfunc),
            (cstr!("__hash__"), uuid_hash_method_vectorcall as vectorcallfunc),
            (cstr!("__int__"), uuid_int_method_vectorcall as vectorcallfunc),
            (cstr!("__repr__"), uuid_repr_vectorcall as vectorcallfunc),
            (cstr!("__setattr__"), uuid_setattr_vectorcall as vectorcallfunc),
            (cstr!("__str__"), uuid_str_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in method_patches { if patch_uuid_method(name, vectorcall) < 0 { return -1; } }
        let property_patches = [
            (cstr!("bytes"), uuid_bytes_vectorcall as vectorcallfunc),
            (cstr!("bytes_le"), uuid_bytes_le_vectorcall as vectorcallfunc),
            (cstr!("fields"), uuid_fields_vectorcall as vectorcallfunc),
            (cstr!("time_low"), uuid_time_low_vectorcall as vectorcallfunc),
            (cstr!("time_mid"), uuid_time_mid_vectorcall as vectorcallfunc),
            (cstr!("time_hi_version"), uuid_time_hi_version_vectorcall as vectorcallfunc),
            (cstr!("clock_seq_hi_variant"), uuid_clock_seq_hi_variant_vectorcall as vectorcallfunc),
            (cstr!("clock_seq_low"), uuid_clock_seq_low_vectorcall as vectorcallfunc),
            (cstr!("time"), uuid_time_vectorcall as vectorcallfunc),
            (cstr!("clock_seq"), uuid_clock_seq_vectorcall as vectorcallfunc),
            (cstr!("node"), uuid_node_vectorcall as vectorcallfunc),
            (cstr!("hex"), uuid_hex_vectorcall as vectorcallfunc),
            (cstr!("urn"), uuid_urn_vectorcall as vectorcallfunc),
            (cstr!("variant"), uuid_variant_vectorcall as vectorcallfunc),
            (cstr!("version"), uuid_version_vectorcall as vectorcallfunc),
        ];
        for (name, vectorcall) in property_patches { if patch_uuid_property(name, vectorcall) < 0 { return -1; } }
        PyType_Modified(UUID_TYPE.cast::<PyTypeObject>());
        0
    }
}

unsafe fn restore_all_patches() -> c_int {
    unsafe {
        if UUID_MODULE.is_null() { return 0; }
        for name in [cstr!("uuid1"), cstr!("uuid3"), cstr!("uuid4"), cstr!("uuid5")] { if restore_module_function(UUID_MODULE, name) < 0 { return -1; } }
        for name in [cstr!("__init__"), cstr!("__getstate__"), cstr!("__setstate__"), cstr!("__eq__"), cstr!("__lt__"), cstr!("__gt__"), cstr!("__le__"), cstr!("__ge__"), cstr!("__hash__"), cstr!("__int__"), cstr!("__repr__"), cstr!("__setattr__"), cstr!("__str__")] { if restore_uuid_method(name) < 0 { return -1; } }
        for name in [cstr!("bytes"), cstr!("bytes_le"), cstr!("fields"), cstr!("time_low"), cstr!("time_mid"), cstr!("time_hi_version"), cstr!("clock_seq_hi_variant"), cstr!("clock_seq_low"), cstr!("time"), cstr!("clock_seq"), cstr!("node"), cstr!("hex"), cstr!("urn"), cstr!("variant"), cstr!("version")] { if restore_uuid_property(name) < 0 { return -1; } }
        PyType_Modified(UUID_TYPE.cast::<PyTypeObject>());
        0
    }
}

unsafe extern "C" fn py_enable(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe {
        if !PATCHED.load(Ordering::SeqCst) {
            if apply_all_patches() < 0 { return ptr::null_mut(); }
            PATCHED.store(true, Ordering::SeqCst);
        }
        none()
    }
}

unsafe extern "C" fn py_disable(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe {
        if PATCHED.load(Ordering::SeqCst) {
            if restore_all_patches() < 0 { return ptr::null_mut(); }
            PATCHED.store(false, Ordering::SeqCst);
        }
        none()
    }
}

unsafe extern "C" fn py_is_enabled(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe { PyBool_FromLong(PATCHED.load(Ordering::SeqCst) as c_long) }
}

static mut METHODS: [PyMethodDef; 6] = [PyMethodDef::zeroed(); 6];

unsafe fn init_methods() {
    unsafe {
        METHODS[0] = PyMethodDef { ml_name: cstr!("enable"), ml_meth: PyMethodDefPointer { PyCFunction: py_enable }, ml_flags: METH_NOARGS, ml_doc: cstr!("Enable uuid vectorcall patches.") };
        METHODS[1] = PyMethodDef { ml_name: cstr!("install"), ml_meth: PyMethodDefPointer { PyCFunction: py_enable }, ml_flags: METH_NOARGS, ml_doc: cstr!("Enable uuid vectorcall patches.") };
        METHODS[2] = PyMethodDef { ml_name: cstr!("disable"), ml_meth: PyMethodDefPointer { PyCFunction: py_disable }, ml_flags: METH_NOARGS, ml_doc: cstr!("Disable uuid vectorcall patches.") };
        METHODS[3] = PyMethodDef { ml_name: cstr!("uninstall"), ml_meth: PyMethodDefPointer { PyCFunction: py_disable }, ml_flags: METH_NOARGS, ml_doc: cstr!("Disable uuid vectorcall patches.") };
        METHODS[4] = PyMethodDef { ml_name: cstr!("is_enabled"), ml_meth: PyMethodDefPointer { PyCFunction: py_is_enabled }, ml_flags: METH_NOARGS, ml_doc: cstr!("Return whether uuid vectorcall patches are enabled.") };
        METHODS[5] = PyMethodDef::zeroed();
    }
}

static mut MODULE_DEF: PyModuleDef = PyModuleDef {
    m_base: PyModuleDef_HEAD_INIT,
    m_name: cstr!("uuideal._uuideal"),
    m_doc: cstr!("Vectorcall patches for stdlib uuid."),
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
