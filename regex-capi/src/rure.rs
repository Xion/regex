use ::error::{Error, ErrorKind};

use ::regex::bytes;
use ::regex::internal::{Exec, ExecBuilder, RegexOptions};
use ::regex::internal::RegularExpression;
use ::libc::{c_char, size_t};

use ::std::collections::HashMap;
use ::std::ops::Deref;
use ::std::ffi::{CStr, CString};
use ::std::ptr;
use ::std::str;
use ::std::slice;


pub struct Regex {
    re: bytes::Regex,
    capture_names: HashMap<String, i32>,
}

pub struct Options {
    size_limit: usize,
    dfa_size_limit: usize,
}

// The `RegexSet` is not exposed with option support or matching at an
// arbitrary position with a crate just yet. To circumvent this, we use
// the `Exec` structure directly.
pub struct RegexSet {
    re: Exec,
    pattern_count: usize
}

const RURE_FLAG_CASEI: u32 = 1 << 0;
const RURE_FLAG_MULTI: u32 = 1 << 1;
const RURE_FLAG_DOTNL: u32 = 1 << 2;
const RURE_FLAG_SWAP_GREED: u32 = 1 << 3;
const RURE_FLAG_SPACE: u32 = 1 << 4;
const RURE_FLAG_UNICODE: u32 = 1 << 5;
const RURE_DEFAULT_FLAGS: u32 = RURE_FLAG_UNICODE;


#[repr(C)]
pub struct rure_match {
    pub start: size_t,
    pub end: size_t,
}

pub struct Captures(Vec<Option<usize>>);

pub struct Iter {
    re: *const Regex,
    last_end: usize,
    last_match: Option<usize>,
}

pub struct IterCaptureNames {
    capture_names: bytes::CaptureNames<'static>,
    name_ptrs: Vec<*mut c_char>,
}

impl Deref for Regex {
    type Target = bytes::Regex;
    fn deref(&self) -> &bytes::Regex { &self.re }
}

impl Deref for RegexSet {
    type Target = Exec;
    fn deref(&self) -> &Exec { &self.re }
}

impl Default for Options {
    fn default() -> Options {
        Options {
            size_limit: 10 * (1<<20),
            dfa_size_limit: 2 * (1<<20),
        }
    }
}

ffi_fn! {
    fn rure_compile_must(pattern: *const c_char) -> *const Regex {
        let len = unsafe { CStr::from_ptr(pattern).to_bytes().len() };
        let pat = pattern as *const u8;
        let mut err = Error::new(ErrorKind::None);
        let re = rure_compile(
            pat, len, RURE_DEFAULT_FLAGS, ptr::null(), &mut err);
        if err.is_err() {
            let _ = writeln!(&mut io::stderr(), "{}", err);
            let _ = writeln!(
                &mut io::stderr(), "aborting from rure_compile_must");
            unsafe { abort() }
        }
        re
    }
}

ffi_fn! {
    fn rure_compile(
        pattern: *const u8,
        length: size_t,
        flags: u32,
        options: *const Options,
        error: *mut Error,
    ) -> *const Regex {
        let pat = unsafe { slice::from_raw_parts(pattern, length) };
        let pat = match str::from_utf8(pat) {
            Ok(pat) => pat,
            Err(err) => {
                unsafe {
                    if !error.is_null() {
                        *error = Error::new(ErrorKind::Str(err));
                    }
                    return ptr::null();
                }
            }
        };
        let mut builder = bytes::RegexBuilder::new(pat);
        if !options.is_null() {
            let options = unsafe { &*options };
            builder = builder.size_limit(options.size_limit);
            builder = builder.dfa_size_limit(options.dfa_size_limit);
        }
        builder = builder.case_insensitive(flags & RURE_FLAG_CASEI > 0);
        builder = builder.multi_line(flags & RURE_FLAG_MULTI > 0);
        builder = builder.dot_matches_new_line(flags & RURE_FLAG_DOTNL > 0);
        builder = builder.swap_greed(flags & RURE_FLAG_SWAP_GREED > 0);
        builder = builder.ignore_whitespace(flags & RURE_FLAG_SPACE > 0);
        builder = builder.unicode(flags & RURE_FLAG_UNICODE > 0);
        match builder.compile() {
            Ok(re) => {
                let mut capture_names = HashMap::new();
                for (i, name) in re.capture_names().enumerate() {
                    if let Some(name) = name {
                        capture_names.insert(name.to_owned(), i as i32);
                    }
                }
                let re = Regex {
                    re: re,
                    capture_names: capture_names,
                };
                Box::into_raw(Box::new(re))
            }
            Err(err) => {
                unsafe {
                    if !error.is_null() {
                        *error = Error::new(ErrorKind::Regex(err));
                    }
                    ptr::null()
                }
            }
        }
    }
}

