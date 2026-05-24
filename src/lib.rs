#![allow(non_snake_case)]

use core::ffi::{c_char, c_int, c_long, c_ulonglong, c_void};
use pyo3_ffi::*;
use std::ffi::CString;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};


const CAPSULE_NAME: *const c_char = c"uuideal._original_vectorcall".as_ptr();

const GETNODE_MODE_TRUSTED: u8 = 0;
const GETNODE_MODE_UNKNOWN: u8 = 1;

static PATCHED: AtomicBool = AtomicBool::new(false);
static AT_FORK_REGISTERED: AtomicBool = AtomicBool::new(false);
static DEFAULT_NODE_READY: AtomicBool = AtomicBool::new(false);
static DEFAULT_NODE: AtomicU64 = AtomicU64::new(0);
static GETNODE_MODE: AtomicU8 = AtomicU8::new(GETNODE_MODE_TRUSTED);

static mut UUID_MODULE: *mut PyObject = ptr::null_mut();
static mut UUID_DICT: *mut PyObject = ptr::null_mut();
static mut ORIGINAL_GETNODE: *mut PyObject = ptr::null_mut();
static mut TRUSTED_GETNODE: *mut PyObject = ptr::null_mut();
static mut UUID_DICT_WATCHER_ID: c_int = -1;
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
static mut INTERNED_NODE: *mut PyObject = ptr::null_mut();
static mut INTERNED_GETNODE: *mut PyObject = ptr::null_mut();
static mut INTERNED_GENERATE_TIME_SAFE: *mut PyObject = ptr::null_mut();
static mut SAFE_UUID_SAFE: *mut PyObject = ptr::null_mut();
static mut SAFE_UUID_UNSAFE: *mut PyObject = ptr::null_mut();
static mut GENERATE_TIME_SAFE: *mut PyObject = ptr::null_mut();

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

#[repr(C)]
struct PyBytesObjectLayout {
    ob_base: PyVarObject,
    ob_shash: Py_hash_t,
    ob_sval: [c_char; 1],
}

const PYASCII_STATE_ASCII: u32 = 1 << 6;

type PyDictWatchCallback = Option<
    unsafe extern "C" fn(c_int, *mut PyObject, *mut PyObject, *mut PyObject) -> c_int,
>;

#[cfg_attr(windows, link(name = "pythonXY"))]
extern "C" {
    fn PyFunction_SetVectorcall(callable: *mut PyObject, vectorcall: vectorcallfunc);
    fn PyVectorcall_Function(callable: *mut PyObject) -> Option<vectorcallfunc>;
    fn _PyLong_New(size: Py_ssize_t) -> *mut PyLongObject;
    fn PyLong_AsUnsignedLongLongMask(object: *mut PyObject) -> c_ulonglong;
    fn PyUnicode_New(size: Py_ssize_t, maxchar: u32) -> *mut PyObject;
    fn PyDict_AddWatcher(callback: PyDictWatchCallback) -> c_int;
    fn PyDict_Watch(watcher_id: c_int, dict: *mut PyObject) -> c_int;
}

#[inline(always)]
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

#[inline(always)]
unsafe fn none() -> *mut PyObject {
    unsafe { Py_None() }
}

#[inline(always)]
unsafe fn not_implemented() -> *mut PyObject {
    unsafe { Py_NotImplemented() }
}

#[inline(always)]
unsafe fn py_bool(value: bool) -> *mut PyObject {
    unsafe {
        if value { Py_True() } else { Py_False() }
    }
}

unsafe fn attribute(object: *mut PyObject, name: *const c_char) -> *mut PyObject {
    unsafe { PyObject_GetAttrString(object, name) }
}

unsafe fn set_slot_by_offset(object: *mut PyObject, offset: Py_ssize_t, value: *mut PyObject) {
    unsafe {
        let slot = slot_pointer(object, offset);
        let previous = *slot;
        Py_INCREF(value);
        *slot = value;
        if !previous.is_null() {
            Py_DECREF(previous);
        }
    }
}

#[inline(always)]
unsafe fn slot_object(object: *mut PyObject, offset: Py_ssize_t) -> *mut PyObject {
    unsafe { *slot_pointer(object, offset) }
}

#[inline(always)]
unsafe fn slot_pointer(object: *mut PyObject, offset: Py_ssize_t) -> *mut *mut PyObject {
    unsafe { object.cast::<u8>().offset(offset).cast::<*mut PyObject>() }
}

#[inline(always)]
unsafe fn fill_uuid_slots_owned_int(
    object: *mut PyObject,
    int_object: *mut PyObject,
    safety_value: *mut PyObject,
) {
    unsafe {
        let int_slot = slot_pointer(object, INT_SLOT_OFFSET);
        let is_safe_slot = slot_pointer(object, IS_SAFE_SLOT_OFFSET);
        let previous_int = *int_slot;
        let previous_is_safe = *is_safe_slot;

        Py_INCREF(safety_value);
        *int_slot = int_object;
        *is_safe_slot = safety_value;

        if !previous_int.is_null() {
            Py_DECREF(previous_int);
        }
        if !previous_is_safe.is_null() {
            Py_DECREF(previous_is_safe);
        }
    }
}

#[inline(always)]
unsafe fn fill_uuid_slots_borrowed_int(
    object: *mut PyObject,
    int_object: *mut PyObject,
    safety_value: *mut PyObject,
) {
    unsafe {
        Py_INCREF(int_object);
        fill_uuid_slots_owned_int(object, int_object, safety_value);
    }
}