ffi_fn! {
    fn rure_free(re: *const Regex) {
        unsafe { Box::from_raw(re as *mut Regex); }
    }
}

ffi_fn! {
    fn rure_is_match(
        re: *const Regex,
        haystack: *const u8,
        len: size_t,
        start: size_t,
    ) -> bool {
        let re = unsafe { &*re };
        let haystack = unsafe { slice::from_raw_parts(haystack, len) };
        re.is_match_at(haystack, start)
    }
}

ffi_fn! {
    fn rure_find(
        re: *const Regex,
        haystack: *const u8,
        len: size_t,
        start: size_t,
        match_info: *mut rure_match,
    ) -> bool {
        let re = unsafe { &*re };
        let haystack = unsafe { slice::from_raw_parts(haystack, len) };
        re.find_at(haystack, start).map(|(s, e)| unsafe {
            if !match_info.is_null() {
                (*match_info).start = s;
                (*match_info).end = e;
            }
        }).is_some()
    }
}

ffi_fn! {
    fn rure_find_captures(
        re: *const Regex,
        haystack: *const u8,
        len: size_t,
        start: size_t,
        captures: *mut Captures,
    ) -> bool {
        let re = unsafe { &*re };
        let haystack = unsafe { slice::from_raw_parts(haystack, len) };
        let slots = unsafe { &mut (*captures).0 };
        re.read_captures_at(slots, haystack, start).is_some()
    }
}

ffi_fn! {
    fn rure_shortest_match(
        re: *const Regex,
        haystack: *const u8,
        len: size_t,
        start: size_t,
        end: *mut usize,
    ) -> bool {
        let re = unsafe { &*re };
        let haystack = unsafe { slice::from_raw_parts(haystack, len) };
        match re.shortest_match_at(haystack, start) {
            None => false,
            Some(i) => {
                if !end.is_null() {
                    unsafe {
                        *end = i;
                    }
                }
                true
            }
        }
    }
}

ffi_fn! {
    fn rure_capture_name_index(
        re: *const Regex,
        name: *const c_char,
    ) -> i32 {
        let re = unsafe { &*re };
        let name = unsafe { CStr::from_ptr(name) };
        let name = match name.to_str() {
            Err(_) => return -1,
            Ok(name) => name,
        };
        re.capture_names.get(name).map(|&i|i).unwrap_or(-1)
    }
}

ffi_fn! {
    fn rure_iter_capture_names_new(
        re: *const Regex,
    ) -> *mut IterCaptureNames {
        let re = unsafe { &*re };
        Box::into_raw(Box::new(IterCaptureNames {
            capture_names: re.re.capture_names(),
            name_ptrs: Vec::new(),
        }))
    }
}

ffi_fn! {
    fn rure_iter_capture_names_free(it: *mut IterCaptureNames) {
        unsafe {
            let it = &mut *it;
            while let Some(ptr) = it.name_ptrs.pop(){
                CString::from_raw(ptr);
            }
            Box::from_raw(it);
        }
    }
}

ffi_fn! {
    fn rure_iter_capture_names_next(
        it: *mut IterCaptureNames,
        capture_name: *mut *mut c_char,
    ) -> bool {
        if capture_name.is_null() {
            return false;
        }

        let it = unsafe { &mut *it };
        let cn = match it.capture_names.next() {
            // Top-level iterator ran out of capture groups
            None => return false,
            Some(val) => {
                let name = match val {
                    // inner Option didn't have a name
                    None => "",
                    Some(name) => name
                };
                name
            }
        };

        unsafe {
            let cs = match CString::new(cn.as_bytes()) {
                Result::Ok(val) => val,
                Result::Err(_) => return false
            };
            let ptr = cs.into_raw();
            it.name_ptrs.push(ptr);
            *capture_name = ptr;
        }
        true

    }
}