unsafe fn invalidate_default_node_cache() {
    DEFAULT_NODE_READY.store(false, Ordering::Relaxed);
}

unsafe fn clear_trusted_getnode() {
    unsafe {
        if !TRUSTED_GETNODE.is_null() {
            Py_DECREF(TRUSTED_GETNODE);
            TRUSTED_GETNODE = ptr::null_mut();
        }
    }
}

unsafe fn clear_generate_time_safe() {
    unsafe {
        if !GENERATE_TIME_SAFE.is_null() {
            Py_DECREF(GENERATE_TIME_SAFE);
            GENERATE_TIME_SAFE = ptr::null_mut();
        }
    }
}

unsafe fn set_generate_time_safe_borrowed(value: *mut PyObject) {
    unsafe {
        clear_generate_time_safe();
        if !value.is_null() && value != Py_None() {
            Py_INCREF(value);
            GENERATE_TIME_SAFE = value;
        }
    }
}

unsafe fn trust_getnode_borrowed(getnode: *mut PyObject) {
    unsafe {
        if TRUSTED_GETNODE == getnode {
            return;
        }
        clear_trusted_getnode();
        if !getnode.is_null() {
            Py_INCREF(getnode);
            TRUSTED_GETNODE = getnode;
        }
    }
}

unsafe fn watched_key_matches(key: *mut PyObject, watched: *mut PyObject) -> bool {
    unsafe {
        if key.is_null() || watched.is_null() {
            return false;
        }
        if key == watched {
            return true;
        }
        PyObject_RichCompareBool(key, watched, Py_EQ) == 1
    }
}

unsafe extern "C" fn uuid_dict_watcher(
    _event: c_int,
    _dict: *mut PyObject,
    key: *mut PyObject,
    new_value: *mut PyObject,
) -> c_int {
    unsafe {
        if key.is_null() {
            invalidate_default_node_cache();
            clear_trusted_getnode();
            GETNODE_MODE.store(GETNODE_MODE_UNKNOWN, Ordering::Relaxed);
            return 0;
        }

        if watched_key_matches(key, INTERNED_NODE) {
            invalidate_default_node_cache();
            return 0;
        }

        if watched_key_matches(key, INTERNED_GETNODE) {
            invalidate_default_node_cache();
            clear_trusted_getnode();

            if !new_value.is_null() && new_value == ORIGINAL_GETNODE {
                GETNODE_MODE.store(GETNODE_MODE_TRUSTED, Ordering::Relaxed);
            } else {
                GETNODE_MODE.store(GETNODE_MODE_UNKNOWN, Ordering::Relaxed);
            }

            return 0;
        }

        if watched_key_matches(key, INTERNED_GENERATE_TIME_SAFE) {
            set_generate_time_safe_borrowed(new_value);
            return 0;
        }

        0
    }
}

unsafe fn install_uuid_dict_watcher() -> c_int {
    unsafe {
        if UUID_DICT_WATCHER_ID >= 0 {
            return 0;
        }

        let watcher_id = PyDict_AddWatcher(Some(uuid_dict_watcher));
        if watcher_id < 0 {
            return -1;
        }

        if PyDict_Watch(watcher_id, UUID_DICT) < 0 {
            return -1;
        }

        UUID_DICT_WATCHER_ID = watcher_id;
        0
    }
}

#[inline(always)]
unsafe fn init_uuid_slots_from_value(object: *mut PyObject, value: u128) -> c_int {
    unsafe {
        let int_object = u128_to_pylong(value);
        if int_object.is_null() {
            return -1;
        }
        fill_uuid_slots_owned_int(object, int_object, SAFE_UUID_UNKNOWN);
        0
    }
}

unsafe fn set_uuid_slots(object: *mut PyObject, value: u128, is_safe: *mut PyObject) -> c_int {
    unsafe {
        if is_safe.is_null() {
            return init_uuid_slots_from_value(object, value);
        }
        let int_object = u128_to_pylong(value);
        if int_object.is_null() {
            return -1;
        }
        fill_uuid_slots_owned_int(object, int_object, is_safe);
        0
    }
}

unsafe fn set_uuid_slots_from_pylong(
    object: *mut PyObject,
    int_object: *mut PyObject,
    is_safe: *mut PyObject,
) -> c_int {
    unsafe {
        let safety_value = if is_safe.is_null() {
            SAFE_UUID_UNKNOWN
        } else {
            is_safe
        };
        fill_uuid_slots_borrowed_int(object, int_object, safety_value);
        0
    }
}

const PYLONG_SHIFT: u32 = 30;
const PYLONG_MASK: u128 = (1u128 << PYLONG_SHIFT) - 1;

#[repr(C)]
struct PyLongInternals {
    ob_refcnt: Py_ssize_t,
    ob_type: *mut PyTypeObject,
    lv_tag: usize,
    ob_digit: [u32; 0],
}