ffi_fn! {
    fn rure_iter_new(
        re: *const Regex,
    ) -> *mut Iter {
        Box::into_raw(Box::new(Iter {
            re: re,
            last_end: 0,
            last_match: None,
        }))
    }
}

ffi_fn! {
    fn rure_iter_free(it: *mut Iter) {
        unsafe { Box::from_raw(it); }
    }
}

ffi_fn! {
    fn rure_iter_next(
        it: *mut Iter,
        haystack: *const u8,
        len: size_t,
        match_info: *mut rure_match,
    ) -> bool {
        let it = unsafe { &mut *it };
        let re = unsafe { &*it.re };
        let text = unsafe { slice::from_raw_parts(haystack, len) };
        if it.last_end > text.len() {
            return false;
        }
        let (s, e) = match re.find_at(text, it.last_end) {
            None => return false,
            Some((s, e)) => (s, e),
        };
        if s == e {
            // This is an empty match. To ensure we make progress, start
            // the next search at the smallest possible starting position
            // of the next match following this one.
            it.last_end += 1;
            // Don't accept empty matches immediately following a match.
            // Just move on to the next match.
            if Some(e) == it.last_match {
                return rure_iter_next(it, haystack, len, match_info);
            }
        } else {
            it.last_end = e;
        }
        it.last_match = Some(e);
        if !match_info.is_null() {
            unsafe {
                (*match_info).start = s;
                (*match_info).end = e;
            }
        }
        true
    }
}

ffi_fn! {
    fn rure_iter_next_captures(
        it: *mut Iter,
        haystack: *const u8,
        len: size_t,
        captures: *mut Captures,
    ) -> bool {
        let it = unsafe { &mut *it };
        let re = unsafe { &*it.re };
        let slots = unsafe { &mut (*captures).0 };
        let text = unsafe { slice::from_raw_parts(haystack, len) };
        if it.last_end > text.len() {
            return false;
        }
        let (s, e) = match re.read_captures_at(slots, text, it.last_end) {
            None => return false,
            Some((s, e)) => (s, e),
        };
        if s == e {
            // This is an empty match. To ensure we make progress, start
            // the next search at the smallest possible starting position
            // of the next match following this one.
            it.last_end += 1;
            // Don't accept empty matches immediately following a match.
            // Just move on to the next match.
            if Some(e) == it.last_match {
                return rure_iter_next_captures(it, haystack, len, captures);
            }
        } else {
            it.last_end = e;
        }
        it.last_match = Some(e);
        true
    }
}

ffi_fn! {
    fn rure_captures_new(re: *const Regex) -> *mut Captures {
        let re = unsafe { &*re };
        let captures = Captures(vec![None; 2 * re.captures_len()]);
        Box::into_raw(Box::new(captures))
    }
}

ffi_fn! {
    fn rure_captures_free(captures: *const Captures) {
        unsafe { Box::from_raw(captures as *mut Captures); }
    }
}

ffi_fn! {
    fn rure_captures_at(
        captures: *const Captures,
        i: size_t,
        match_info: *mut rure_match,
    ) -> bool {
        let captures = unsafe { &(*captures).0 };
        match (captures[i * 2], captures[i * 2 + 1]) {
            (Some(start), Some(end)) => {
                if !match_info.is_null() {
                    unsafe {
                        (*match_info).start = start;
                        (*match_info).end = end;
                    }
                }
                true
            }
            _ => false
        }
    }
}

ffi_fn! {
    fn rure_captures_len(captures: *const Captures) -> size_t {
        unsafe { (*captures).0.len() / 2 }
    }
}

ffi_fn! {
    fn rure_options_new() -> *mut Options {
        Box::into_raw(Box::new(Options::default()))
    }
}

ffi_fn! {
    fn rure_options_free(options: *mut Options) {
        unsafe { Box::from_raw(options); }
    }
}