unsafe fn u128_to_pylong(value: u128) -> *mut PyObject {
    unsafe {
        let high_digit = (value >> (PYLONG_SHIFT * 4)) as u32;
        if high_digit != 0 {
            let object = _PyLong_New(5);
            if object.is_null() {
                return ptr::null_mut();
            }

            let long = object.cast::<PyLongInternals>();

            (*long).lv_tag = 5 << 3;

            let digits = (*long).ob_digit.as_mut_ptr();
            *digits = (value & PYLONG_MASK) as u32;
            *digits.add(1) = ((value >> PYLONG_SHIFT) & PYLONG_MASK) as u32;
            *digits.add(2) = ((value >> (PYLONG_SHIFT * 2)) & PYLONG_MASK) as u32;
            *digits.add(3) = ((value >> (PYLONG_SHIFT * 3)) & PYLONG_MASK) as u32;
            *digits.add(4) = high_digit;

            return object.cast::<PyObject>();
        }

        let ndigits = if value == 0 {
            0
        } else if value < (1u128 << PYLONG_SHIFT) {
            1
        } else if value < (1u128 << (PYLONG_SHIFT * 2)) {
            2
        } else if value < (1u128 << (PYLONG_SHIFT * 3)) {
            3
        } else if value < (1u128 << (PYLONG_SHIFT * 4)) {
            4
        } else {
            5
        };

        let object = _PyLong_New(ndigits as Py_ssize_t);
        if object.is_null() {
            return ptr::null_mut();
        }

        let long = object.cast::<PyLongInternals>();

        (*long).lv_tag = if ndigits == 0 { 1 } else { ndigits << 3 };

        let digits = (*long).ob_digit.as_mut_ptr();
        let mut remaining = value;
        for index in 0..ndigits {
            *digits.add(index) = (remaining & PYLONG_MASK) as u32;
            remaining >>= PYLONG_SHIFT;
        }

        object.cast::<PyObject>()
    }
}

unsafe fn pylong_to_u128(object: *mut PyObject) -> Option<u128> {
    unsafe {
        let long = object.cast::<PyLongInternals>();

        let ndigits = {
            let tag = (*long).lv_tag;
            match tag & 3 {
                2 => return None,    // negative
                1 => return Some(0), // zero
                _ => {}
            }
            tag >> 3
        };

        let digits = (*long).ob_digit.as_ptr();
        Some(match ndigits {
            0 => 0,
            1 => *digits as u128,
            2 => (*digits.add(1) as u128) << PYLONG_SHIFT
                | *digits as u128,
            3 => (*digits.add(2) as u128) << (PYLONG_SHIFT * 2)
                | (*digits.add(1) as u128) << PYLONG_SHIFT
                | *digits as u128,
            4 => (*digits.add(3) as u128) << (PYLONG_SHIFT * 3)
                | (*digits.add(2) as u128) << (PYLONG_SHIFT * 2)
                | (*digits.add(1) as u128) << PYLONG_SHIFT
                | *digits as u128,
            5 => {
                let high = *digits.add(4);
                if high > 0xff {
                    return None;
                }
                (high as u128) << (PYLONG_SHIFT * 4)
                    | (*digits.add(3) as u128) << (PYLONG_SHIFT * 3)
                    | (*digits.add(2) as u128) << (PYLONG_SHIFT * 2)
                    | (*digits.add(1) as u128) << PYLONG_SHIFT
                    | *digits as u128
            }
            _ => return None,
        })
    }
}

#[inline(always)]
unsafe fn pylong_cmp_unsigned(a: *mut PyObject, b: *mut PyObject) -> Option<std::cmp::Ordering> {
    unsafe {
        let la = a.cast::<PyLongInternals>();
        let lb = b.cast::<PyLongInternals>();

        let tag_a = (*la).lv_tag;
        let tag_b = (*lb).lv_tag;

        if (tag_a & 3) == 2 || (tag_b & 3) == 2 {
            return None;
        }

        let na = if (tag_a & 3) == 1 { 0 } else { tag_a >> 3 };
        let nb = if (tag_b & 3) == 1 { 0 } else { tag_b >> 3 };

        if na != nb {
            return Some(na.cmp(&nb));
        }

        if na == 0 || na > 5 {
            return if na == 0 { Some(std::cmp::Ordering::Equal) } else { None };
        }

        let da = (*la).ob_digit.as_ptr();
        let db = (*lb).ob_digit.as_ptr();

        let mut i = na;
        while i > 0 {
            i -= 1;
            let ad = *da.add(i);
            let bd = *db.add(i);
            if ad != bd {
                return Some(ad.cmp(&bd));
            }
        }
        Some(std::cmp::Ordering::Equal)
    }
}

#[inline(always)]
unsafe fn uuid_int_from_slot(object: *mut PyObject) -> Option<u128> {
    unsafe {
        let int_object = slot_object(object, INT_SLOT_OFFSET);
        if int_object.is_null() {
            return None;
        }
        pylong_to_u128(int_object)
    }
}

#[inline(always)]
unsafe fn uuid_int(object: *mut PyObject) -> Option<u128> {
    unsafe { uuid_int_from_slot(object) }
}

unsafe fn ascii_unicode_bytes(object: *mut PyObject) -> Option<&'static [u8]> {
    unsafe {
        let unicode = object.cast::<PyASCIIObjectLayout>();
        if (*unicode).state & PYASCII_STATE_ASCII == 0 {
            return None;
        }
        let size = (*unicode).length;
        if size < 0 {
            return None;
        }
        let data = object
            .cast::<u8>()
            .add(std::mem::size_of::<PyASCIIObjectLayout>());
        Some(std::slice::from_raw_parts(data, size as usize))
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

fn parse_uuid_ascii(bytes: &[u8]) -> Option<u128> {
    uuid::Uuid::try_parse_ascii(bytes)
        .ok()
        .map(|uuid| uuid.as_u128())
}

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

unsafe fn parse_uuid_hex_pyunicode(object: *mut PyObject) -> Option<u128> {
    unsafe {
        if PyUnicode_Check(object) == 0 {
            return None;
        }
        if let Some(bytes) = ascii_unicode_bytes(object) {
            if let Some(value) = parse_uuid_ascii(bytes) {
                return Some(value);
            }
        }
        let mut size: Py_ssize_t = 0;
        let ptr = PyUnicode_AsUTF8AndSize(object, &mut size);
        if ptr.is_null() {
            PyErr_Clear();
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), size as usize);
        if let Some(value) = parse_uuid_ascii(bytes) {
            return Some(value);
        }
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return None,
        };
        uuid::Uuid::parse_str(text).ok().map(|uuid| uuid.as_u128())
    }
}

unsafe fn uuid_bytes_object(value: u128) -> *mut PyObject {
    let bytes = value.to_be_bytes();
    unsafe { PyBytes_FromStringAndSize(bytes.as_ptr().cast(), bytes.len() as Py_ssize_t) }
}

unsafe fn uuid_bytes_le_object(value: u128) -> *mut PyObject {
    unsafe {
        let object = PyBytes_FromStringAndSize(ptr::null(), 16);
        if object.is_null() {
            return ptr::null_mut();
        }

        let bytes = (*object.cast::<PyBytesObjectLayout>())
            .ob_sval
            .as_mut_ptr()
            .cast::<u8>();

        ptr::write_unaligned(bytes.cast::<u32>(), ((value >> 96) as u32).to_le());
        ptr::write_unaligned(bytes.add(4).cast::<u16>(), ((value >> 80) as u16).to_le());
        ptr::write_unaligned(bytes.add(6).cast::<u16>(), ((value >> 64) as u16).to_le());
        ptr::write_unaligned(bytes.add(8).cast::<u64>(), (value as u64).to_be());

        object
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
            if object.is_null() {
                Py_DECREF(tuple);
                return ptr::null_mut();
            }
            PyTuple_SET_ITEM(tuple, index as Py_ssize_t, object);
        }
        tuple
    }
}