ffi_fn! {
    fn rure_options_size_limit(options: *mut Options, limit: size_t) {
        let options = unsafe { &mut *options };
        options.size_limit = limit;
    }
}

ffi_fn! {
    fn rure_options_dfa_size_limit(options: *mut Options, limit: size_t) {
        let options = unsafe { &mut *options };
        options.dfa_size_limit = limit;
    }
}

ffi_fn! {
    fn rure_compile_set(
        patterns: *const *const u8,
        patterns_lengths: *const size_t,
        patterns_count: size_t,
        flags: u32,
        options: *const Options,
        error: *mut Error
    ) -> *const RegexSet {
        let (raw_pats, raw_patsl) = unsafe {
            (
                slice::from_raw_parts(patterns, patterns_count),
                slice::from_raw_parts(patterns_lengths, patterns_count)
            )
        };

        let mut pats = Vec::with_capacity(patterns_count);
        for (&raw_pat, &raw_patl) in raw_pats.iter().zip(raw_patsl) {
            let pat = unsafe { slice::from_raw_parts(raw_pat, raw_patl) };
            pats.push(match str::from_utf8(pat) {
                Ok(pat) => pat,
                Err(err) => {
                    unsafe {
                        if !error.is_null() {
                            *error = Error::new(ErrorKind::Str(err));
                        }
                        return ptr::null();
                    }
                }
            });
        }

        // Start with a default set and override values if present.
        let mut opts = RegexOptions::default();
        let pat_count = pats.len();
        opts.pats = pats.into_iter().map(|s| s.to_owned()).collect();

        if !options.is_null() {
            let options = unsafe { &*options };
            opts.size_limit = options.size_limit;
            opts.dfa_size_limit = options.dfa_size_limit;
        }

        opts.case_insensitive = flags & RURE_FLAG_CASEI > 0;
        opts.multi_line = flags & RURE_FLAG_MULTI > 0;
        opts.dot_matches_new_line = flags & RURE_FLAG_DOTNL > 0;
        opts.swap_greed = flags & RURE_FLAG_SWAP_GREED > 0;
        opts.ignore_whitespace = flags & RURE_FLAG_SPACE > 0;
        opts.unicode = flags & RURE_FLAG_UNICODE > 0;

        // `Exec` does not expose a `new` function with appropriate arguments
        // so we construct directly.
        let builder = ExecBuilder::new_options(opts)
                                    .bytes(true)
                                    .only_utf8(false);

        match builder.build() {
            Ok(ex) => {
                let re = RegexSet {
                    re: ex,
                    pattern_count: pat_count
                };
                Box::into_raw(Box::new(re))
            }
            Err(err) => {
                unsafe {
                    if !error.is_null() {
                        *error = Error::new(ErrorKind::Regex(err))
                    }
                    ptr::null()
                }
            }
        }
    }
}

ffi_fn! {
    fn rure_set_free(re: *const RegexSet) {
        unsafe { Box::from_raw(re as *mut RegexSet); }
    }
}

ffi_fn! {
    fn rure_set_is_match(
        re: *const RegexSet,
        haystack: *const u8,
        len: size_t,
        start: size_t
    ) -> bool {
        let re = unsafe { &*re };
        let haystack = unsafe { slice::from_raw_parts(haystack, len) };
        re.searcher().is_match_at(haystack, start)
    }
}

ffi_fn! {
    fn rure_set_matches(
        re: *const RegexSet,
        haystack: *const u8,
        len: size_t,
        start: size_t,
        matches: *mut bool
    ) -> bool {
        let re = unsafe { &*re };
        let mut matches = unsafe {
            slice::from_raw_parts_mut(matches, re.pattern_count)
        };
        let haystack = unsafe { slice::from_raw_parts(haystack, len) };

        // many_matches_at isn't guaranteed to set non-matches to false
        for item in matches.iter_mut() {
            *item = false;
        }

        re.searcher().many_matches_at(&mut matches, haystack, start)
    }
}

ffi_fn! {
    fn rure_set_len(re: *const RegexSet) -> size_t {
        unsafe { (*re).pattern_count }
    }
}