#[inline(always)]
fn uuid_field_value<const FIELD: u8>(value: u128) -> u128 {
    match FIELD {
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

#[inline(always)]
unsafe fn uuid_field_object<const FIELD: u8>(value: u128) -> *mut PyObject {
    unsafe { small_unsigned_long(uuid_field_value::<FIELD>(value)) }
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
    let version = match version {
        1 => uuid::Version::Mac,
        2 => uuid::Version::Dce,
        3 => uuid::Version::Md5,
        4 => uuid::Version::Random,
        5 => uuid::Version::Sha1,
        6 => uuid::Version::SortMac,
        7 => uuid::Version::SortRand,
        8 => uuid::Version::Custom,
        _ => return None,
    };
    let mut builder = uuid::Builder::from_u128(value);
    builder.set_variant(uuid::Variant::RFC4122);
    builder.set_version(version);
    Some(builder.into_uuid().as_u128())
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

unsafe fn allocate_uuid(value: u128) -> *mut PyObject {
    unsafe {
        let object = PyType_GenericAlloc(UUID_TYPE.cast::<PyTypeObject>(), 0);
        if object.is_null() {
            return ptr::null_mut();
        }
        let int_object = u128_to_pylong(value);
        if int_object.is_null() {
            Py_DECREF(object);
            return ptr::null_mut();
        }
        fill_uuid_slots_owned_int(object, int_object, SAFE_UUID_UNKNOWN);
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

#[inline(always)]
unsafe fn keyword_count(kwnames: *mut PyObject) -> Py_ssize_t {
    unsafe {
        if kwnames.is_null() {
            0
        } else {
            PyTuple_Size(kwnames)
        }
    }
}

#[inline(always)]
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
        if PyVectorcall_NARGS(nargsf) != 0 || !kwnames.is_null() {
            return call_original(callable, args, nargsf, kwnames);
        }
        allocate_uuid(uuid::Uuid::new_v4().as_u128())
    }
}

unsafe fn name_bytes(name: *mut PyObject) -> Option<&'static [u8]> {
    unsafe {
        if PyUnicode_Check(name) != 0 {
            if let Some(bytes) = ascii_unicode_bytes(name) {
                return Some(bytes);
            }
            let mut size: Py_ssize_t = 0;
            let pointer = PyUnicode_AsUTF8AndSize(name, &mut size);
            if pointer.is_null() {
                return None;
            }
            Some(std::slice::from_raw_parts(pointer.cast::<u8>(), size as usize))
        } else if PyBytes_Check(name) != 0 {
            let size = PyBytes_Size(name);
            let pointer = PyBytes_AsString(name);
            Some(std::slice::from_raw_parts(pointer.cast::<u8>(), size as usize))
        } else {
            None
        }
    }
}

unsafe fn is_uuid_instance(object: *mut PyObject) -> c_int {
    unsafe {
        if (*object).ob_type == UUID_TYPE.cast::<PyTypeObject>() {
            1
        } else {
            PyObject_IsInstance(object, UUID_TYPE)
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
        let namespace_check = is_uuid_instance(namespace);
        if namespace_check < 0 {
            return ptr::null_mut();
        }
        if namespace_check == 0 {
            return call_original(callable, args, nargsf, kwnames);
        }
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
        allocate_uuid(value)
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
) -> Option<(*mut PyObject, *mut PyObject, bool, bool)> {
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
        let mut node_supplied = positional_count >= 1;
        let mut clock_seq_supplied = positional_count >= 2;
        for index in 0..keyword_count(kwnames) {
            let value = *args.add(positional_count as usize + index as usize);
            if keyword_matches(kwnames, index, c"node".as_ptr()) {
                if node_supplied {
                    call_original(callable, args, nargsf, kwnames);
                    return None;
                }
                node_object = value;
                node_supplied = true;
            } else if keyword_matches(kwnames, index, c"clock_seq".as_ptr()) {
                if clock_seq_supplied {
                    call_original(callable, args, nargsf, kwnames);
                    return None;
                }
                clock_seq_object = value;
                clock_seq_supplied = true;
            } else {
                call_original(callable, args, nargsf, kwnames);
                return None;
            }
        }
        Some((node_object, clock_seq_object, node_supplied, clock_seq_supplied))
    }
}

unsafe fn current_getnode_borrowed() -> *mut PyObject {
    unsafe {
        let getnode = PyDict_GetItem(UUID_DICT, INTERNED_GETNODE);
        if getnode.is_null() {
            PyErr_SetString(PyExc_RuntimeError, c"uuideal: uuid.getnode is missing".as_ptr());
        }
        getnode
    }
}

unsafe fn current_node_value() -> Option<u128> {
    unsafe {
        let node = PyDict_GetItem(UUID_DICT, INTERNED_NODE);
        if node.is_null() || node == Py_None() || PyLong_Check(node) == 0 {
            return None;
        }
        pylong_to_u128(node)
    }
}

unsafe fn call_getnode_value(getnode: *mut PyObject) -> Option<u128> {
    unsafe {
        let result = PyObject_CallNoArgs(getnode);
        if result.is_null() {
            return None;
        }
        if PyLong_Check(result) == 0 {
            Py_DECREF(result);
            return None;
        }
        let value = PyLong_AsUnsignedLongLongMask(result) as u128;
        Py_DECREF(result);
        Some(value)
    }
}

fn mask_default_node_value(node: u128) -> u128 {
    node & 0xffffffffffff
}

unsafe fn validate_explicit_node_value(
    _callable: *mut PyObject,
    _args: *const *mut PyObject,
    _nargsf: usize,
    _kwnames: *mut PyObject,
    node: u128,
) -> Option<u128> {
    unsafe {
        if node < (1u128 << 48) {
            Some(node)
        } else {
            PyErr_SetString(
                PyExc_ValueError,
                c"field 6 out of range (need a 48-bit value)".as_ptr(),
            );
            None
        }
    }
}

unsafe fn default_node_slow(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> Option<u128> {
    unsafe {
        let getnode = current_getnode_borrowed();
        if getnode.is_null() {
            return None;
        }

        let Some(node) = call_getnode_value(getnode) else {
            call_original(callable, args, nargsf, kwnames);
            return None;
        };
        let node = mask_default_node_value(node);

        if getnode == ORIGINAL_GETNODE || getnode == TRUSTED_GETNODE {
            DEFAULT_NODE.store(node as u64, Ordering::Relaxed);
            DEFAULT_NODE_READY.store(true, Ordering::Relaxed);
            GETNODE_MODE.store(GETNODE_MODE_TRUSTED, Ordering::Relaxed);
            return Some(node);
        }

        if let Some(current_node) = current_node_value() {
            if mask_default_node_value(current_node) == node {
                trust_getnode_borrowed(getnode);
                DEFAULT_NODE.store(node as u64, Ordering::Relaxed);
                DEFAULT_NODE_READY.store(true, Ordering::Relaxed);
                GETNODE_MODE.store(GETNODE_MODE_TRUSTED, Ordering::Relaxed);
                return Some(node);
            }
        }

        GETNODE_MODE.store(GETNODE_MODE_UNKNOWN, Ordering::Relaxed);
        Some(node)
    }
}

#[inline(always)]
unsafe fn default_node(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> Option<u128> {
    if GETNODE_MODE.load(Ordering::Relaxed) == GETNODE_MODE_TRUSTED
        && DEFAULT_NODE_READY.load(Ordering::Relaxed)
    {
        return Some(DEFAULT_NODE.load(Ordering::Relaxed) as u128);
    }

    unsafe { default_node_slow(callable, args, nargsf, kwnames) }
}

unsafe fn resolve_node(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
    node_object: *mut PyObject,
) -> Option<u128> {
    unsafe {
        if node_object.is_null() || node_object == Py_None() {
            default_node(callable, args, nargsf, kwnames)
        } else if PyLong_Check(node_object) != 0 {
            let Some(value) = pylong_to_u128(node_object) else {
                call_original(callable, args, nargsf, kwnames);
                return None;
            };
            validate_explicit_node_value(callable, args, nargsf, kwnames, value)
        } else {
            call_original(callable, args, nargsf, kwnames);
            None
        }
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
        } else if PyLong_Check(clock_seq_object) != 0 {
            let Some(clock_seq_value) = pylong_to_u128(clock_seq_object) else {
                return call_original(callable, args, nargsf, kwnames);
            };
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
        allocate_uuid(value)
    }
}

unsafe fn uuid1_from_generate_time_safe() -> *mut PyObject {
    unsafe {
        let result = PyObject_CallNoArgs(GENERATE_TIME_SAFE);
        if result.is_null() {
            return ptr::null_mut();
        }
        if PyTuple_Check(result) == 0 || PyTuple_Size(result) != 2 {
            Py_DECREF(result);
            return ptr::null_mut();
        }
        let uuid_time = PyTuple_GetItem(result, 0);
        let safely_generated = PyTuple_GetItem(result, 1);

        let Some(value) = bytes_to_uuid_int(uuid_time, false) else {
            Py_DECREF(result);
            return ptr::null_mut();
        };

        let safely_int = PyLong_AsLong(safely_generated);
        let is_safe = if safely_int == -1 && !PyErr_Occurred().is_null() {
            PyErr_Clear();
            SAFE_UUID_UNKNOWN
        } else if safely_int == 0 {
            SAFE_UUID_SAFE
        } else if safely_int == -1 {
            SAFE_UUID_UNSAFE
        } else {
            SAFE_UUID_UNKNOWN
        };

        Py_DECREF(result);

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

unsafe extern "C" fn uuid1_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        let Some((node_object, clock_seq_object, node_supplied, clock_seq_supplied)) =
            parse_node_and_clock_seq(callable, args, nargsf, kwnames)
        else {
            return ptr::null_mut();
        };
        if !GENERATE_TIME_SAFE.is_null() {
            let node_is_none = !node_supplied || node_object.is_null() || node_object == Py_None();
            let clock_seq_is_none =
                !clock_seq_supplied || clock_seq_object.is_null() || clock_seq_object == Py_None();
            if node_is_none && clock_seq_is_none {
                return uuid1_from_generate_time_safe();
            }
        }
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
        let Some((node_object, clock_seq_object, _node_supplied, _clock_seq_supplied)) =
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
    unsafe { allocate_uuid(uuid::Uuid::now_v7().as_u128()) }
}

unsafe extern "C" fn uuid7_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 0 || !kwnames.is_null() {
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
        let bytes = std::slice::from_raw_parts(pointer, 16);
        let uuid = if little_endian {
            uuid::Uuid::from_slice_le(bytes)
        } else {
            uuid::Uuid::from_slice(bytes)
        };
        uuid.ok().map(|uuid| uuid.as_u128())
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
        let nkw = keyword_count(kwnames);

        if positional_count == 2 && nkw == 0 {
            if let Some(value) = parse_uuid_hex_pyunicode(*args.add(1)) {
                if init_uuid_slots_from_value(self_object, value) < 0 {
                    return ptr::null_mut();
                }
                return none();
            }
        }

        if positional_count == 1 && nkw == 1 {
            let kw_value = *args.add(1);
            if keyword_matches(kwnames, 0, c"hex".as_ptr()) {
                if let Some(value) = parse_uuid_hex_pyunicode(kw_value) {
                    if init_uuid_slots_from_value(self_object, value) < 0 {
                        return ptr::null_mut();
                    }
                    return none();
                }
            } else if keyword_matches(kwnames, 0, c"int".as_ptr()) {
                if pylong_to_u128(kw_value).is_some() {
                    if set_uuid_slots_from_pylong(self_object, kw_value, ptr::null_mut()) < 0 {
                        return ptr::null_mut();
                    }
                    return none();
                }
            }
        }

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

        for index in 0..nkw {
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

        let from_int = int_object != Py_None();

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
            if set_uuid_slots(self_object, value, is_safe_object) < 0 {
                return ptr::null_mut();
            }
        } else if from_int {
            if set_uuid_slots_from_pylong(self_object, int_object, is_safe_object) < 0 {
                return ptr::null_mut();
            }
        } else {
            if set_uuid_slots(self_object, value, is_safe_object) < 0 {
                return ptr::null_mut();
            }
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
        let int_object = slot_object(self_object, INT_SLOT_OFFSET);
        if int_object.is_null() {
            PyErr_SetString(PyExc_RuntimeError, c"uuideal: UUID int slot is null".as_ptr());
            return ptr::null_mut();
        }
        incref(int_object)
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
        let int_object = slot_object(self_object, INT_SLOT_OFFSET);
        if int_object.is_null() {
            PyErr_SetString(PyExc_RuntimeError, c"uuideal: UUID int slot is null".as_ptr());
            return ptr::null_mut();
        }
        let hash = PyObject_Hash(int_object);
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
        set_slot_by_offset(object, INT_SLOT_OFFSET, value_object);
        set_slot_by_offset(object, IS_SAFE_SLOT_OFFSET, SAFE_UUID_UNKNOWN);
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
const COMPARE_EQ: u8 = 0;
const COMPARE_LT: u8 = 1;
const COMPARE_GT: u8 = 2;
const COMPARE_LE: u8 = 3;
const COMPARE_GE: u8 = 4;

#[inline(always)]
unsafe fn rich_compare<const OP: u8>(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe {
        if PyVectorcall_NARGS(nargsf) != 2 || !kwnames.is_null() {
            return call_original(callable, args, nargsf, kwnames);
        }
        let self_object = *args;
        let other_object = *args.add(1);

        if (*other_object).ob_type != UUID_TYPE.cast::<PyTypeObject>() {
            let instance_check = PyObject_IsInstance(other_object, UUID_TYPE);
            if instance_check < 0 {
                return ptr::null_mut();
            }
            if instance_check == 0 {
                return not_implemented();
            }
        }

        let self_int = slot_object(self_object, INT_SLOT_OFFSET);
        let other_int = slot_object(other_object, INT_SLOT_OFFSET);

        if self_int.is_null() || other_int.is_null() {
            return call_original(callable, args, nargsf, kwnames);
        }

        if self_int == other_int {
            return py_bool(match OP {
                COMPARE_LT | COMPARE_GT => false,
                _ => true,
            });
        }

        let Some(ordering) = pylong_cmp_unsigned(self_int, other_int) else {
            return call_original(callable, args, nargsf, kwnames);
        };

        py_bool(match OP {
            COMPARE_EQ => ordering.is_eq(),
            COMPARE_LT => ordering.is_lt(),
            COMPARE_GT => ordering.is_gt(),
            COMPARE_LE => ordering.is_le(),
            _ => ordering.is_ge(),
        })
    }
}

unsafe extern "C" fn uuid_eq_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<COMPARE_EQ>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_lt_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<COMPARE_LT>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_gt_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<COMPARE_GT>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_le_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<COMPARE_LE>(callable, args, nargsf, kwnames) }
}
unsafe extern "C" fn uuid_ge_vectorcall(
    callable: *mut PyObject,
    args: *const *mut PyObject,
    nargsf: usize,
    kwnames: *mut PyObject,
) -> *mut PyObject {
    unsafe { rich_compare::<COMPARE_GE>(callable, args, nargsf, kwnames) }
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
        uuid_field_object::<FIELD>(value)
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
        let int_object = slot_object(self_object, INT_SLOT_OFFSET);
        let is_safe = slot_object(self_object, IS_SAFE_SLOT_OFFSET);
        if int_object.is_null() || is_safe.is_null() {
            PyErr_SetString(PyExc_RuntimeError, c"uuideal: UUID state slots are null".as_ptr());
            return ptr::null_mut();
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

unsafe fn resolve_safe_uuid_borrowed(is_safe_int: *mut PyObject) -> *mut PyObject {
    unsafe {
        if is_safe_int.is_null() || is_safe_int == Py_None() {
            return SAFE_UUID_UNKNOWN;
        }
        let v = PyLong_AsLong(is_safe_int);
        if v == -1 && !PyErr_Occurred().is_null() {
            PyErr_Clear();
            return SAFE_UUID_UNKNOWN;
        }
        if v == 0 {
            return SAFE_UUID_SAFE;
        }
        if v == -1 {
            return SAFE_UUID_UNSAFE;
        }
        let result = PyObject_CallOneArg(SAFE_UUID_TYPE, is_safe_int);
        if result.is_null() {
            return SAFE_UUID_UNKNOWN;
        }
        result
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
        if is_safe_value.is_null() {
            let int_slot = slot_pointer(self_object, INT_SLOT_OFFSET);
            let previous_int = *int_slot;
            Py_INCREF(int_object);
            *int_slot = int_object;
            xdecref(previous_int);
            let is_safe_slot = slot_pointer(self_object, IS_SAFE_SLOT_OFFSET);
            let previous_is_safe = *is_safe_slot;
            Py_INCREF(SAFE_UUID_UNKNOWN);
            *is_safe_slot = SAFE_UUID_UNKNOWN;
            xdecref(previous_is_safe);
            return none();
        }
        let is_safe = resolve_safe_uuid_borrowed(is_safe_value);
        let owned = !is_safe_value.is_null()
            && is_safe != SAFE_UUID_UNKNOWN
            && is_safe != SAFE_UUID_SAFE
            && is_safe != SAFE_UUID_UNSAFE;
        set_slot_by_offset(self_object, INT_SLOT_OFFSET, int_object);
        set_slot_by_offset(self_object, IS_SAFE_SLOT_OFFSET, is_safe);
        if owned {
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
                c"uuideal: unable to resolve required UUID int/is_safe slot offsets".as_ptr(),
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
        UUID_DICT = PyModule_GetDict(module);
        if UUID_DICT.is_null() {
            return -1;
        }
        UUID_TYPE = attribute(module, c"UUID".as_ptr());
        SAFE_UUID_TYPE = attribute(module, c"SafeUUID".as_ptr());
        if SAFE_UUID_TYPE.is_null() {
            return -1;
        }
        SAFE_UUID_UNKNOWN = attribute(SAFE_UUID_TYPE, c"unknown".as_ptr());
        SAFE_UUID_SAFE = attribute(SAFE_UUID_TYPE, c"safe".as_ptr());
        SAFE_UUID_UNSAFE = attribute(SAFE_UUID_TYPE, c"unsafe".as_ptr());
        if SAFE_UUID_UNKNOWN.is_null() || SAFE_UUID_SAFE.is_null() || SAFE_UUID_UNSAFE.is_null() {
            return -1;
        }
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
            INTERNED_NODE = PyUnicode_InternFromString(c"_node".as_ptr());
            INTERNED_GETNODE = PyUnicode_InternFromString(c"getnode".as_ptr());
            INTERNED_GENERATE_TIME_SAFE = PyUnicode_InternFromString(c"_generate_time_safe".as_ptr());
            if INTERNED_INT.is_null()
                || INTERNED_IS_SAFE.is_null()
                || INTERNED_VALUE.is_null()
                || INTERNED_NODE.is_null()
                || INTERNED_GETNODE.is_null()
                || INTERNED_GENERATE_TIME_SAFE.is_null()
            {
                return -1;
            }
        }
        let gts = PyDict_GetItem(UUID_DICT, INTERNED_GENERATE_TIME_SAFE);
        if gts.is_null() {
            PyErr_Clear();
            clear_generate_time_safe();
        } else {
            set_generate_time_safe_borrowed(gts);
        }
        if ORIGINAL_GETNODE.is_null() {
            ORIGINAL_GETNODE = PyDict_GetItem(UUID_DICT, INTERNED_GETNODE);
            if ORIGINAL_GETNODE.is_null() {
                PyErr_SetString(PyExc_RuntimeError, c"uuideal: uuid.getnode is missing".as_ptr());
                return -1;
            }
            Py_INCREF(ORIGINAL_GETNODE);
            GETNODE_MODE.store(GETNODE_MODE_TRUSTED, Ordering::Relaxed);
            invalidate_default_node_cache();
        }
        if install_uuid_dict_watcher() < 0 {
            return -1;
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

unsafe extern "C" fn py_install(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe {
        if !PATCHED.load(Ordering::SeqCst) {
            if register_reseed_at_fork() < 0 {
                return ptr::null_mut();
            }
            if apply_all_patches() < 0 {
                return ptr::null_mut();
            }
            PATCHED.store(true, Ordering::SeqCst);
        }
        none()
    }
}

unsafe extern "C" fn py_uninstall(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
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

unsafe extern "C" fn py_installed(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe { PyBool_FromLong(PATCHED.load(Ordering::SeqCst) as c_long) }
}

unsafe extern "C" fn py_reseed_rng(_self: *mut PyObject, _args: *mut PyObject) -> *mut PyObject {
    unsafe {
        match rand::rng().reseed() {
            Ok(()) => none(),
            Err(err) => {
                let message = match CString::new(err.to_string()) {
                    Ok(message) => message,
                    Err(_) => CString::new("failed to reseed random number generator")
                        .expect("static error message has no interior nul"),
                };
                PyErr_SetString(PyExc_OSError, message.as_ptr());
                ptr::null_mut()
            }
        }
    }
}

unsafe fn register_reseed_at_fork() -> c_int {
    unsafe {
        if AT_FORK_REGISTERED.load(Ordering::SeqCst) {
            return 0;
        }

        let os_module = PyImport_ImportModule(c"os".as_ptr());
        if os_module.is_null() {
            return -1;
        }

        let register_at_fork = PyObject_GetAttrString(os_module, c"register_at_fork".as_ptr());
        Py_DECREF(os_module);
        if register_at_fork.is_null() {
            PyErr_Clear();
            AT_FORK_REGISTERED.store(true, Ordering::SeqCst);
            return 0;
        }

        let uuideal_module = PyImport_ImportModule(c"uuideal._uuideal".as_ptr());
        if uuideal_module.is_null() {
            Py_DECREF(register_at_fork);
            return -1;
        }

        let reseed_rng = PyObject_GetAttrString(uuideal_module, c"reseed_rng".as_ptr());
        Py_DECREF(uuideal_module);
        if reseed_rng.is_null() {
            Py_DECREF(register_at_fork);
            return -1;
        }

        let args = PyTuple_New(0);
        if args.is_null() {
            Py_DECREF(reseed_rng);
            Py_DECREF(register_at_fork);
            return -1;
        }

        let kwargs = PyDict_New();
        if kwargs.is_null() {
            Py_DECREF(args);
            Py_DECREF(reseed_rng);
            Py_DECREF(register_at_fork);
            return -1;
        }

        if PyDict_SetItemString(kwargs, c"after_in_child".as_ptr(), reseed_rng) < 0 {
            Py_DECREF(kwargs);
            Py_DECREF(args);
            Py_DECREF(reseed_rng);
            Py_DECREF(register_at_fork);
            return -1;
        }

        let result = PyObject_Call(register_at_fork, args, kwargs);

        Py_DECREF(kwargs);
        Py_DECREF(args);
        Py_DECREF(reseed_rng);
        Py_DECREF(register_at_fork);

        if result.is_null() {
            return -1;
        }

        Py_DECREF(result);
        AT_FORK_REGISTERED.store(true, Ordering::SeqCst);
        0
    }
}

static mut METHODS: [PyMethodDef; 8] = [PyMethodDef::zeroed(); 8];

unsafe fn init_methods() {
    unsafe {
        METHODS[0] = PyMethodDef {
            ml_name: c"install".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_install,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Install uuid vectorcall patches.".as_ptr(),
        };
        METHODS[1] = PyMethodDef {
            ml_name: c"uninstall".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_uninstall,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Uninstall uuid vectorcall patches.".as_ptr(),
        };
        METHODS[2] = PyMethodDef {
            ml_name: c"installed".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_installed,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Return whether uuid vectorcall patches are installed.".as_ptr(),
        };
        METHODS[3] = PyMethodDef {
            ml_name: c"uuid6".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunctionFastWithKeywords: py_uuid6,
            },
            ml_flags: METH_FASTCALL | METH_KEYWORDS,
            ml_doc: c"Generate a version 6 UUID.".as_ptr(),
        };
        METHODS[4] = PyMethodDef {
            ml_name: c"uuid7".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunctionFastWithKeywords: py_uuid7,
            },
            ml_flags: METH_FASTCALL | METH_KEYWORDS,
            ml_doc: c"Generate a version 7 UUID.".as_ptr(),
        };
        METHODS[5] = PyMethodDef {
            ml_name: c"reseed_rng".as_ptr(),
            ml_meth: PyMethodDefPointer {
                PyCFunction: py_reseed_rng,
            },
            ml_flags: METH_NOARGS,
            ml_doc: c"Reseed the Rust random number generator.".as_ptr(),
        };
        METHODS[7] = PyMethodDef::zeroed();
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
        if load_uuid_references() < 0 {
            return ptr::null_mut();
        }
        PyModuleDef_Init(ptr::addr_of_mut!(MODULE_DEF))
    }
}
